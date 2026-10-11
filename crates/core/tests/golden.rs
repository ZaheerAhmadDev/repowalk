//! Golden-file test against fixtures/sample (plan §13, RSK-10).
//! Regenerate: UPDATE_GOLDEN=1 cargo test -p repowalk-core --test golden
//!
//! The compared form is canonical: mtimes zeroed, `indexed_at` cleared and
//! `repo_root` normalized, so the golden file is stable across machines.

use repowalk_core::model::{Index, SCHEMA_VERSION};
use repowalk_core::pipeline;
use std::path::{Path, PathBuf};

fn fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/sample")
        .canonicalize()
        .expect("fixtures/sample must exist")
}

fn golden_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/golden/index.json")
}

/// Canonical form: everything machine-dependent zeroed out.
fn canonical(idx: &Index) -> Index {
    let mut c = idx.clone();
    c.indexed_at = String::new();
    c.repo_root = "fixtures/sample".to_string();
    for f in &mut c.files {
        f.mtime = 0;
    }
    c
}

fn canonical_json(idx: &Index) -> String {
    let c = canonical(idx);
    serde_json::to_string_pretty(&c).expect("canonical index serializes")
}

#[test]
fn golden_index_matches_fixture() {
    let root = fixture_root();
    let idx = pipeline::build_index(&root, &|_| {}).expect("sample fixture indexes");

    assert_eq!(idx.version, SCHEMA_VERSION);
    assert!(idx.files.iter().any(|f| f.path == "main.rs"));
    assert!(idx.files.iter().any(|f| f.path == "tool.py"));
    assert!(
        !idx.symbols.is_empty() && !idx.edges.is_empty(),
        "sample must exercise the full pipeline"
    );

    let json = canonical_json(&idx);
    let golden = golden_path();
    if std::env::var("UPDATE_GOLDEN").is_ok() {
        std::fs::create_dir_all(golden.parent().unwrap()).unwrap();
        std::fs::write(&golden, &json).unwrap();
        eprintln!("golden regenerated at {}", golden.display());
        return;
    }

    let Ok(expected) = std::fs::read_to_string(&golden) else {
        panic!(
            "golden file missing at {} — run `UPDATE_GOLDEN=1 cargo test -p repowalk-core --test golden`",
            golden.display()
        );
    };
    assert_eq!(
        json, expected,
        "index drifted from fixtures/golden/index.json — if intentional, \
         re-run with UPDATE_GOLDEN=1 and review the diff"
    );
}
