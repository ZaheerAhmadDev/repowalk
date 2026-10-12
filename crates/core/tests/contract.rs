//! Contract tests against the FROZEN data model (plan §6, RSK-29).
//! A failure here means the contract broke — do NOT change model.rs to make
//! these pass; open a discussion instead.
//!
//! Equality is asserted through the serde representation (the wire contract),
//! since the frozen model deliberately derives no PartialEq.

use repowalk_core::model::{
    Confidence, Edge, EdgeKind, FileEntry, Index, Symbol, SymbolKind, SCHEMA_VERSION,
};
use std::path::Path;

fn temp_root(tag: &str) -> std::path::PathBuf {
    let p = std::env::temp_dir().join(format!("rw-contract-{}-{}", std::process::id(), tag));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p
}

fn sample() -> Index {
    let mut idx = Index::new("fixtures/sample");
    idx.files.push(FileEntry {
        path: "src/main.rs".into(),
        lang: "rust".into(),
        lines: 3,
        mtime: 1,
        is_entrypoint: true,
    });
    idx.symbols.push(Symbol {
        id: "src/main.rs:1".into(),
        name: "main".into(),
        kind: SymbolKind::Function,
        signature: "fn main()".into(),
        doc: "entry".into(),
        start_line: 1,
        end_line: 3,
        container: None,
        file: "src/main.rs".into(),
    });
    idx.edges.push(Edge {
        caller: "src/main.rs:1".into(),
        callee: "src/util.rs:5".into(),
        line: 2,
        kind: EdgeKind::Call,
        confidence: Confidence::NameOnly,
    });
    idx
}

fn json(v: &impl serde::Serialize) -> serde_json::Value {
    serde_json::to_value(v).expect("model type serializes")
}

#[test]
fn schema_version_is_frozen_at_one() {
    assert_eq!(SCHEMA_VERSION, 1);
    assert_eq!(Index::new("x").version, SCHEMA_VERSION);
}

#[test]
fn index_new_defaults_are_empty() {
    let idx = Index::new("some/repo");
    assert_eq!(idx.repo_root, "some/repo");
    assert!(idx.indexed_at.is_empty());
    assert!(idx.files.is_empty());
    assert!(idx.symbols.is_empty());
    assert!(idx.call_sites.is_empty());
    assert!(idx.edges.is_empty());
    assert!(idx.imports.is_empty());
}

#[test]
fn serde_roundtrip_preserves_index() {
    let idx = sample();
    let before = json(&idx);
    let text = serde_json::to_string(&idx).unwrap();
    let back: Index = serde_json::from_str(&text).expect("index deserializes");
    assert_eq!(json(&back), before, "roundtrip must be lossless");
}

#[test]
fn symbol_id_format_is_path_line() {
    let id = "src/main.rs:12";
    let (path, line) = id.rsplit_once(':').expect("symbol id must contain ':'");
    assert_eq!(path, "src/main.rs");
    assert_eq!(line.parse::<usize>().unwrap(), 12);
}

#[test]
fn enums_serialize_as_plain_strings() {
    assert_eq!(json(&Confidence::Exact), "Exact");
    assert_eq!(json(&Confidence::NameOnly), "NameOnly");
    assert_eq!(json(&Confidence::Ambiguous), "Ambiguous");
    assert_eq!(json(&Confidence::Unresolved), "Unresolved");
    assert_eq!(json(&EdgeKind::Call), "Call");
    assert_eq!(json(&EdgeKind::Import), "Import");
    assert_eq!(json(&SymbolKind::Function), "Function");
    assert_eq!(json(&SymbolKind::Class), "Class");
}

#[test]
fn store_cache_roundtrip_through_real_fs() {
    use repowalk_core::store;
    let root = temp_root("roundtrip");
    let idx = sample();
    store::save(&root, &idx).unwrap();
    assert!(store::cache_valid(&root, &idx.files));
    let loaded = store::load(&root).unwrap();
    assert_eq!(json(&loaded), json(&idx));
    assert_eq!(
        store::cache_path(&root),
        Path::new(&root).join(".repowalk/index.json")
    );
    let _ = std::fs::remove_dir_all(&root);
}
