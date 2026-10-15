//! End-to-end tests for the v2 query commands (PROD-3): `report`, `graph`,
//! `variables`, `complexity`, `refs`. These run the real `repowalk-cli` binary
//! against a temp copy of `fixtures/sample`, exercising argument parsing, the
//! learn/analysis pipeline, rendering, and honest error exits together.
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

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

/// Fresh copy of the sample fixture in a unique temp dir (keeps the checked-in
/// fixture's `.repowalk` cache untouched).
fn temp_repo(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("rw-cli-v2-{}-{}", std::process::id(), tag));
    let _ = fs::remove_dir_all(&dir);
    copy_dir(&fixture_root(), &dir);
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

/// Run expecting a nonzero exit; returns stderr for message assertions.
fn run_err(args: &[&str]) -> String {
    let output = Command::new(BIN).args(args).output().expect("spawn");
    assert!(
        !output.status.success(),
        "repowalk-cli {args:?} unexpectedly succeeded"
    );
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn report_renders_markdown_with_sections() {
    let repo = temp_repo("report");
    let out = run(&["report", repo.to_str().unwrap()]);
    assert!(
        out.starts_with("# repowalk architecture report"),
        "heading:\n{out}"
    );
    assert!(out.contains("## Overview"), "overview:\n{out}");
    assert!(out.contains("## Complexity"), "complexity:\n{out}");
    assert!(out.contains("## Guardrails"), "guardrails:\n{out}");
    assert!(!out.contains('\u{1b}'), "no ANSI in Markdown");
    let _ = fs::remove_dir_all(&repo);
}

#[test]
fn report_exits_nonzero_on_error_severity_rule_finding() {
    let repo = temp_repo("report-rules");
    // fixtures/sample carries the a.rs <-> b.rs import cycle; a no_cycles rule
    // over the source files turns it into an Error-severity finding.
    fs::create_dir_all(repo.join(".repowalk")).unwrap();
    fs::write(
        repo.join(".repowalk").join("rules.json"),
        r#"{"no_cycles":["*.rs"]}"#,
    )
    .unwrap();

    let output = Command::new(BIN)
        .args(["report", repo.to_str().unwrap()])
        .output()
        .expect("spawn");
    assert!(
        !output.status.success(),
        "an Error-severity finding must exit nonzero"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("## Guardrails"),
        "report still prints:\n{stdout}"
    );
    assert!(stdout.contains("no_cycles"), "finding listed:\n{stdout}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Error-severity"), "stderr:\n{stderr}");

    // A non-violating rule set keeps the exit code at 0.
    fs::write(
        repo.join(".repowalk").join("rules.json"),
        r#"{"max_cyclomatic":100}"#,
    )
    .unwrap();
    run(&["report", repo.to_str().unwrap()]);

    let _ = fs::remove_dir_all(&repo);
}

#[test]
fn report_json_emits_underlying_data() {
    let repo = temp_repo("report-json");
    let out = run(&["report", "--json", repo.to_str().unwrap()]);
    assert!(out.contains("\"overview\":"), "{out}");
    assert!(out.contains("\"complexity\":"), "{out}");
    assert!(out.contains("\"hotspots\":["), "{out}");
    assert!(out.contains("\"variables\":"), "{out}");
    assert!(out.contains("\"guardrails\":"), "{out}");
    let _ = fs::remove_dir_all(&repo);
}

#[test]
fn graph_mermaid_and_dot_and_svg() {
    let repo = temp_repo("graph");
    let path = repo.to_str().unwrap();

    let mermaid = run(&["graph", "--format", "mermaid", "--scope", "call", path]);
    assert!(mermaid.starts_with("flowchart"), "mermaid:\n{mermaid}");

    let dot = run(&["graph", "--format", "dot", "--scope", "types", path]);
    assert!(dot.starts_with("digraph"), "dot:\n{dot}");

    let svg = run(&["graph", "--format", "svg", "--scope", "modules", path]);
    assert!(svg.contains("<svg"), "svg:\n{svg}");

    let _ = fs::remove_dir_all(&repo);
}

#[test]
fn graph_cfg_requires_symbol_and_rejects_unknown() {
    let repo = temp_repo("graph-cfg");
    let path = repo.to_str().unwrap();

    let out = run_err(&["graph", "--scope", "cfg", path]);
    assert!(out.contains("--scope cfg requires --symbol"), "{out}");

    let err = run_err(&["graph", "--scope", "cfg", "--symbol", "nope", path]);
    assert!(err.contains("unknown symbol: nope"), "{err}");

    // A resolvable function renders a CFG.
    let cfg = run(&["graph", "--scope", "cfg", "--symbol", "greet", path]);
    assert!(cfg.starts_with("flowchart TD"), "cfg:\n{cfg}");

    let _ = fs::remove_dir_all(&repo);
}

#[test]
fn variables_lists_a_files_rows_and_json() {
    let repo = temp_repo("variables");
    let path = repo.to_str().unwrap();

    let table = run(&["variables", "main.rs", path]);
    assert!(table.contains("Variables — main.rs"), "{table}");
    assert!(table.contains("person"), "{table}");

    let json = run(&["variables", "--json", "main.rs", path]);
    assert!(json.contains("\"file\":\"main.rs\""), "{json}");
    assert!(json.contains("\"variables\":["), "{json}");

    let err = run_err(&["variables", "nope.rs", path]);
    assert!(err.contains("no such file in index: nope.rs"), "{err}");

    let _ = fs::remove_dir_all(&repo);
}

#[test]
fn complexity_function_and_file_views() {
    let repo = temp_repo("complexity");
    let path = repo.to_str().unwrap();

    let funcs = run(&["complexity", "--top", "3", path]);
    assert!(funcs.contains("Complexity"), "{funcs}");
    assert!(
        funcs.contains("cyclomatic"),
        "expected a table header:\n{funcs}"
    );

    let files = run(&["complexity", "--by", "file", "--json", path]);
    assert!(files.contains("\"by\":\"file\""), "{files}");
    assert!(files.contains("\"rows\":["), "{files}");

    let _ = fs::remove_dir_all(&repo);
}

#[test]
fn refs_finds_use_sites_and_rejects_unknown() {
    let repo = temp_repo("refs");
    let path = repo.to_str().unwrap();

    let out = run(&["refs", "greet", path]);
    assert!(out.contains("References"), "{out}");
    assert!(out.contains("util.rs:"), "{out}");

    let json = run(&["refs", "--json", "greet", path]);
    assert!(json.contains("\"count\":"), "{json}");
    assert!(json.contains("\"resolved\":["), "{json}");

    let err = run_err(&["refs", "definitely_missing", path]);
    assert!(err.contains("unknown symbol or variable"), "{err}");

    let _ = fs::remove_dir_all(&repo);
}

#[test]
fn missing_repo_path_is_a_clear_error() {
    let err = run_err(&["report", "/tmp/rw-cli-definitely-missing-path"]);
    assert!(err.contains("repo path not found"), "{err}");
}
