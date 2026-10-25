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
//! Each file gets a cheap stamp of `(mtime in nanoseconds, byte size)` —
//! persisted next to the analysis in `<root>/.repowalk/analysis-stamps.json`.
//! Nanosecond mtime catches two edits inside one whole second, and byte size
//! catches a `git checkout` / `touch -r` that preserved the mtime (R-14); the
//! old `(seconds, line count)` stamp let both through as a false hit. It lives
//! in a **sibling file** rather than inside `analysis.json` because:
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
//! # Producer revision
//!
//! Per-file stamps only catch *input* staleness. When `learn()` itself begins
//! producing more or different output, every pre-existing cache would still
//! look like a hit. [`LEARN_REVISION`] is therefore stored at the top level of
//! the sidecar and must match for any per-file hit; a missing, unparsable, or
//! different revision re-analyzes everything and rewrites the sidecar. This is
//! a cache key, not a schema change: `ANALYSIS_VERSION` and `analysis.json`
//! are untouched.
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
use crate::parallel::FileJob;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

/// Current on-disk stamp-cache schema (independent of `ANALYSIS_VERSION`).
///
/// Version 2 replaced the `(seconds mtime, line count)` stamp with
/// `(nanosecond mtime, byte size)` so a same-second edit or a preserved-mtime
/// size change is a miss (R-14); a version-1 sidecar is rejected outright.
const STAMP_VERSION: u32 = 2;

/// Producer revision of the [`learn`] pipeline's output.
///
/// **Bump this whenever `learn()`'s output shape or content changes** — a new
/// artifact field it starts filling, a changed analyzer, a different module
/// graph. It is a **cache key**, not a schema version: it is persisted only in
/// the pipeline-internal `.repowalk/analysis-stamps.json` sidecar and never in
/// `analysis.json`, whose `ANALYSIS_VERSION` stays 1 (that artifact is frozen).
///
/// A sidecar written by a different (or pre-revision) pipeline is treated as a
/// full cache miss, so a stale artifact can never be served silently. Revision
/// 2 is the first to carry the module graph ([`Analysis::modules`] /
/// [`Analysis::module_edges`]) that `learn` began building in CORE-9; every
/// revision-1 cache (which stored an empty graph) must be re-analyzed once.
pub const LEARN_REVISION: u32 = 2;

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

/// One file's incremental stamp: nanosecond mtime + byte size (R-14).
///
/// Nanoseconds catch two edits inside one whole second; size catches an edit
/// that preserved the mtime (`git checkout` / `touch -r`). Neither can be
/// confused by the old whole-second `(mtime, lines)` stamp.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Stamp {
    mtime_nanos: u64,
    size: u64,
}

/// The `.repowalk/analysis-stamps.json` payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct StampFile {
    version: u32,
    /// Producer revision ([`LEARN_REVISION`]) that wrote this sidecar. Pre-revision
    /// sidecars lack the field ⇒ serde default `0`, which never matches the
    /// current revision and so forces a full re-analysis.
    #[serde(default)]
    revision: u32,
    /// Number of files (re)analyzed by the most recent [`learn`] call.
    reanalyzed: usize,
    /// `repo-relative path -> (mtime_nanos, size)` for every file at that call.
    stamps: BTreeMap<String, Stamp>,
}

/// `<root>/.repowalk/analysis-stamps.json` — the incremental stamp cache.
pub fn stamps_path(root: &Path) -> PathBuf {
    root.join(".repowalk").join("analysis-stamps.json")
}

/// Number of files the most recent [`learn`] re-analyzed (F10 observable).
/// `0` when no learn has run yet or the sidecar is unreadable/stale (a
/// revision-mismatched sidecar does not count as a cache hit).
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
    learn_with(root, progress, None)
}

/// [`learn`] with a cooperative cancellation flag (model audit F-1).
///
/// `cancel` is checked between phases and per file during the analyze stage
/// (including inside the parallel parse loop of [`crate::pipeline`]). When it is
/// observed, `learn_with` returns `Ok` with the results computed so far and
/// **persists nothing**: no `analysis.json`, no stamp sidecar, no `index.json`
/// rewrite. A cancelled run can therefore never poison the on-disk cache with a
/// partial artifact or a half-paired write — the next learn simply re-runs.
///
/// A caller that cancels (the GUI's supersede path) discards the returned
/// partials; the `Ok` return keeps the signature total without inventing a
/// third outcome for "you asked me to stop".
pub fn learn_with(
    root: &Path,
    progress: &dyn Fn(LearnProgress),
    cancel: Option<&AtomicBool>,
) -> anyhow::Result<(Index, Analysis)> {
    let cancelled = || cancel.is_some_and(|c| c.load(Ordering::Relaxed));
    let empty = Analysis::new();

    // ---- walk -------------------------------------------------------------
    emit(progress, "walk", 0, 0, &empty);
    let files = crate::walk::walk(root)?;
    emit(progress, "walk", files.len(), files.len(), &empty);

    // Fresh stamps. We keep our own `files` for this: `index.json` may be a
    // cache hit carrying stale line counts. The stamp is stat'ed fresh from
    // disk (nanosecond mtime + size) rather than taken from the walked
    // `FileEntry`, whose `mtime` is whole seconds (R-14).
    let current: BTreeMap<String, Stamp> = files
        .iter()
        .map(|f| {
            let full = root.join(f.path.as_str());
            (
                f.path.clone(),
                Stamp {
                    mtime_nanos: crate::walk::mtime_nanos(&full),
                    size: crate::walk::file_size(&full),
                },
            )
        })
        .collect();

    // ---- parse + resolve (delegated to the pipeline) ----------------------
    let idx = crate::pipeline::build_index_from_files(
        root,
        files.clone(),
        &|phase, done, total| {
            emit(progress, phase, done, total, &empty);
        },
        cancel,
    )?;

    // ---- analyze (incremental) -------------------------------------------
    // The cached artifact is the other half of the cache key, so load it before
    // consulting the per-file stamps. `analysis::load` returns `None` for an
    // absent, unreadable, corrupt, or version-mismatched artifact (it is the
    // authority on that check — no hand-rolled version test here). When the
    // artifact did not load, a matching stamp sidecar is *not* a safe hit:
    // seeding would keep nothing and `save` would persist a version-current but
    // **empty** artifact that the next read serves silently (R-21) and that a
    // torn/absent write (R-24) never heals. So an unloadable artifact is treated
    // as full staleness — every analyzable file is forced into re-analysis and
    // the sidecar is rewritten at the end.
    let cached = crate::analysis::load(root);

    // Cancellation between parse and analyze: return the partial pair without
    // touching the cache (F-1).
    if cancelled() {
        return Ok((idx, Analysis::new()));
    }

    // Stamps are only trusted when the artifact they describe actually loaded.
    // An empty `old_stamps` makes every comparison below a difference, which is
    // exactly "re-analyze every analyzable file".
    let old_stamps: BTreeMap<String, Stamp> = match &cached {
        Some(_) => load_stamps(root).map(|s| s.stamps).unwrap_or_default(),
        None => BTreeMap::new(),
    };

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
    // repo-level module graph is recomputed below (it is not per-file). When the
    // artifact did not load there is nothing to seed — `Analysis::new()` carries
    // the current `ANALYSIS_VERSION`, and every section is filled by the full
    // re-analysis above, so the persisted artifact can never be a partially
    // filled one.
    let mut analysis = Analysis::new();
    if let Some(old) = cached {
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

    // ---- analyze (per file, parallel) -------------------------------------
    // The per-file jobs run through the deterministic parallel map (INTEL-5,
    // wired in by model audit F-3). `walk` sorts entries by path (walk.rs), so
    // `needs` is path-sorted and unique — `map_files`' path-sorted output order
    // therefore equals the old sequential iteration order, and appending each
    // per-file partial in that order reproduces the sequential artifact
    // byte-identically. Progress is emitted from THIS thread during the merge
    // (never from workers), keeping the event stream monotonic and identical in
    // shape to the old loop. Workers observe `cancel` per file and yield an
    // empty partial; the merge loop also checks it, and a cancelled run returns
    // before any cache write (F-1).
    if total > 0 {
        let jobs: Vec<FileJob> = needs.iter().map(|f| FileJob::new(&f.path)).collect();
        let langs: BTreeMap<&str, crate::analyze::Lang> = needs
            .iter()
            .map(|f| {
                (
                    f.path.as_str(),
                    crate::analyze::Lang::from_walk(&f.lang)
                        .expect("needs only holds analyzable files"),
                )
            })
            .collect();
        let results = crate::parallel::map_files(&jobs, 0, |job| -> anyhow::Result<Analysis> {
            if cancelled() {
                return Ok(Analysis::new());
            }
            let lang = langs[job.path.as_str()];
            let source = std::fs::read_to_string(root.join(&job.path))?;
            let symbols = symbols_by_file
                .get(&job.path)
                .map(Vec::as_slice)
                .unwrap_or(&[]);
            let mut part = Analysis::new();
            crate::analyze::analyze(&job.path, lang, &source, symbols, &mut part);
            Ok(part)
        });
        for (i, result) in results.into_iter().enumerate() {
            if cancelled() {
                return Ok((idx, analysis));
            }
            let part = result?;
            analysis.variables.extend(part.variables);
            analysis.cfgs.extend(part.cfgs);
            analysis.types.extend(part.types);
            analysis.type_edges.extend(part.type_edges);
            analysis.uses.extend(part.uses);
            emit(progress, "analyze", i + 1, total, &analysis);
        }
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
            revision: LEARN_REVISION,
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
    // A sidecar is only usable when both its schema version and its producer
    // revision match. Anything else — missing, unparsable, or written by an
    // older pipeline — is a full cache miss: `learn` re-analyzes every file and
    // rewrites the sidecar at the current revision.
    (stamps.version == STAMP_VERSION && stamps.revision == LEARN_REVISION).then_some(stamps)
}

fn save_stamps(root: &Path, stamps: &StampFile) -> anyhow::Result<()> {
    crate::store::atomic_write(&stamps_path(root), &serde_json::to_vec(stamps)?)
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

    /// Rewrite the raw stamp sidecar in place. Used to simulate a cache written
    /// by an older pipeline (force/remove the `revision` field) or to corrupt it.
    fn rewrite_sidecar(root: &Path, mutate: impl FnOnce(&mut serde_json::Value)) {
        let path = stamps_path(root);
        let mut value: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        mutate(&mut value);
        std::fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
    }

    /// A throwaway copy of the real `fixtures/sample` repo (files only, so the
    /// shared `.repowalk` cache is never copied). Lets a test learn the real
    /// fixture without racing `learns_sample_and_populates_modules`.
    fn copy_sample_fixture(tag: &str) -> PathBuf {
        let src = sample_root();
        let dst = temp_root(tag);
        for entry in std::fs::read_dir(&src).unwrap() {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_file() {
                std::fs::copy(entry.path(), dst.join(entry.file_name())).unwrap();
            }
        }
        dst
    }

    /// Analyzable files in an index — exactly what `reanalyzed` counts on a full
    /// (revision-mismatched) pass.
    fn analyzable_count(idx: &Index) -> usize {
        idx.files
            .iter()
            .filter(|f| crate::analyze::Lang::from_walk(&f.lang).is_some())
            .count()
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

        // Rewrite main.rs with a different byte size so the stamp differs even
        // if the mtime happens to land in the same (nano)second.
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

    // ---- cooperative cancellation (model audit F-1) ------------------------

    /// A pre-cancelled learn returns Ok but persists nothing: no analysis.json,
    /// no stamp sidecar, and (parse being cancelled before the first file) no
    /// index.json either. The next learn re-runs from a pristine cache.
    #[test]
    fn cancelled_learn_persists_nothing() {
        let root = temp_root("cancelled");
        write_fixture(&root);
        let cancel = AtomicBool::new(true);

        let result = learn_with(&root, &|_| {}, Some(&cancel)).unwrap();
        assert!(
            result.0.symbols.is_empty(),
            "a pre-cancelled run parses no symbols"
        );
        assert!(
            !crate::analysis::analysis_path(&root).exists(),
            "a cancelled run must not write analysis.json"
        );
        assert!(
            !stamps_path(&root).exists(),
            "a cancelled run must not write the stamp sidecar"
        );
        assert!(
            !crate::store::cache_path(&root).exists(),
            "a cancelled run must not write index.json"
        );

        // The next learn (flag dropped) runs a full cold pass and writes normally.
        learn(&root, &|_| {}).unwrap();
        assert!(crate::analysis::analysis_path(&root).exists());
        assert!(reanalyzed(&root) >= 1);

        let _ = std::fs::remove_dir_all(&root);
    }

    /// Cancelling mid-run must not flip the outcome of the *next* learn: after a
    /// warm learn, a cancelled re-learn leaves the existing cache untouched and
    /// still loadable.
    #[test]
    fn cancelled_relearn_leaves_existing_cache_intact() {
        let root = temp_root("cancel-warm");
        write_fixture(&root);
        let (_, first) = learn(&root, &|_| {}).unwrap();

        let cancel = AtomicBool::new(true);
        let _ = learn_with(&root, &|_| {}, Some(&cancel)).unwrap();
        assert_eq!(
            analysis_for(&root).unwrap(),
            first,
            "the cancelled run must not overwrite the artifact"
        );
        assert_eq!(load_stamps(&root).unwrap().revision, LEARN_REVISION);

        // And the pipeline is still warm afterwards.
        assert_eq!(analyze_done(&learn_events(&root)), 0);

        let _ = std::fs::remove_dir_all(&root);
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

    // ---- producer revision (CORE-10) -------------------------------------

    #[test]
    fn stale_revision_forces_full_reanalysis() {
        let root = temp_root("rev-stale");
        write_fixture(&root);

        let (idx, first) = learn(&root, &|_| {}).unwrap();
        let file_count = analyzable_count(&idx);
        assert_eq!(file_count, 2, "fixture has two analyzable files");
        assert_eq!(
            reanalyzed(&root),
            file_count,
            "cold learn analyzes all files"
        );

        // Simulate a cache written by revision 1 (pre-modules output).
        rewrite_sidecar(&root, |v| {
            v["revision"] = serde_json::json!(1);
        });

        let events = learn_events(&root);
        assert_eq!(
            reanalyzed(&root),
            file_count,
            "a revision-1 sidecar must force re-analysis of every file"
        );
        assert_eq!(analyze_done(&events), file_count);
        assert_eq!(
            analysis_for(&root).unwrap(),
            first,
            "full re-analysis reproduces the same artifact"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn missing_revision_forces_full_reanalysis() {
        let root = temp_root("rev-absent");
        write_fixture(&root);

        let (idx, _) = learn(&root, &|_| {}).unwrap();
        let file_count = analyzable_count(&idx);

        // A pre-revision sidecar simply lacks the field.
        rewrite_sidecar(&root, |v| {
            v.as_object_mut().unwrap().remove("revision");
        });

        let events = learn_events(&root);
        assert_eq!(
            reanalyzed(&root),
            file_count,
            "an absent revision must force re-analysis of every file"
        );
        assert_eq!(analyze_done(&events), file_count);

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn current_revision_preserves_incremental_behaviour() {
        let root = temp_root("rev-incremental");
        write_fixture(&root);
        learn(&root, &|_| {}).unwrap();

        // Warm re-learn at the current revision: nothing to do.
        let warm = learn_events(&root);
        assert_eq!(analyze_done(&warm), 0, "warm re-learn analyzes 0");
        assert_eq!(reanalyzed(&root), 0);

        // Touch exactly one file (larger byte size ⇒ stamp changes regardless
        // of whether the mtime lands in the same nanosecond-or-second window).
        std::fs::write(
            root.join("src/main.rs"),
            "fn main() {\n    let x = 1;\n    let y = 2;\n    println!(\"{}\", x + y);\n}\n",
        )
        .unwrap();
        let touched = learn_events(&root);
        assert_eq!(
            analyze_done(&touched),
            1,
            "only the touched file is re-analyzed"
        );
        assert_eq!(reanalyzed(&root), 1);

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn revision_roundtrips_through_sidecar() {
        let root = temp_root("rev-roundtrip");
        write_fixture(&root);
        learn(&root, &|_| {}).unwrap();

        let raw: serde_json::Value =
            serde_json::from_slice(&std::fs::read(stamps_path(&root)).unwrap()).unwrap();
        assert_eq!(
            raw["revision"],
            serde_json::json!(LEARN_REVISION),
            "sidecar persists the current producer revision"
        );

        let loaded = load_stamps(&root).expect("current-revision sidecar is a valid cache");
        assert_eq!(loaded.revision, LEARN_REVISION);
        assert_eq!(loaded.version, STAMP_VERSION);

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn corrupt_sidecar_forces_full_reanalysis_without_panic() {
        let root = temp_root("rev-corrupt");
        write_fixture(&root);

        let (idx, first) = learn(&root, &|_| {}).unwrap();
        let file_count = analyzable_count(&idx);

        std::fs::write(stamps_path(&root), b"{ not valid json").unwrap();

        let events = learn_events(&root);
        assert_eq!(
            reanalyzed(&root),
            file_count,
            "a corrupt sidecar must never be a silent hit"
        );
        assert_eq!(analyze_done(&events), file_count);
        assert_eq!(analysis_for(&root).unwrap(), first);
        // The rewrite healed the sidecar at the current revision.
        assert_eq!(load_stamps(&root).unwrap().revision, LEARN_REVISION);

        let _ = std::fs::remove_dir_all(&root);
    }

    /// The regression this card exists to prevent: a revision-1 cache served
    /// `modules: []`. After the bump the real fixture's module tree is back and
    /// the following warm learn is a hit again (0 re-analyzed).
    #[test]
    fn sample_fixture_repopulates_modules_after_revision_bump() {
        let root = copy_sample_fixture("rev-sample");
        let (idx, _) = learn(&root, &|_| {}).unwrap();
        let file_count = analyzable_count(&idx);
        assert!(file_count > 0, "the real fixture has analyzable sources");

        // Exactly the on-disk state DSN-4 hit: stamps present, revision 1.
        rewrite_sidecar(&root, |v| {
            v["revision"] = serde_json::json!(1);
        });

        let (_, after_bump) = learn(&root, &|_| {}).unwrap();
        assert_eq!(
            reanalyzed(&root),
            file_count,
            "revision 1 ⇒ full re-analysis on the real fixture"
        );
        assert!(
            !after_bump.modules.is_empty(),
            "Analysis.modules must be non-empty after the bump"
        );

        let warm = learn_events(&root);
        assert_eq!(analyze_done(&warm), 0, "warm re-learn after bump is a hit");
        assert_eq!(reanalyzed(&root), 0);
        assert!(
            !analysis_for(&root).unwrap().modules.is_empty(),
            "the served artifact still carries the module tree"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    // ---- unloadable analysis.json forces full staleness (CORE-11) ---------

    /// Learn once, prove it is complete, then poison `analysis.json` and learn
    /// again. The rewrite must be a complete artifact (real variables/types),
    /// never the version-current-but-empty artifact the defect produced, and
    /// the learn after it must be a warm hit that still serves the real values.
    fn assert_unloadable_artifact_forces_complete_reanalysis(
        tag: &str,
        poison: impl FnOnce(&Path),
    ) {
        // The real fixture: it has the card's observable values (variables=5,
        // types=1), so "complete" is judged against a genuinely non-empty
        // artifact, not a synthetic one.
        let root = copy_sample_fixture(tag);

        // Cold learn: the real per-file payload, with types present.
        let (idx, first) = learn(&root, &|_| {}).unwrap();
        let file_count = analyzable_count(&idx);
        assert!(file_count > 0, "the fixture has analyzable files");
        assert!(
            !first.variables.is_empty() && !first.types.is_empty(),
            "cold learn must be complete: variables={} types={}",
            first.variables.len(),
            first.types.len()
        );
        let warm = learn_events(&root);
        assert!(!warm.is_empty(), "warm learn ran");
        assert_eq!(analyze_done(&warm), 0, "baseline is warm");

        // Poison the cached artifact, leaving the stamp sidecar current.
        poison(&root);
        assert!(
            analysis_for(&root).is_none(),
            "the poisoned artifact must not load"
        );

        // The unloadable artifact is full staleness: every analyzable file is
        // re-analyzed and the rewrite is complete, not empty.
        let events = learn_events(&root);
        assert_eq!(
            reanalyzed(&root),
            file_count,
            "{tag}: an unloadable analysis.json must force a full re-analysis"
        );
        assert_eq!(analyze_done(&events), file_count);
        let healed = analysis_for(&root).expect("a complete artifact was written back");
        assert_eq!(
            (
                healed.variables.len(),
                healed.cfgs.len(),
                healed.types.len(),
                healed.uses.len()
            ),
            (
                first.variables.len(),
                first.cfgs.len(),
                first.types.len(),
                first.uses.len()
            ),
            "{tag}: the rewritten artifact must be complete, not empty"
        );
        assert!(!healed.variables.is_empty() && !healed.types.is_empty());

        // A second learn immediately after is a warm hit that keeps the values.
        assert_eq!(
            analyze_done(&learn_events(&root)),
            0,
            "{tag}: the learn after the heal must be warm"
        );
        assert_eq!(reanalyzed(&root), 0);
        assert_eq!(
            analysis_for(&root).unwrap(),
            first,
            "{tag}: warm re-learn still serves the complete artifact"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    /// (a) The artifact is simply gone while the sidecar is current.
    #[test]
    fn missing_analysis_artifact_forces_complete_reanalysis() {
        assert_unloadable_artifact_forces_complete_reanalysis("poison-absent", |root| {
            std::fs::remove_file(crate::analysis::analysis_path(root)).unwrap();
        });
    }

    /// (b) The artifact body is garbage that does not even parse.
    #[test]
    fn corrupt_analysis_artifact_forces_complete_reanalysis() {
        assert_unloadable_artifact_forces_complete_reanalysis("poison-corrupt", |root| {
            std::fs::write(crate::analysis::analysis_path(root), b"{ broken").unwrap();
        });
    }

    /// (c) The artifact parses but carries an unknown version, which
    /// `analysis::load` refuses.
    #[test]
    fn unknown_version_analysis_artifact_forces_complete_reanalysis() {
        assert_unloadable_artifact_forces_complete_reanalysis("poison-version", |root| {
            let path = crate::analysis::analysis_path(root);
            let mut v: serde_json::Value =
                serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
            v["version"] = serde_json::json!(crate::analysis::ANALYSIS_VERSION + 1);
            std::fs::write(&path, serde_json::to_vec(&v).unwrap()).unwrap();
        });
    }
}
