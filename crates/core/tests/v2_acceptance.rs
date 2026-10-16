//! VER-3 independent v2 acceptance battery (TL-VER).
//!
//! Written by the verification seat, not by any producer. Every assertion here
//! is a claim the v2 cards make, re-derived from the public API and the on-disk
//! artifacts — the author's own tests are deliberately not trusted:
//!
//! * **CORE-10** — a producer-revision change must invalidate the whole
//!   per-file analysis cache, a warm re-learn must be a hit, and a corrupt
//!   sidecar must be a miss, never a panic.
//! * **R-14** — input staleness: a same-second rewrite, a size change with a
//!   preserved mtime, and an edit of a file that is not the newest must each be
//!   a cache MISS (the old global-second-max comparison hit all three).
//! * **R-27** — language dispatch: a TS/JS-only tree must extract non-zero
//!   symbols, and an unsupported language must be reported, never silently
//!   counted as zero.
//! * **Emitter fidelity** — Mermaid, DOT and the Markdown report must describe
//!   the same node/edge counts for the same diagram (one shared model).
//! * **Frozen contract** — the artifact carries its version and an
//!   unknown-version file is refused, never misread.
//!
//! CLI-level cross-checks (report --json) live in `crates/cli/tests/v2_acceptance.rs`
//! so the persisted artifact, not just the in-memory value, is what is judged.
use repowalk_core::analysis::{self, ModuleEdgeKind};
use repowalk_core::analyze::Lang;
use repowalk_core::export::{self, Diagram};
use repowalk_core::learn;
use repowalk_core::model::Index;
use repowalk_core::pipeline;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

fn temp_root(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("rw-v2acc-{}-{}", std::process::id(), tag));
    let _ = fs::remove_dir_all(&p);
    fs::create_dir_all(&p).unwrap();
    p
}

/// Copy `fixtures/sample` flat files into a fresh temp dir (never the shared
/// `.repowalk` cache — the fixture is used by other tests concurrently).
fn copy_sample(tag: &str) -> PathBuf {
    let src = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/sample")
        .canonicalize()
        .expect("fixtures/sample must exist");
    let dst = temp_root(tag);
    for entry in fs::read_dir(&src).unwrap() {
        let entry = entry.unwrap();
        if entry.file_type().unwrap().is_file() {
            fs::copy(entry.path(), dst.join(entry.file_name())).unwrap();
        }
    }
    dst
}

fn analyzable_count(idx: &Index) -> usize {
    idx.files
        .iter()
        .filter(|f| Lang::from_walk(&f.lang).is_some())
        .count()
}

/// Run `learn` and return the max `done` reached in the `analyze` phase — the
/// number of files the last learn actually re-analyzed.
fn learn_analyze_done(root: &Path) -> usize {
    let seen = std::cell::RefCell::new(Vec::new());
    learn::learn(root, &|p| seen.borrow_mut().push(p)).unwrap();
    seen.into_inner()
        .iter()
        .filter(|p| p.phase == "analyze")
        .map(|p| p.done)
        .max()
        .unwrap_or(0)
}

/// Overwrite the raw stamp sidecar in place (simulate an older pipeline / corrupt it).
fn rewrite_sidecar(root: &Path, mutate: impl FnOnce(&mut serde_json::Value)) {
    let path = learn::stamps_path(root);
    let mut v: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    mutate(&mut v);
    fs::write(&path, serde_json::to_vec(&v).unwrap()).unwrap();
}

fn set_mtime(path: &Path, t: SystemTime) {
    let f = fs::OpenOptions::new().write(true).open(path).unwrap();
    f.set_modified(t).unwrap();
}

fn epoch_secs(t: SystemTime) -> u64 {
    t.duration_since(UNIX_EPOCH).unwrap().as_secs()
}

fn variable_names(a: &analysis::Analysis, file: &str) -> Vec<String> {
    a.variables
        .iter()
        .filter(|v| v.file == file)
        .map(|v| v.name.clone())
        .collect()
}

// ---------------------------------------------------------------------------
// CORE-10 — cache invalidation on PRODUCER change
// ---------------------------------------------------------------------------

/// A `revision: 1` sidecar (exactly the on-disk state DSN-4 hit) must force a
/// full re-analysis and repopulate the module graph CORE-9 added.
#[test]
fn core10_stale_revision_forces_full_reanalysis_and_repopulates_modules() {
    let root = copy_sample("core10-rev1");
    let (idx, _) = learn::learn(&root, &|_| {}).unwrap();
    let file_count = analyzable_count(&idx);
    assert!(file_count > 0, "fixture has analyzable sources");

    // Prove the cold learn already produced the CORE-9 payload.
    assert!(
        !analysis::load(&root).unwrap().modules.is_empty(),
        "cold learn must populate Analysis.modules"
    );

    // Simulate a pre-CORE-9 cache: same stamps, revision 1.
    rewrite_sidecar(&root, |v| v["revision"] = serde_json::json!(1));

    let reanalyzed = learn_analyze_done(&root);
    assert_eq!(
        reanalyzed, file_count,
        "revision 1 must re-analyze every analyzable file"
    );
    assert_eq!(
        learn::reanalyzed(&root),
        file_count,
        "the sidecar must report the full re-analysis"
    );

    // (b) the served artifact carries the module graph again.
    let served = analysis::load(&root).unwrap();
    assert!(
        !served.modules.is_empty(),
        "modules must be non-empty after the bump"
    );
    assert!(served
        .module_edges
        .iter()
        .any(|e| e.kind == ModuleEdgeKind::Submodule));

    // (c) the very next learn is a warm hit.
    assert_eq!(learn_analyze_done(&root), 0, "warm re-learn must analyze 0");
    assert_eq!(learn::reanalyzed(&root), 0);
    assert!(!analysis::load(&root).unwrap().modules.is_empty());

    let _ = fs::remove_dir_all(&root);
}

/// An absent `revision` field (serde default 0) is a miss, not a hit.
#[test]
fn core10_absent_revision_forces_full_reanalysis() {
    let root = copy_sample("core10-absent");
    let (idx, _) = learn::learn(&root, &|_| {}).unwrap();
    let file_count = analyzable_count(&idx);

    rewrite_sidecar(&root, |v| {
        v.as_object_mut().unwrap().remove("revision");
    });

    assert_eq!(
        learn_analyze_done(&root),
        file_count,
        "an absent revision must force a full re-analysis"
    );
    assert!(!analysis::load(&root).unwrap().modules.is_empty());
    let _ = fs::remove_dir_all(&root);
}

/// (d) One-file edit re-analyzes exactly that file, and the artifact on disk
/// (what the CLI reads) carries the new data.
#[test]
fn core10_warm_hit_then_one_file_edit_reanalyzes_exactly_one() {
    let root = copy_sample("core10-one");
    learn::learn(&root, &|_| {}).unwrap();
    assert_eq!(learn_analyze_done(&root), 0, "second learn is warm (0)");

    // Change exactly one file; add a variable so the analysis changes too.
    fs::write(
        root.join("a.rs"),
        "//! Half of the sample import cycle.\n\nuse crate::b;\n\n\
         /// Calls into b, closing the a → b → a cycle.\n\
         pub fn a_fn() {\n    let zeta = 41;\n    b::b_fn();\n}\n",
    )
    .unwrap();

    assert_eq!(
        learn_analyze_done(&root),
        1,
        "a one-file edit re-analyzes exactly one file"
    );
    assert_eq!(learn::reanalyzed(&root), 1);
    let served = analysis::load(&root).unwrap();
    assert!(
        variable_names(&served, "a.rs").iter().any(|n| n == "zeta"),
        "the persisted artifact must carry the edited file's analysis"
    );
    let _ = fs::remove_dir_all(&root);
}

/// VER-3 budget harness. Runs only when `VER3_BIG_REPO` points at a repo copy
/// (and, for meaningful numbers, under `--release`). Prints cold / warm /
/// one-file-incremental learn timings and a single large-file parse+analyze.
///
/// Run:
/// `VER3_BIG_REPO=/tmp/… cargo test --release -p repowalk-core --test v2_acceptance -- --ignored measured_learn_budgets --nocapture`
#[test]
#[ignore = "VER-3 measurement harness: set VER3_BIG_REPO=<repo copy> and run --release --nocapture"]
fn measured_learn_budgets() {
    let Ok(repo) = std::env::var("VER3_BIG_REPO") else {
        return;
    };
    let root = PathBuf::from(&repo);
    let _ = fs::remove_dir_all(root.join(".repowalk"));

    let t = std::time::Instant::now();
    let (idx, a) = learn::learn(&root, &|_| {}).unwrap();
    println!(
        "VER3 cold learn: {:?}; files={} analyzable={} variables={} modules={}",
        t.elapsed(),
        idx.files.len(),
        analyzable_count(&idx),
        a.variables.len(),
        a.modules.len()
    );

    let t = std::time::Instant::now();
    learn::learn(&root, &|_| {}).unwrap();
    println!(
        "VER3 warm learn: {:?}; reanalyzed={}",
        t.elapsed(),
        learn::reanalyzed(&root)
    );

    let target = idx
        .files
        .iter()
        .filter(|f| Lang::from_walk(&f.lang).is_some())
        .max_by_key(|f| f.lines)
        .map(|f| f.path.clone())
        .expect("copy has analyzable files");
    let mut src = fs::read_to_string(root.join(&target)).unwrap();
    src.push_str("\n// ver3 incremental probe\n");
    fs::write(root.join(&target), src).unwrap();
    let t = std::time::Instant::now();
    learn::learn(&root, &|_| {}).unwrap();
    println!(
        "VER3 incremental learn after editing {target}: {:?}; reanalyzed={}",
        t.elapsed(),
        learn::reanalyzed(&root)
    );

    // The largest single source in this repo's core crate (~1600 lines).
    let big = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/analyze/python.rs");
    if let Ok(source) = fs::read_to_string(&big) {
        let mut analysis = analysis::Analysis::new();
        let t = std::time::Instant::now();
        repowalk_core::analyze::analyze("python.rs", Lang::Python, &source, &[], &mut analysis);
        println!(
            "VER3 parse+analyze python.rs ({} lines): {:?}; cfgs={} vars={}",
            source.lines().count(),
            t.elapsed(),
            analysis.cfgs.len(),
            analysis.variables.len()
        );
    }
}

/// A garbage sidecar body must never be a silent hit and must never panic.
#[test]
fn core10_garbage_sidecar_body_is_a_miss_not_a_panic() {
    let root = copy_sample("core10-garbage");
    let (idx, first) = learn::learn(&root, &|_| {}).unwrap();
    let file_count = analyzable_count(&idx);

    fs::write(learn::stamps_path(&root), b"this is not json at all").unwrap();

    // Must return Ok (no panic) and re-analyze everything.
    let seen = std::cell::RefCell::new(Vec::new());
    let result = learn::learn(&root, &|p| seen.borrow_mut().push(p));
    assert!(
        result.is_ok(),
        "a corrupt sidecar must not panic: {result:?}"
    );

    assert_eq!(
        learn::reanalyzed(&root),
        file_count,
        "a garbage sidecar must be a full miss"
    );
    assert_eq!(
        analysis::load(&root).unwrap(),
        first,
        "full re-analysis must reproduce the same artifact"
    );

    // The rewrite healed the sidecar at the current revision.
    let healed: serde_json::Value =
        serde_json::from_slice(&fs::read(learn::stamps_path(&root)).unwrap()).unwrap();
    assert_eq!(healed["revision"], serde_json::json!(learn::LEARN_REVISION));
    let _ = fs::remove_dir_all(&root);
}

// ---------------------------------------------------------------------------
// R-14 — input staleness (the old max(mtime)-at-whole-second comparison hit all
// three of these). Judged at the learn/artifact level, which is what the CLI serves.
// ---------------------------------------------------------------------------

/// Two writes inside one whole second, same line count: the seconds mtime and
/// line count stamp is identical, so this is the strict case.
///
/// Regression guard for the R-14 residual found by VER-3 (card CORE-8): the
/// learn-level stamp must be `(mtime_nanos, size)`, not `(mtime_seconds, lines)`.
#[test]
fn r14_same_second_rewrite_is_a_miss() {
    let root = temp_root("r14-same-second");
    let file = root.join("m.rs");
    fs::write(
        &file,
        "fn f() {\n    let alpha = 1;\n    let beta = 2;\n}\n",
    )
    .unwrap();
    let t0 = fs::metadata(&file).unwrap().modified().unwrap();
    set_mtime(&file, t0);

    learn::learn(&root, &|_| {}).unwrap();

    // Same line count, same byte length, same whole second.
    fs::write(
        &file,
        "fn f() {\n    let gamma = 1;\n    let delta = 2;\n}\n",
    )
    .unwrap();
    set_mtime(&file, t0 + Duration::from_nanos(1));
    assert_eq!(
        epoch_secs(fs::metadata(&file).unwrap().modified().unwrap()),
        epoch_secs(t0),
        "the two writes must share one whole second"
    );

    assert_eq!(
        learn_analyze_done(&root),
        1,
        "a same-second rewrite must be re-analyzed"
    );
    let served = analysis::load(&root).unwrap();
    let names = variable_names(&served, "m.rs");
    assert!(
        names.iter().any(|n| n == "gamma") && !names.iter().any(|n| n == "alpha"),
        "the served analysis is stale: {names:?}"
    );
    let _ = fs::remove_dir_all(&root);
}

/// A size change with the mtime preserved exactly (`touch -r` / `git checkout`).
///
/// Regression guard for the R-14 residual found by VER-3 (card CORE-8).
#[test]
fn r14_size_change_with_preserved_mtime_is_a_miss() {
    let root = temp_root("r14-preserved-mtime");
    let file = root.join("m.rs");
    fs::write(&file, "fn f() {\n    let a = 1;\n}\n").unwrap();
    let t0 = fs::metadata(&file).unwrap().modified().unwrap();
    set_mtime(&file, t0);

    learn::learn(&root, &|_| {}).unwrap();

    // Longer identifier: byte size changes, line count does not, mtime preserved.
    fs::write(&file, "fn f() {\n    let aaa = 1;\n}\n").unwrap();
    set_mtime(&file, t0);
    assert_eq!(
        fs::metadata(&file).unwrap().modified().unwrap(),
        t0,
        "mtime must be byte-for-byte preserved"
    );

    assert_eq!(
        learn_analyze_done(&root),
        1,
        "a preserved-mtime size change must be re-analyzed"
    );
    let names = variable_names(&analysis::load(&root).unwrap(), "m.rs");
    assert!(
        names.iter().any(|n| n == "aaa"),
        "stale analysis: {names:?}"
    );
    let _ = fs::remove_dir_all(&root);
}

/// An edit of a file that is *not* the newest (global max mtime unchanged).
#[test]
fn r14_edit_of_non_newest_file_is_a_miss() {
    let root = temp_root("r14-non-newest");
    let older = root.join("older.rs");
    let newest = root.join("newest.rs");
    fs::write(&older, "fn older() {\n    let a = 1;\n}\n").unwrap();
    fs::write(&newest, "fn newest() {\n    let n = 1;\n}\n").unwrap();
    let base = UNIX_EPOCH + Duration::from_secs(1_600_000_000);
    set_mtime(&older, base);
    set_mtime(&newest, base + Duration::from_secs(1000));

    learn::learn(&root, &|_| {}).unwrap();
    assert_eq!(learn_analyze_done(&root), 0, "warm baseline");

    // Edit ONLY the older file and keep it older than `newest`.
    fs::write(&older, "fn older() {\n    let zzz = 1;\n}\n").unwrap();
    set_mtime(&older, base + Duration::from_secs(1));

    assert_eq!(
        learn_analyze_done(&root),
        1,
        "editing a non-newest file must re-analyze exactly it"
    );
    let names = variable_names(&analysis::load(&root).unwrap(), "older.rs");
    assert!(
        names.iter().any(|n| n == "zzz"),
        "stale analysis: {names:?}"
    );
    let _ = fs::remove_dir_all(&root);
}

// ---------------------------------------------------------------------------
// R-14 at the INDEX layer (CORE-8's store.rs fix) — must also miss.
// ---------------------------------------------------------------------------

/// Same-second rewrite of a file whose content changes but keeps its line count
/// and byte length. Only the nanosecond mtime differs, so the old global
/// max-at-whole-second comparison was a HIT.
#[test]
fn r14_index_cache_same_second_rewrite_is_a_miss() {
    let root = temp_root("r14-idx-same-second");
    let file = root.join("m.rs");
    fs::write(&file, "pub fn alpha() {}\n").unwrap();
    let t0 = fs::metadata(&file).unwrap().modified().unwrap();
    set_mtime(&file, t0);
    let first = pipeline::build_index(&root, &|_| {}).unwrap();
    assert!(first.symbols.iter().any(|s| s.name == "alpha"));

    // "alpha" -> "gamma": same byte length, same line count, same second.
    fs::write(&file, "pub fn gamma() {}\n").unwrap();
    set_mtime(&file, t0 + Duration::from_nanos(1));
    assert_eq!(
        epoch_secs(fs::metadata(&file).unwrap().modified().unwrap()),
        epoch_secs(t0)
    );

    let second = pipeline::build_index(&root, &|_| {}).unwrap();
    let names: Vec<&str> = second.symbols.iter().map(|s| s.name.as_str()).collect();
    assert!(
        names.contains(&"gamma") && !names.contains(&"alpha"),
        "index cache served stale symbols: {names:?}"
    );
    let _ = fs::remove_dir_all(&root);
}

/// A size change with the mtime preserved exactly.
#[test]
fn r14_index_cache_size_change_with_preserved_mtime_is_a_miss() {
    let root = temp_root("r14-idx-preserved");
    let file = root.join("m.rs");
    fs::write(&file, "pub fn a() {}\n").unwrap();
    let t0 = fs::metadata(&file).unwrap().modified().unwrap();
    set_mtime(&file, t0);
    pipeline::build_index(&root, &|_| {}).unwrap();

    fs::write(&file, "pub fn aaa() {}\n").unwrap();
    set_mtime(&file, t0);
    assert_eq!(fs::metadata(&file).unwrap().modified().unwrap(), t0);

    let after = pipeline::build_index(&root, &|_| {}).unwrap();
    let names: Vec<&str> = after.symbols.iter().map(|s| s.name.as_str()).collect();
    assert!(
        names.contains(&"aaa"),
        "index cache served stale: {names:?}"
    );
    let _ = fs::remove_dir_all(&root);
}

/// An edit of a file that is not the newest (global max mtime unchanged).
#[test]
fn r14_index_cache_non_newest_edit_is_a_miss() {
    let root = temp_root("r14-idx-non-newest");
    let older = root.join("older.rs");
    let newest = root.join("newest.rs");
    fs::write(&older, "pub fn old_a() {}\n").unwrap();
    fs::write(&newest, "pub fn fresh() {}\n").unwrap();
    let base = UNIX_EPOCH + Duration::from_secs(1_600_000_000);
    set_mtime(&older, base);
    set_mtime(&newest, base + Duration::from_secs(1000));
    pipeline::build_index(&root, &|_| {}).unwrap();

    fs::write(&older, "pub fn old_b() {}\n").unwrap();
    set_mtime(&older, base + Duration::from_secs(1));

    let after = pipeline::build_index(&root, &|_| {}).unwrap();
    let names: Vec<&str> = after.symbols.iter().map(|s| s.name.as_str()).collect();
    assert!(
        names.contains(&"old_b") && !names.contains(&"old_a"),
        "index cache served stale: {names:?}"
    );
    let _ = fs::remove_dir_all(&root);
}

// ---------------------------------------------------------------------------
// ANTI — an unloadable `analysis.json` must not be served as empty data
// (VER-3 finding; see board/docs/verification-v2.md and card CORE-11)
// ---------------------------------------------------------------------------

/// Regression guard for card CORE-11 (found by VER-3). If the persisted
/// `analysis.json` is missing, corrupt, or version-mismatched — but
/// `analysis-stamps.json` is still current — `learn()` must treat that as full
/// staleness: re-analyze every file, never seed nothing and rewrite a
/// *version-current but empty* artifact that the next read would serve silently
/// (which is what R-21 forbids and what shipped before CORE-07's fix).
#[test]
fn anti_unloadable_artifact_must_force_full_reanalysis() {
    let root = copy_sample("anti-unloadable");
    let (idx, _) = learn::learn(&root, &|_| {}).unwrap();
    let file_count = analyzable_count(&idx);
    assert_eq!(learn_analyze_done(&root), 0, "baseline warm");

    // A parses-but-version-mismatched artifact, stamps left current.
    let mut v: serde_json::Value =
        serde_json::from_slice(&fs::read(analysis::analysis_path(&root)).unwrap()).unwrap();
    v["version"] = serde_json::json!(analysis::ANALYSIS_VERSION + 1);
    fs::write(
        analysis::analysis_path(&root),
        serde_json::to_vec(&v).unwrap(),
    )
    .unwrap();

    // A full re-learn is required: the cached artifact is unusable.
    assert_eq!(
        learn_analyze_done(&root),
        file_count,
        "an unreadable analysis.json must force re-analysis of every file"
    );
    let served = analysis::load(&root).unwrap();
    assert!(
        !served.variables.is_empty(),
        "learn() must not persist an empty artifact after refusing a bad version"
    );
    let _ = fs::remove_dir_all(&root);
}

// ---------------------------------------------------------------------------
// R-27 — language dispatch (CORE-8)
// ---------------------------------------------------------------------------

/// A tree with ONLY `.ts` and `.js` files must extract non-zero symbols. The
/// pre-CORE-8 `_ =>` arm returned empty vectors (the "0 symbols" regression).
#[test]
fn r27_ts_js_only_tree_reports_nonzero_symbols() {
    let root = temp_root("r27-ts-js");
    fs::write(
        root.join("main.ts"),
        "export function greet(name: string): string {\n    return name;\n}\n\n\
         export class Greeter {\n    hello(): string {\n        return greet(\"world\");\n    }\n}\n",
    )
    .unwrap();
    fs::write(
        root.join("util.js"),
        "export function helper(x) {\n    return x;\n}\n",
    )
    .unwrap();

    let report = pipeline::build_index_with_report(&root, &|_| {}).unwrap();
    assert!(
        !report.index.symbols.is_empty(),
        "a TS/JS-only tree must extract symbols (was 0 before CORE-8)"
    );
    let names: Vec<&str> = report
        .index
        .symbols
        .iter()
        .map(|s| s.name.as_str())
        .collect();
    assert!(names.contains(&"greet"), "missing greet: {names:?}");
    assert!(names.contains(&"Greeter"), "missing Greeter: {names:?}");
    assert!(names.contains(&"helper"), "missing helper: {names:?}");
    assert!(
        report.unsupported.is_empty(),
        "typescript/javascript are supported: {:?}",
        report.unsupported
    );
    let _ = fs::remove_dir_all(&root);
}

/// An unsupported language must be named explicitly, never silently 0-symbol.
#[test]
fn r27_unsupported_language_is_reported_explicitly() {
    let root = temp_root("r27-unsupported");
    fs::write(root.join("script.rb"), "def hi\n  1\nend\n").unwrap();
    fs::write(
        root.join("ok.rs"),
        "pub fn add(a: i32, b: i32) -> i32 { a + b }\n",
    )
    .unwrap();

    let report = pipeline::build_index_with_report(&root, &|_| {}).unwrap();

    // The file is still indexed (not dropped) ...
    assert!(report.index.files.iter().any(|f| f.path == "script.rb"));
    // ... and the language is named, not folded into a silent zero.
    let rb = report
        .unsupported
        .iter()
        .find(|u| u.language == "rb")
        .unwrap_or_else(|| panic!("`.rb` must be reported: {:?}", report.unsupported));
    assert_eq!(rb.files, 1);
    assert_eq!(
        report.unsupported_summary().as_deref(),
        Some("1 files not analyzed (rb)")
    );
    // The supported language still extracted symbols.
    assert!(report.index.symbols.iter().any(|s| s.name == "add"));
    let _ = fs::remove_dir_all(&root);
}

// ---------------------------------------------------------------------------
// Emitter fidelity — one shared model, no drift between surfaces
// ---------------------------------------------------------------------------

fn mermaid_flow_counts(s: &str) -> (usize, usize) {
    let (mut nodes, mut edges) = (0, 0);
    for line in s.lines() {
        let t = line.trim();
        if t.is_empty() || t.starts_with("flowchart") || t.starts_with("%%") {
            continue;
        }
        if t.contains("-->") || t.contains("-.->") {
            edges += 1;
        } else {
            nodes += 1;
        }
    }
    (nodes, edges)
}

fn mermaid_class_counts(s: &str) -> (usize, usize) {
    let (mut nodes, mut edges) = (0, 0);
    for line in s.lines() {
        let t = line.trim();
        if t.starts_with("classDiagram") || t.starts_with("%%") || t == "}" {
            continue;
        }
        if t.starts_with("class ") {
            nodes += 1;
        } else if t.contains("<|--") || t.contains("..|>") || t.contains("*--") || t.contains("-->")
        {
            edges += 1;
        }
    }
    (nodes, edges)
}

fn dot_counts(s: &str) -> (usize, usize) {
    let (mut nodes, mut edges) = (0, 0);
    for line in s.lines() {
        let t = line.trim();
        if !t.ends_with(';') {
            continue;
        }
        if t.starts_with("rankdir") || t.starts_with("node [") || t.starts_with("edge [") {
            continue;
        }
        if t.contains(" -> ") {
            edges += 1;
        } else {
            nodes += 1;
        }
    }
    (nodes, edges)
}

#[test]
fn emitters_agree_on_call_types_and_module_counts() {
    let root = copy_sample("emit-consistency");
    let (idx, a) = learn::learn(&root, &|_| {}).unwrap();

    // Call graph: Mermaid flowchart vs DOT vs the SVG node group.
    let m = export::mermaid(&idx, &a, &Diagram::CallGraph);
    let d = export::dot(&idx, &a, &Diagram::CallGraph);
    let (mn, me) = mermaid_flow_counts(&m);
    assert_eq!(
        (mn, me),
        dot_counts(&d),
        "call graph drifted:\nM\n{m}\nD\n{d}"
    );
    assert!(mn > 0, "sample must have call nodes");

    // Module graph: same shared model.
    let mm = export::mermaid(&idx, &a, &Diagram::Modules);
    let dm = export::dot(&idx, &a, &Diagram::Modules);
    let (mmn, mme) = mermaid_flow_counts(&mm);
    assert_eq!((mmn, mme), dot_counts(&dm), "module graph drifted");
    assert_eq!(mmn, a.modules.len(), "module nodes == Analysis.modules");

    // Type graph: Mermaid classDiagram vs DOT.
    let mt = export::mermaid(&idx, &a, &Diagram::Types);
    let dt = export::dot(&idx, &a, &Diagram::Types);
    let (mtn, mte) = mermaid_class_counts(&mt);
    assert_eq!(
        (mtn, mte),
        dot_counts(&dt),
        "type graph drifted:\nM\n{mt}\nD\n{dt}"
    );

    // SVG must carry the same node count for the call graph (no cap on 6 files).
    let svg = export::svg(&idx, &a, &Diagram::CallGraph);
    let svg_nodes = svg.matches("<rect class=\"node\"").count()
        + svg.matches("<polygon class=\"node\"").count();
    assert_eq!(
        svg_nodes, mn,
        "SVG call-graph node count must match Mermaid/DOT"
    );

    // The report embeds the exact same diagram bodies (no second renderer).
    let report = export::markdown_report(&idx, &a);
    for body in [&m, &mm, &mt] {
        assert!(
            report.contains(body.as_str()),
            "report does not embed a diagram verbatim"
        );
    }
    let _ = fs::remove_dir_all(&root);
}

// ---------------------------------------------------------------------------
// Frozen contract — version present, unknown version refused
// ---------------------------------------------------------------------------

#[test]
fn frozen_contract_version_present_and_unknown_version_refused() {
    let root = copy_sample("contract");
    let (_, a) = learn::learn(&root, &|_| {}).unwrap();

    // The written analysis artifact carries the frozen version.
    let raw: serde_json::Value =
        serde_json::from_slice(&fs::read(analysis::analysis_path(&root)).unwrap()).unwrap();
    assert_eq!(
        raw["version"],
        serde_json::json!(analysis::ANALYSIS_VERSION)
    );
    assert_eq!(a.version, analysis::ANALYSIS_VERSION);

    // Unknown version: refuse (load -> None), never misread.
    let mut v = raw.clone();
    v["version"] = serde_json::json!(analysis::ANALYSIS_VERSION + 1);
    fs::write(
        analysis::analysis_path(&root),
        serde_json::to_vec(&v).unwrap(),
    )
    .unwrap();
    assert!(
        analysis::load(&root).is_none(),
        "an unknown analysis version must be refused"
    );

    // Index artifact: version field present, unknown version invalidates the cache.
    let idx_raw: serde_json::Value =
        serde_json::from_slice(&fs::read(repowalk_core::store::cache_path(&root)).unwrap())
            .unwrap();
    assert_eq!(
        idx_raw["version"],
        serde_json::json!(repowalk_core::model::SCHEMA_VERSION)
    );
    let mut iv = idx_raw.clone();
    iv["version"] = serde_json::json!(repowalk_core::model::SCHEMA_VERSION + 1);
    fs::write(
        repowalk_core::store::cache_path(&root),
        serde_json::to_vec(&iv).unwrap(),
    )
    .unwrap();
    let files = repowalk_core::walk::walk(&root).unwrap();
    assert!(
        !repowalk_core::store::cache_valid(&root, &files),
        "an unknown index version must invalidate the cache"
    );

    let _ = fs::remove_dir_all(&root);
}
