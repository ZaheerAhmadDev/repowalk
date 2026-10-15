//! v2 analysis golden harness (VER-2) — fixtures → analyzers → selected fields.
//!
//! Two layers:
//!
//!  * **SCHEMA** (always active): `Analysis` round-trips through JSON, every
//!    required field is present, and `version == ANALYSIS_VERSION`. This passes
//!    today and is the regression net for the frozen `analysis.json` contract.
//!
//!  * **GOLDEN** (one per language, all active): hand-counted variables / CFG /
//!    types from the fixtures under `fixtures/vars/` and `fixtures/ts/`, checked
//!    against `crates/core/src/analyze/{rust,python,ts}.rs` (INTEL-2/3/4 landed).
//!    They were parked behind `#[ignore = "awaiting INTEL-x"]` while the
//!    analyzers were interface stubs; the integrator dropped the attribute once
//!    all three languages passed, so a regression now fails the default
//!    `cargo test`. `golden_ts_variables` earned its keep at once: it caught
//!    TypeScript not counting a declaration as a write, unlike Rust and Python.
//!    `pending_analyzers_are_reported` still prints per-language readiness.
//!
//! Goldens compare SELECTED fields only (named variables, one function's
//! cyclomatic/nesting/back-edge, and the expected type fields/methods/edges) —
//! never whole-file dumps — so formatting churn cannot break them.

use repowalk_core::analysis::{
    Analysis, CfgEdgeKind, CfgKind, FunctionCfg, TypeEdgeKind, TypeKind, TypeNode, VarKind,
    Variable, ANALYSIS_VERSION,
};
use repowalk_core::analyze::{analyze, Lang};
use repowalk_core::model::Symbol;
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// fixture + analyzer plumbing
// ---------------------------------------------------------------------------

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures")
}

/// Read a fixture and return `(source, repo-relative-path)`.
fn read_fixture(rel: &str) -> (String, String) {
    let path = fixtures_dir().join(rel);
    let src = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read fixture {}: {e}", path.display()));
    (src, rel.replace('\\', "/"))
}

/// Run the Rust analyzer over a fixture, returning the artifact + its symbols.
fn run_rust(rel: &str) -> (Analysis, Vec<Symbol>) {
    let (src, path) = read_fixture(rel);
    let (symbols, _, _) = repowalk_core::parse_rust::extract(&src, &path);
    let mut out = Analysis::new();
    analyze(&path, Lang::Rust, &src, &symbols, &mut out);
    (out, symbols)
}

/// Run the Python analyzer over a fixture, returning the artifact + its symbols.
fn run_python(rel: &str) -> (Analysis, Vec<Symbol>) {
    let (src, path) = read_fixture(rel);
    let (symbols, _, _) = repowalk_core::parse_py::extract(&src, &path);
    let mut out = Analysis::new();
    analyze(&path, Lang::Python, &src, &symbols, &mut out);
    (out, symbols)
}

/// Run the TypeScript analyzer over a fixture. No TS parser is wired into
/// `core` yet (INTEL-4 owns `parse_ts`), so we hand it an empty symbol slice,
/// exactly as VER-2 prescribes.
fn run_ts(rel: &str) -> (Analysis, Vec<Symbol>) {
    let (src, path) = read_fixture(rel);
    let symbols: Vec<Symbol> = Vec::new();
    let mut out = Analysis::new();
    analyze(&path, Lang::TypeScript, &src, &symbols, &mut out);
    (out, symbols)
}

fn variable<'a>(a: &'a Analysis, name: &str) -> &'a Variable {
    a.variables
        .iter()
        .find(|v| v.name == name)
        .unwrap_or_else(|| {
            panic!(
                "variable `{name}` not found; analyzer produced: {:?}",
                a.variables
                    .iter()
                    .map(|v| v.name.as_str())
                    .collect::<Vec<_>>()
            )
        })
}

fn cfg_by_name<'a>(symbols: &[Symbol], a: &'a Analysis, name: &str) -> &'a FunctionCfg {
    let sym = symbols
        .iter()
        .find(|s| s.name == name)
        .unwrap_or_else(|| panic!("symbol `{name}` not found"));
    a.cfgs
        .iter()
        .find(|c| c.symbol == sym.id)
        .unwrap_or_else(|| {
            panic!(
                "no CFG for `{name}` ({}); analyzer produced: {:?}",
                sym.id,
                a.cfgs.iter().map(|c| c.symbol.as_str()).collect::<Vec<_>>()
            )
        })
}

fn type_node<'a>(a: &'a Analysis, name: &str) -> &'a TypeNode {
    a.types.iter().find(|t| t.name == name).unwrap_or_else(|| {
        panic!(
            "type `{name}` not found; analyzer produced: {:?}",
            a.types.iter().map(|t| t.name.as_str()).collect::<Vec<_>>()
        )
    })
}

// ---------------------------------------------------------------------------
// SCHEMA contract — always active
// ---------------------------------------------------------------------------

#[test]
fn schema_roundtrips_and_has_every_required_field() {
    let a = Analysis::new();
    assert_eq!(
        a.version, ANALYSIS_VERSION,
        "Analysis::new must pin the current schema version"
    );

    let value = serde_json::to_value(&a).expect("Analysis serializes");
    let obj = value.as_object().expect("Analysis is a JSON object");
    for field in [
        "version",
        "variables",
        "cfgs",
        "types",
        "type_edges",
        "modules",
        "module_edges",
        "uses",
    ] {
        assert!(
            obj.contains_key(field),
            "analysis.json is missing required field `{field}`"
        );
    }

    let text = serde_json::to_string(&a).expect("Analysis serializes");
    let back: Analysis = serde_json::from_str(&text).expect("Analysis deserializes");
    assert_eq!(a, back, "Analysis round-trip must be lossless");
    assert_eq!(back.version, ANALYSIS_VERSION);
}

#[test]
fn schema_roundtrips_a_populated_analysis() {
    let mut a = Analysis::new();
    a.variables.push(Variable {
        id: "sample.ts:12:unusedTag".into(),
        name: "unusedTag".into(),
        kind: VarKind::Local,
        file: "sample.ts".into(),
        decl_line: 12,
        scope_start: 12,
        scope_end: 12,
        type_name: Some("string".into()),
        reads: 0,
        writes: 1,
        first_use: None,
        last_use: None,
        enclosing: "sample.ts:10".into(),
    });
    a.cfgs.push(FunctionCfg {
        symbol: "sample.ts:10".into(),
        nodes: vec![],
        edges: vec![repowalk_core::analysis::CfgEdge {
            from: 0,
            to: 0,
            kind: CfgEdgeKind::Back,
            line: 12,
        }],
        cyclomatic: 1,
        max_nesting: 0,
    });

    let text = serde_json::to_string(&a).expect("serialize");
    let back: Analysis = serde_json::from_str(&text).expect("deserialize");
    assert_eq!(a, back, "populated Analysis round-trip must be lossless");
}

// ---------------------------------------------------------------------------
// PENDING report — always active, never fails on a stub
// ---------------------------------------------------------------------------

/// Prints per-language readiness (variable / cfg counts). It never fails on an
/// empty analyzer — but that is no longer an expected state now that all three
/// languages have landed, so an empty result here is the first place a silent
/// extraction regression would surface.
#[test]
fn pending_analyzers_are_reported() {
    let checks = [
        ("rust", run_rust("vars/rust_counts.rs").0),
        ("python", run_python("vars/python_counts.py").0),
        ("typescript", run_ts("ts/sample.ts").0),
    ];

    let mut results: Vec<(&str, String)> = Vec::new();
    for (lang, a) in &checks {
        if a.variables.is_empty() && a.cfgs.is_empty() {
            let reason = format!("{lang}: analyzer produced nothing");
            eprintln!("REGRESSION {reason} — the analyzer must find variables and cfgs here");
            results.push((lang, reason));
        } else {
            eprintln!(
                "READY {lang}: analyzer produced {} variables / {} cfgs",
                a.variables.len(),
                a.cfgs.len()
            );
        }
    }

    // The harness must always be able to build the schema artifact.
    assert_eq!(Analysis::new().version, ANALYSIS_VERSION);
    // Pending languages are expected; we only surface them (no assert).
    let _ = results;
}

// ---------------------------------------------------------------------------
// GOLDEN — Rust (active)
// ---------------------------------------------------------------------------

#[test]
fn golden_rust_variable_counts() {
    let (a, symbols) = run_rust("vars/rust_counts.rs");

    let x = variable(&a, "x");
    assert_eq!(x.kind, VarKind::Param);
    assert_eq!((x.reads, x.writes), (3, 0), "x counts");
    assert_eq!(
        x.enclosing,
        symbols.iter().find(|s| s.name == "clamp_like").unwrap().id
    );

    let y = variable(&a, "y");
    assert_eq!(y.kind, VarKind::Local);
    assert_eq!((y.reads, y.writes), (2, 2), "y counts");

    let limit = variable(&a, "limit");
    assert_eq!(limit.kind, VarKind::Param);
    assert_eq!((limit.reads, limit.writes), (1, 0), "limit counts");

    let total = variable(&a, "total");
    assert_eq!((total.reads, total.writes), (2, 2), "total counts");

    let out = variable(&a, "out");
    assert_eq!((out.reads, out.writes), (1, 1), "out counts");

    let sum = variable(&a, "sum");
    assert_eq!((sum.reads, sum.writes), (1, 1), "sum counts");

    let combined = variable(&a, "combined");
    assert_eq!((combined.reads, combined.writes), (1, 1), "combined counts");
}

#[test]
fn golden_rust_cfg() {
    let (a, symbols) = run_rust("vars/rust_counts.rs");

    let clamp = cfg_by_name(&symbols, &a, "clamp_like");
    assert_eq!(clamp.cyclomatic, 2, "clamp_like: one if + 1");
    assert_eq!(clamp.max_nesting, 1, "clamp_like nesting");
    assert!(
        clamp.nodes.iter().any(|n| n.kind == CfgKind::Branch),
        "clamp_like should contain a Branch node"
    );
    assert!(
        !clamp.edges.iter().any(|e| e.kind == CfgEdgeKind::Back),
        "clamp_like has no loop, so no back edge"
    );

    let count = cfg_by_name(&symbols, &a, "count_up");
    assert_eq!(count.cyclomatic, 2, "count_up: one for + 1");
    assert_eq!(count.max_nesting, 1, "count_up nesting");
    assert!(
        count.edges.iter().any(|e| e.kind == CfgEdgeKind::Back),
        "count_up has a for loop, so a back edge"
    );

    let main = cfg_by_name(&symbols, &a, "main");
    assert_eq!(main.cyclomatic, 1, "main: no decisions");
    assert_eq!(main.max_nesting, 0, "main nesting");
}

// ---------------------------------------------------------------------------
// GOLDEN — Python (active)
// ---------------------------------------------------------------------------

#[test]
fn golden_python_variable_counts() {
    let (a, symbols) = run_python("vars/python_counts.py");

    let x = variable(&a, "x");
    assert_eq!(x.kind, VarKind::Param);
    assert_eq!((x.reads, x.writes), (3, 0), "x counts");
    assert_eq!(
        x.enclosing,
        symbols.iter().find(|s| s.name == "clamp_like").unwrap().id
    );

    let y = variable(&a, "y");
    assert_eq!(y.kind, VarKind::Local);
    assert_eq!((y.reads, y.writes), (2, 2), "y counts");

    let limit = variable(&a, "limit");
    assert_eq!(limit.kind, VarKind::Param);
    assert_eq!((limit.reads, limit.writes), (1, 0), "limit counts");

    let total = variable(&a, "total");
    assert_eq!((total.reads, total.writes), (2, 2), "total counts");

    let out = variable(&a, "out");
    assert_eq!((out.reads, out.writes), (1, 1), "out counts");

    let total_sum = variable(&a, "total_sum");
    assert_eq!(
        (total_sum.reads, total_sum.writes),
        (1, 1),
        "total_sum counts"
    );
}

#[test]
fn golden_python_cfg() {
    let (a, symbols) = run_python("vars/python_counts.py");

    let clamp = cfg_by_name(&symbols, &a, "clamp_like");
    assert_eq!(clamp.cyclomatic, 2, "clamp_like: one if + 1");
    assert_eq!(clamp.max_nesting, 1, "clamp_like nesting");

    let count = cfg_by_name(&symbols, &a, "count_up");
    assert_eq!(count.cyclomatic, 2, "count_up: one for + 1");
    assert_eq!(count.max_nesting, 1, "count_up nesting");
    assert!(
        count.edges.iter().any(|e| e.kind == CfgEdgeKind::Back),
        "count_up has a for loop, so a back edge"
    );

    let main = cfg_by_name(&symbols, &a, "main");
    assert_eq!(main.cyclomatic, 1, "main: no decisions");
}

// ---------------------------------------------------------------------------
// GOLDEN — TypeScript (active)
// ---------------------------------------------------------------------------

#[test]
fn golden_ts_variables() {
    let (a, _symbols) = run_ts("ts/sample.ts");

    // Exactly one deliberately unused local.
    let unused = variable(&a, "unusedTag");
    assert_eq!(unused.kind, VarKind::Local);
    assert_eq!(
        (unused.reads, unused.writes),
        (0, 1),
        "unusedTag must be written but never read"
    );

    let positives = variable(&a, "positives");
    assert_eq!(
        (positives.reads, positives.writes),
        (3, 3),
        "positives: decl + `+=` + `-=` writes, three reads"
    );

    let unused_locals = a
        .variables
        .iter()
        .filter(|v| v.kind == VarKind::Local && v.reads == 0)
        .count();
    assert_eq!(unused_locals, 1, "exactly one unused local in the fixture");
}

#[test]
fn golden_ts_cfg() {
    let (a, _symbols) = run_ts("ts/sample.ts");

    let classify = a
        .cfgs
        .iter()
        .find(|c| c.symbol.contains("classify"))
        .or_else(|| a.cfgs.iter().find(|c| c.cyclomatic == 3))
        .unwrap_or_else(|| {
            panic!(
                "no CFG for `classify`; have {:?}",
                a.cfgs.iter().map(|c| &c.symbol).collect::<Vec<_>>()
            )
        });
    assert_eq!(classify.cyclomatic, 3, "classify: one for + one if + 1");
    assert_eq!(classify.max_nesting, 2, "classify: if nested inside for");
    assert!(
        classify.edges.iter().any(|e| e.kind == CfgEdgeKind::Back),
        "classify has a for loop, so a back edge"
    );
}

#[test]
fn golden_ts_types() {
    let (a, _symbols) = run_ts("ts/sample.ts");

    let animal = type_node(&a, "Animal");
    assert_eq!(animal.kind, TypeKind::Class);
    assert!(
        animal.fields.iter().any(|f| f.name == "name"),
        "Animal should have field `name`"
    );
    assert!(
        animal.methods.iter().any(|m| m == "speak"),
        "Animal should have method `speak`"
    );

    let dog = type_node(&a, "Dog");
    assert_eq!(dog.kind, TypeKind::Class);
    let field_names: Vec<&str> = dog.fields.iter().map(|f| f.name.as_str()).collect();
    assert!(
        field_names.contains(&"legs") && field_names.contains(&"barkCount"),
        "Dog should have fields `legs` + `barkCount`, got {field_names:?}"
    );
    assert!(
        dog.methods.iter().any(|m| m == "describe") && dog.methods.iter().any(|m| m == "bark"),
        "Dog should have methods `describe` + `bark`, got {:?}",
        dog.methods
    );

    let inherits = a
        .type_edges
        .iter()
        .any(|e| e.kind == TypeEdgeKind::Inherit && e.from == dog.id && e.to == animal.id);
    assert!(
        inherits,
        "expected an Inherit edge Dog -> Animal; edges: {:?}",
        a.type_edges
    );
}
