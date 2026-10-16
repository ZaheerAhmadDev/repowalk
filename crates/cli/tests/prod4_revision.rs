//! PROD-4 regression: a query command must never mix a fresh index with a
//! stale analysis after an edit.
//!
//! The old `artifacts::load` reused a merely *loadable* `analysis.json` and
//! rebuilt the index fresh, so after a source edit a report showed fresh
//! `lines`/`symbols` next to stale `variables`/`cfgs` and exited 0. Every query
//! now runs one incremental `learn` pass, so the index and analysis fields move
//! together and a warm re-run re-analyzes nothing.
//!
//! `repowalk_core` is a normal dependency of this crate, so the integration test
//! can read the same `learn::reanalyzed` sidecar the CLI wrote.
use std::fs;
use std::path::PathBuf;
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_repowalk-cli");

fn temp_repo(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("rw-cli-prod4-{}-{}", std::process::id(), tag));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn run(args: &[&str]) -> String {
    let output = Command::new(BIN).args(args).output().expect("spawn");
    assert!(
        output.status.success(),
        "repowalk-cli {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("stdout is utf-8")
}

/// The first `"key":<digits>` in a flat JSON blob.
fn json_num(json: &str, key: &str) -> Option<u64> {
    let needle = format!("\"{key}\":");
    let start = json.find(&needle)? + needle.len();
    let digits: String = json[start..]
        .chars()
        .skip_while(|c| c.is_whitespace())
        .take_while(|c| c.is_ascii_digit())
        .collect();
    digits.parse().ok()
}

/// 3 lines, 1 symbol, 1 variable.
const SMALL: &str = "pub fn f() {\n    let alpha = 1;\n}\n";
/// 8 lines, 2 symbols, 2 variables (adds `gamma` and a whole new `g`).
const EDITED: &str = "pub fn f() {\n    let alpha = 1;\n    let gamma = 2;\n}\n\n\
                      pub fn g() -> i32 {\n    7\n}\n";

/// The card's repro: edit a source file between two `report --json` calls and
/// require the **analysis-derived** counts to change too, not just the index's.
#[test]
fn an_edit_between_reports_refreshes_the_analysis_derived_counts() {
    let repo = temp_repo("edit");
    fs::write(repo.join("m.rs"), SMALL).unwrap();
    let path = repo.to_str().unwrap();

    let cold = run(&["report", "--json", path]);
    assert_eq!(json_num(&cold, "lines"), Some(3), "cold lines: {cold}");
    assert_eq!(json_num(&cold, "symbols"), Some(1), "cold symbols: {cold}");
    assert_eq!(
        json_num(&cold, "variables"),
        Some(1),
        "cold variables: {cold}"
    );

    fs::write(repo.join("m.rs"), EDITED).unwrap();
    let after = run(&["report", "--json", path]);
    assert_eq!(json_num(&after, "lines"), Some(8), "fresh lines: {after}");
    assert_eq!(
        json_num(&after, "symbols"),
        Some(2),
        "fresh symbols: {after}"
    );
    assert_eq!(
        json_num(&after, "variables"),
        Some(2),
        "analysis variables must be fresh too (was stale at 1): {after}"
    );

    // The analysis-derived surfaces agree with the refreshed artifact.
    let cx = run(&["complexity", "--by", "function", path]);
    assert!(cx.contains("2 functions"), "both functions present: {cx}");
    let vars = run(&["variables", "m.rs", path]);
    assert!(
        vars.contains("(2 declared)"),
        "both variables present: {vars}"
    );
    assert!(
        vars.contains("gamma"),
        "the new variable is present: {vars}"
    );

    let _ = fs::remove_dir_all(&repo);
}

/// The incremental claim at the CLI surface: a warm query re-analyzes **0**
/// files and a one-file edit re-analyzes exactly **1**.
#[test]
fn warm_query_reanalyzes_zero_and_one_edit_reanalyzes_one() {
    let repo = temp_repo("warm");
    fs::write(repo.join("a.rs"), SMALL).unwrap();
    fs::write(repo.join("b.rs"), "pub fn h() {\n    let beta = 1;\n}\n").unwrap();
    let path = repo.to_str().unwrap();

    run(&["report", "--json", path]);
    assert!(
        repowalk_core::learn::reanalyzed(&repo) >= 2,
        "cold query analyzes every analyzable file"
    );

    run(&["report", "--json", path]);
    assert_eq!(
        repowalk_core::learn::reanalyzed(&repo),
        0,
        "a warm query must re-analyze nothing"
    );

    fs::write(repo.join("a.rs"), EDITED).unwrap();
    run(&["report", "--json", path]);
    assert_eq!(
        repowalk_core::learn::reanalyzed(&repo),
        1,
        "a one-file edit must re-analyze exactly that file"
    );

    let _ = fs::remove_dir_all(&repo);
}
