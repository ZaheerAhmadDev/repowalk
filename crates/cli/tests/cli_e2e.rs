//! End-to-end smoke tests: run the real `repowalk-cli` binary against a copy of
//! `fixtures/sample` and assert on stdout. These complement the unit tests by
//! exercising argument parsing, the pipeline, and rendering together.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Absolute path to the binary Cargo built for this test run.
const BIN: &str = env!("CARGO_BIN_EXE_repowalk-cli");

fn fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("fixtures/sample")
        .canonicalize()
        .expect("fixtures/sample must exist")
}

fn copy_dir(src: &Path, dst: &Path) {
    fs::create_dir_all(dst).unwrap();
    for entry in fs::read_dir(src).unwrap() {
        let entry = entry.unwrap();
        let to = dst.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &to);
        } else {
            fs::copy(entry.path(), &to).unwrap();
        }
    }
}

/// Fresh copy of the sample fixture in a unique temp dir (avoids writing the
/// `.repowalk` cache into the checked-in fixture).
fn temp_repo(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("rw-cli-e2e-{}-{}", std::process::id(), tag));
    let _ = fs::remove_dir_all(&dir);
    copy_dir(&fixture_root(), &dir);
    dir
}

fn run(args: &[&str]) -> String {
    let output = Command::new(BIN)
        .args(args)
        .output()
        .expect("spawn repowalk-cli");
    assert!(
        output.status.success(),
        "repowalk-cli {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("stdout is utf-8")
}

#[test]
fn map_prints_languages_loc_entrypoints_and_health() {
    let repo = temp_repo("map");
    let out = run(&["map", repo.to_str().unwrap()]);

    assert!(out.contains("Languages:"), "languages line missing:\n{out}");
    assert!(out.contains("rust"), "rust lang missing:\n{out}");
    assert!(out.contains("python"), "python lang missing:\n{out}");
    assert!(out.contains("LOC:"), "loc line missing:\n{out}");
    assert!(
        out.contains("Entrypoints"),
        "entrypoints line missing:\n{out}"
    );
    assert!(out.contains("Module tree"), "module tree missing:\n{out}");
    assert!(
        out.contains("cycles") && out.contains("unresolved"),
        "health line missing:\n{out}"
    );

    let _ = fs::remove_dir_all(&repo);
}

#[test]
fn tour_lists_numbered_reading_order() {
    let repo = temp_repo("tour");
    let out = run(&["tour", repo.to_str().unwrap()]);

    assert!(out.contains("Tour"), "tour header missing:\n{out}");
    assert!(out.contains("1. a.rs"), "first tour item missing:\n{out}");
    assert!(out.contains("2. b.rs"), "second tour item missing:\n{out}");

    let _ = fs::remove_dir_all(&repo);
}

#[test]
fn explain_resolves_a_known_symbol_with_callers() {
    let repo = temp_repo("explain");
    let out = run(&["explain", "greet", repo.to_str().unwrap()]);

    assert!(out.contains("greet"), "symbol name missing:\n{out}");
    assert!(out.contains("util.rs:"), "file:line missing:\n{out}");
    assert!(out.contains("Callers"), "callers section missing:\n{out}");
    assert!(out.contains("helper"), "caller not listed:\n{out}");

    let _ = fs::remove_dir_all(&repo);
}

#[test]
fn explain_reports_unknown_symbol_honestly() {
    let repo = temp_repo("explain-missing");
    let out = run(&["explain", "definitely_not_a_symbol", repo.to_str().unwrap()]);

    assert!(
        out.contains("not found"),
        "expected not-found message:\n{out}"
    );

    let _ = fs::remove_dir_all(&repo);
}

#[test]
fn find_returns_ranked_hits() {
    let repo = temp_repo("find");
    let out = run(&["find", "greet", repo.to_str().unwrap()]);

    assert!(out.contains("util.rs:"), "expected a util.rs hit:\n{out}");
    assert!(out.contains("greet"), "expected the greet symbol:\n{out}");

    let _ = fs::remove_dir_all(&repo);
}

#[test]
fn unknown_subcommand_exits_nonzero() {
    let output = Command::new(BIN)
        .arg("not-a-real-command")
        .output()
        .expect("spawn repowalk-cli");
    assert!(!output.status.success(), "unknown command must fail");
}
