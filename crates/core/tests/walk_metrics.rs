use std::fs;
use std::path::PathBuf;

use repowalk_core::metrics;
use repowalk_core::model::Index;
use repowalk_core::walk;

fn fixture(name: &str) -> PathBuf {
    let mut dir = std::env::temp_dir();
    dir.push(format!("repowalk-test-{}-{}", name, std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn walk_collects_sources_and_skips_build_dirs() {
    let root = fixture("walk");
    fs::create_dir_all(root.join("src")).unwrap();
    fs::create_dir_all(root.join("target")).unwrap();
    fs::create_dir_all(root.join(".git")).unwrap();

    fs::write(root.join("src/main.rs"), "fn main() {\n    run();\n}\n").unwrap();
    fs::write(root.join("src/lib.rs"), "pub fn add(a: i32, b: i32) -> i32 { a + b }\n").unwrap();
    fs::write(root.join("target/junk.rs"), "fn nope() {}\n").unwrap();
    fs::write(root.join(".git/config"), "ignored\n").unwrap();

    let files = walk::walk(&root).unwrap();
    let paths: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();

    assert!(paths.contains(&"src/main.rs"));
    assert!(paths.contains(&"src/lib.rs"));
    assert!(!paths.iter().any(|p| p.starts_with("target/")));
    assert!(!paths.iter().any(|p| p.starts_with(".git/")));

    let main = files.iter().find(|f| f.path == "src/main.rs").unwrap();
    assert_eq!(main.lang, "rust");
    assert_eq!(main.lines, 3);
    assert!(main.is_entrypoint);

    fs::remove_dir_all(&root).ok();
}

#[test]
fn metrics_aggregate_languages_and_largest_files() {
    let root = fixture("metrics");
    fs::create_dir_all(root.join("a")).unwrap();
    fs::write(root.join("a/main.rs"), "fn main() {\n    one();\n    two();\n}\n").unwrap();
    fs::write(root.join("a/other.rs"), "pub fn hi() {}\n").unwrap();
    fs::write(root.join("a/tool.py"), "print('hi')\n").unwrap();

    let files = walk::walk(&root).unwrap();
    let mut index = Index::new(root.to_string_lossy());
    index.files = files;
    let m = metrics::compute(&index);

    assert_eq!(m.total_loc, 6);
    assert_eq!(m.entrypoints, vec!["a/main.rs".to_string()]);

    let rust = m.langs.iter().find(|(l, _)| l == "rust").unwrap();
    assert_eq!(rust.1, 2);

    assert_eq!(m.top_files[0], ("a/main.rs".to_string(), 4));

    fs::remove_dir_all(&root).ok();
}
