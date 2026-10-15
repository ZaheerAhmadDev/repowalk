//! v2 learn pipeline: turn a repo into `(Index, Analysis)` in ordered phases,
//! with an incremental per-file cache so a re-learn only re-analyzes what
//! changed (F1 "Repo 101", F10 "fast & incremental indexing").
//!
//! Phases: `walk` → `parse` → `resolve` → `analyze` → `cache`. The
//! `parse`/`resolve` half is delegated to [`crate::pipeline`] (the single source
//! of truth for resolve/timestamp logic); `learn` keeps the walk result itself
//! so the incremental stamps below always see the *fresh* file list, even when
//! `index.json` was served from cache.
//!
//! # Incremental cache (F10)
//!
//! Each file gets a cheap stamp of `(mtime, line count)` — exactly the two
//! fields [`crate::model::FileEntry`] already carries — persisted next to the
//! analysis in `<root>/.repowalk/analysis-stamps.json`. It lives in a **sibling
//! file** rather than inside `analysis.json` because:
//!   * `analysis.json` is a shared, version-checked consumer contract
//!     ([`crate::analysis`]); stamps are pipeline-internal bookkeeping, and
//!     widening [`Analysis`] would force every reader/test to change;
//!   * stamps can be rewritten or discarded without touching the artifact the
//!     GUI/CLI read, and the frozen `index.json` schema is untouched.
//!
//! On a re-learn the per-file entries (`variables`/`cfgs`/`types`/`uses`) of
//! files whose stamp is unchanged are reused verbatim from the loaded
//! `analysis.json`; changed/added files are re-analyzed; files that vanished
//! drop out. [`reanalyzed`] reports how many files the last `learn` re-analyzed
//! (F10's "observable in logs/UI").
//!
//! # Repo-level module graph
//!
//! After the per-file analyze stage this driver runs the repo-level module
//! builder ([`crate::modules::build`]) and stores its output in
//! [`Analysis::modules`]/[`Analysis::module_edges`] before `analysis::save`.
//! `modules::build` is already deterministic and sorted, so `learn` stores its
//! result verbatim (no re-sort/filter); a warm re-learn over an unchanged index
//! therefore reproduces the identical graph. This step is folded into the
//! existing `analyze`→`cache` boundary — it adds no sixth phase.
use crate::analysis::Analysis;
use crate::model::{FileEntry, Index, Symbol};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// Current on-disk stamp-cache schema (independent of `ANALYSIS_VERSION`).
const STAMP_VERSION: u32 = 1;

/// Progress event for the UI (F1). `done`/`total` are per-phase: `done` starts
/// at 0 and ends at `total` for every phase. The four counts are running
/// totals over the whole analysis (they only grow during `analyze`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LearnProgress {
    pub phase: &'static str,
    pub done: usize,
    pub total: usize,
    pub variables: usize,
    pub cfgs: usize,
    pub types: usize,
    pub uses: usize,
}

/// One file's incremental stamp: mtime (seconds) + line count.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Stamp {
    mtime: u64,
    lines: usize,
}

/// The `.repowalk/analysis-stamps.json` payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct StampFile {
    version: u32,
    /// Number of files (re)analyzed by the most recent [`learn`] call.
    reanalyzed: usize,
    /// `repo-relative path -> (mtime, lines)` for every file at that call.
    stamps: BTreeMap<String, Stamp>,
}

/// `<root>/.repowalk/analysis-stamps.json` — the incremental stamp cache.
pub fn stamps_path(root: &Path) -> PathBuf {
    root.join(".repowalk").join("analysis-stamps.json")
}

/// Number of files the most recent [`learn`] re-analyzed (F10 observable).
/// `0` when no learn has run yet (or the stamp cache is unreadable).
pub fn reanalyzed(root: &Path) -> usize {
    load_stamps(root).map(|s| s.reanalyzed).unwrap_or(0)
}

/// The v2 analysis artifact from cache only — **never** re-analyzes anything.
/// `None` when no (version-matching) `analysis.json` exists yet. This is what
/// the GUI/CLI read after a learn.
pub fn analysis_for(root: &Path) -> Option<Analysis> {
    crate::analysis::load(root)
}

/// Learn `root`: walk → parse → resolve → analyze → cache, returning the frozen
/// [`Index`] plus the v2 [`Analysis`]. `progress` is called at every phase
/// boundary and once per file during `parse` and `analyze`.
///
/// Reuses the cached analysis for unchanged files; only changed/added files are
/// re-analyzed. A missing/unreadable `root` is an `Err` (never a panic).
pub fn learn(root: &Path, progress: &dyn Fn(LearnProgress)) -> anyhow::Result<(Index, Analysis)> {
    let empty = Analysis::new();

    // ---- walk -------------------------------------------------------------
    emit(progress, "walk", 0, 0, &empty);
    let files = crate::walk::walk(root)?;
    emit(progress, "walk", files.len(), files.len(), &empty);

    // Fresh stamps. We keep our own `files` for this: `index.json` may be a
    // cache hit carrying stale line counts.
    let current: BTreeMap<String, Stamp> = files
        .iter()
        .map(|f| {
            (
                f.path.clone(),
                Stamp {
                    mtime: f.mtime,
                    lines: f.lines,
                },
            )
        })
        .collect();

    // ---- parse + resolve (delegated to the pipeline) ----------------------
    let idx =
        crate::pipeline::build_index_from_files(root, files.clone(), &|phase, done, total| {
            emit(progress, phase, done, total, &empty);
        })?;

    // ---- analyze (incremental) -------------------------------------------
    let old_stamps: BTreeMap<String, Stamp> =
        load_stamps(root).map(|s| s.stamps).unwrap_or_default();

    // A file needs re-analysis when it is analyzable and its stamp changed (or
    // it is new). A file is "stale" — its cached entries must be dropped — when
    // it changed, or when it is not analyzable any more (e.g. extension swap).
    let needs: Vec<&FileEntry> = files
        .iter()
        .filter(|f| {
            crate::analyze::Lang::from_walk(&f.lang).is_some()
                && old_stamps.get(f.path.as_str()) != current.get(f.path.as_str())
        })
        .collect();
    let stale: BTreeSet<String> = files
        .iter()
        .filter(|f| {
            crate::analyze::Lang::from_walk(&f.lang).is_none()
                || old_stamps.get(f.path.as_str()) != current.get(f.path.as_str())
        })
        .map(|f| f.path.clone())
        .collect();

    // Seed from the cached artifact, keeping only entries for files that still
    // exist and did not change. Only the per-file stages are seeded here; the
    // repo-level module graph is recomputed below (it is not per-file).
    let mut analysis = Analysis::new();
    if let Some(old) = crate::analysis::load(root) {
        let keep = |file: &str| current.contains_key(file) && !stale.contains(file);
        analysis.variables = old
            .variables
            .into_iter()
            .filter(|v| keep(&v.file))
            .collect();
        analysis.cfgs = old
            .cfgs
            .into_iter()
            .filter(|c| keep(&symbol_file(&c.symbol)))
            .collect();
        analysis.types = old.types.into_iter().filter(|t| keep(&t.file)).collect();
        analysis.type_edges = old
            .type_edges
            .into_iter()
            .filter(|e| keep(&symbol_file(&e.from)))
            .collect();
        analysis.uses = old.uses.into_iter().filter(|u| keep(&u.file)).collect();
    }

    // Symbols grouped by file so each analyzer sees only its own file's symbols.
    let mut symbols_by_file: BTreeMap<String, Vec<Symbol>> = BTreeMap::new();
    for s in &idx.symbols {
        symbols_by_file
            .entry(s.file.clone())
            .or_default()
            .push(s.clone());
    }

    let total = needs.len();
    emit(progress, "analyze", 0, total, &analysis);
    for (i, f) in needs.iter().enumerate() {
        let Some(lang) = crate::analyze::Lang::from_walk(&f.lang) else {
            continue;
        };
        let source = std::fs::read_to_string(root.join(f.path.as_str()))?;
        let symbols = symbols_by_file
            .get(f.path.as_str())
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        crate::analyze::analyze(f.path.as_str(), lang, &source, symbols, &mut analysis);
        emit(progress, "analyze", i + 1, total, &analysis);
    }

    // ---- modules (repo-level, deterministic) ------------------------------
    // Derived from the frozen index; `modules::build` is already sorted, so the
    // result is stored verbatim. Folded into the analyze→cache boundary.
    let (modules, module_edges) = crate::modules::build(&idx);
    analysis.modules = modules;
    analysis.module_edges = module_edges;

    // ---- cache ------------------------------------------------------------
    emit(progress, "cache", 0, 1, &analysis);
    crate::analysis::save(root, &analysis)?;
    save_stamps(
        root,
        &StampFile {
            version: STAMP_VERSION,
            reanalyzed: total,
            stamps: current,
        },
    )?;
    emit(progress, "cache", 1, 1, &analysis);

    Ok((idx, analysis))
}

/// Emit a [`LearnProgress`] snapshotting the running analysis counts.
fn emit(
    progress: &dyn Fn(LearnProgress),
    phase: &'static str,
    done: usize,
    total: usize,
    analysis: &Analysis,
) {
    progress(LearnProgress {
        phase,
        done,
        total,
        variables: analysis.variables.len(),
        cfgs: analysis.cfgs.len(),
        types: analysis.types.len(),
        uses: analysis.uses.len(),
    });
}

/// Map a stable id (`file:line` or `file:name`) back to its file.
fn symbol_file(id: &str) -> String {
    id.rsplit_once(':')
        .map(|(file, _)| file.to_string())
        .unwrap_or_else(|| id.to_string())
}

fn load_stamps(root: &Path) -> Option<StampFile> {
    let bytes = std::fs::read(stamps_path(root)).ok()?;
    let stamps: StampFile = serde_json::from_slice(&bytes).ok()?;
    (stamps.version == STAMP_VERSION).then_some(stamps)
}

fn save_stamps(root: &Path, stamps: &StampFile) -> anyhow::Result<()> {
    let path = stamps_path(root);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec(stamps)?)?;
    std::fs::rename(&tmp, &path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::{ModuleEdgeKind, ModuleKind};
    use std::cell::RefCell;

    fn temp_root(tag: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("rw-learn-test-{}-{}", std::process::id(), tag));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    fn write_fixture(root: &Path) {
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(
            root.join("src/main.rs"),
            "fn main() {\n    let x = 1;\n    println!(\"{}\", x);\n}\n",
        )
        .unwrap();
        std::fs::write(
            root.join("src/tool.py"),
            "def run():\n    y = 2\n    return y\n",
        )
        .unwrap();
    }

    /// Run learn and return every progress event it emitted.
    fn learn_events(root: &Path) -> Vec<LearnProgress> {
        let seen = RefCell::new(Vec::new());
        learn(root, &|p| seen.borrow_mut().push(p)).unwrap();
        seen.into_inner()
    }

    fn analyze_done(events: &[LearnProgress]) -> usize {
        events
            .iter()
            .filter(|p| p.phase == "analyze")
            .map(|p| p.done)
            .max()
            .unwrap_or(0)
    }

    #[test]
    fn learns_fixture_and_writes_analysis_artifact() {
        let root = temp_root("basic");
        write_fixture(&root);

        assert!(
            analysis_for(&root).is_none(),
            "analysis_for must be None before the first learn"
        );

        let (idx, analysis) = learn(&root, &|_| {}).unwrap();

        assert!(
            crate::analysis::analysis_path(&root).exists(),
            "learn must write analysis.json"
        );
        assert_eq!(analysis.version, crate::analysis::ANALYSIS_VERSION);
        assert_eq!(analysis_for(&root).unwrap(), analysis);
        assert!(
            !analysis.variables.is_empty()
                || !analysis.cfgs.is_empty()
                || !analysis.uses.is_empty(),
            "learn must produce a non-empty analysis for the fixture"
        );
        assert!(idx.files.len() >= 2, "both fixture files are indexed");
        assert!(
            reanalyzed(&root) >= 1,
            "cold learn analyzed at least one file"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn phases_run_in_order_and_end_at_totals() {
        let root = temp_root("phases");
        write_fixture(&root);
        let events = learn_events(&root);

        let order = ["walk", "parse", "resolve", "analyze", "cache"];
        let mut previous = -1i64;
        for phase in order {
            let idx = events
                .iter()
                .position(|p| p.phase == phase)
                .unwrap_or_else(|| panic!("phase {phase} never reported"));
            assert!(idx as i64 > previous, "phase {phase} out of order");
            previous = idx as i64;
        }
        for phase in order {
            let last = events.iter().rfind(|p| p.phase == phase).unwrap();
            assert_eq!(last.done, last.total, "phase {phase} must end at its total");
        }
        let done: Vec<usize> = events
            .iter()
            .filter(|p| p.phase == "analyze")
            .map(|p| p.done)
            .collect();
        assert!(done.windows(2).all(|w| w[0] <= w[1]), "analyze monotonic");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn re_learn_reuses_cache_and_reanalyzes_nothing() {
        let root = temp_root("cached");
        write_fixture(&root);

        let (_, first) = learn(&root, &|_| {}).unwrap();
        assert!(reanalyzed(&root) >= 1);

        let events = learn_events(&root);
        assert_eq!(
            analyze_done(&events),
            0,
            "unchanged files must not be re-analyzed"
        );
        assert_eq!(reanalyzed(&root), 0, "no file re-analyzed on re-learn");
        assert_eq!(
            analysis_for(&root).unwrap(),
            first,
            "re-learn returns the same data"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn touching_one_file_reanalyzes_exactly_that_file() {
        let root = temp_root("one-touch");
        write_fixture(&root);
        learn(&root, &|_| {}).unwrap();
        let before = analysis_for(&root).unwrap();

        // Rewrite main.rs with a different line count so the stamp differs even
        // if the (second-resolution) mtime is unchanged.
        std::fs::write(
            root.join("src/main.rs"),
            "fn main() {\n    let x = 1;\n    let y = 2;\n    println!(\"{}\", x + y);\n}\n",
        )
        .unwrap();

        let events = learn_events(&root);
        assert_eq!(analyze_done(&events), 1, "one file re-analyzed");
        assert_eq!(reanalyzed(&root), 1, "reanalyzed() reports exactly one");

        // The untouched file's cached entries survive verbatim.
        let after = analysis_for(&root).unwrap();
        let py_before: Vec<_> = before
            .variables
            .iter()
            .filter(|v| v.file == "src/tool.py")
            .cloned()
            .collect();
        let py_after: Vec<_> = after
            .variables
            .iter()
            .filter(|v| v.file == "src/tool.py")
            .cloned()
            .collect();
        assert_eq!(py_before, py_after, "unchanged file reused from cache");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn missing_root_is_err_not_panic() {
        let root = temp_root("missing").join("does-not-exist");
        assert!(learn(&root, &|_| {}).is_err());
    }

    /// `fixtures/sample` is the real repo fixture (flat layout, one crate).
    fn sample_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/sample")
            .canonicalize()
            .expect("fixtures/sample must exist")
    }

    #[test]
    fn learns_sample_and_populates_modules() {
        let root = sample_root();
        let (idx, analysis) = learn(&root, &|_| {}).unwrap();

        assert!(
            !analysis.modules.is_empty(),
            "learn must populate Analysis.modules"
        );
        assert!(
            analysis
                .module_edges
                .iter()
                .any(|e| e.kind == ModuleEdgeKind::Submodule),
            "the tree must carry at least one Submodule edge"
        );
        for f in &idx.files {
            assert!(
                analysis
                    .modules
                    .iter()
                    .any(|n| n.kind == ModuleKind::File && n.id == f.path),
                "missing File node for indexed source {}",
                f.path
            );
        }

        // The artifact round-trips the graph exactly (save → analysis_for/load).
        let loaded = analysis_for(&root).expect("saved analysis loads");
        assert_eq!(loaded.modules, analysis.modules);
        assert_eq!(loaded.module_edges, analysis.module_edges);
        assert_eq!(loaded, analysis);
    }

    #[test]
    fn learns_two_dir_fixture_with_crate_dir_file_shape() {
        let root = temp_root("two-dir");
        std::fs::create_dir_all(root.join("src/sub")).unwrap();
        std::fs::write(root.join("src/alpha.rs"), "pub fn alpha() {}\n").unwrap();
        std::fs::write(root.join("src/beta.rs"), "pub fn beta() {}\n").unwrap();
        std::fs::write(root.join("src/sub/gamma.rs"), "pub fn gamma() {}\n").unwrap();

        let (_, analysis) = learn(&root, &|_| {}).unwrap();

        let count = |kind: ModuleKind| analysis.modules.iter().filter(|n| n.kind == kind).count();
        assert_eq!(count(ModuleKind::Crate), 1, "one root crate");
        assert_eq!(count(ModuleKind::Dir), 2, "src + src/sub are Dir nodes");
        assert_eq!(count(ModuleKind::Module), 0, "no module-root file");
        assert_eq!(count(ModuleKind::File), 3, "one File node per source");

        let has_submodule = |from: &str, to: &str| {
            analysis
                .module_edges
                .iter()
                .any(|e| e.kind == ModuleEdgeKind::Submodule && e.from == from && e.to == to)
        };
        assert!(has_submodule("src", "src/sub"));
        assert!(has_submodule("src", "src/alpha.rs"));
        assert!(has_submodule("src/sub", "src/sub/gamma.rs"));
    }

    #[test]
    fn empty_repo_yields_empty_modules() {
        let root = temp_root("empty-modules");
        let (idx, analysis) = learn(&root, &|_| {}).unwrap();
        assert!(idx.files.is_empty());
        assert!(analysis.modules.is_empty(), "no sources ⇒ no module nodes");
        assert!(
            analysis.module_edges.is_empty(),
            "no sources ⇒ no module edges"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn warm_relearn_reproduces_modules_and_reanalyzes_nothing() {
        let root = temp_root("modules-warm");
        write_fixture(&root);

        let (_, first) = learn(&root, &|_| {}).unwrap();
        assert!(!first.modules.is_empty(), "cold learn builds the tree");

        let seen = RefCell::new(Vec::new());
        let (_, second) = learn(&root, &|p| seen.borrow_mut().push(p)).unwrap();
        assert_eq!(
            analyze_done(&seen.into_inner()),
            0,
            "warm re-learn must re-analyze 0 files"
        );
        assert_eq!(second.modules, first.modules, "warm modules identical");
        assert_eq!(
            second.module_edges, first.module_edges,
            "warm module edges identical"
        );

        let _ = std::fs::remove_dir_all(&root);
    }
}
