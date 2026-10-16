//! VER-3 independently verifies the CLI half of the v2 surface (TL-VER).
//!
//! The core battery (`crates/core/tests/v2_acceptance.rs`) judges `learn()` in
//! memory; these tests judge the **persisted artifact** the CLI actually reads,
//! so a green core test plus a stale `analysis.json` cannot pass unnoticed.
//!
//! * report `--json` aggregates must agree with the diagram emitters (one model).
//! * a warm re-report is deterministic (artifact round-trip, no drift).
//! * an unknown-version artifact must be refused, never misread (regression guard
//!   for card CORE-11, which VER-3 found and CORE-07 fixed).
//!
//! No `serde_json` here: the CLI crate has no such dependency (and the battery
//! adds none). The JSON is read with small, explicit string scans.
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
        // Never copy the checked-in `.repowalk` cache: this battery must
        // exercise a real cold learn, not the fixture's stale artifact.
        if entry.file_name() == ".repowalk" {
            continue;
        }
        let to = dst.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &to);
        } else {
            fs::copy(entry.path(), &to).unwrap();
        }
    }
}

fn temp_repo(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("rw-cli-acc-{}-{}", std::process::id(), tag));
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

/// `Some(n)` for the first `"key":<digits>` occurrence in a flat JSON blob.
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

fn flow_node_count(diagram: &str) -> usize {
    diagram
        .lines()
        .map(str::trim)
        .filter(|t| {
            !t.is_empty()
                && !t.starts_with("flowchart")
                && !t.starts_with("%%")
                && !t.contains("-->")
                && !t.contains("-.->")
        })
        .count()
}

fn dot_node_count(diagram: &str) -> usize {
    diagram
        .lines()
        .map(str::trim)
        .filter(|t| {
            t.ends_with(';')
                && !t.starts_with("rankdir")
                && !t.starts_with("node [")
                && !t.starts_with("edge [")
                && !t.contains(" -> ")
        })
        .count()
}

/// The CLI's own JSON aggregates must equal the node counts of the CLI's own
/// emitters: Mermaid and DOT (and the report) come from one shared model.
#[test]
fn report_json_aggregates_match_the_emitters() {
    let repo = temp_repo("agree");
    let path = repo.to_str().unwrap();

    let json = run(&["report", "--json", path]);
    let modules = json_num(&json, "modules").expect("overview.modules");
    let types = json_num(&json, "types").expect("overview.types");
    let variables = json_num(&json, "variables").expect("overview.variables");
    let symbols = json_num(&json, "symbols").expect("overview.symbols");

    let modules_mermaid = run(&["graph", "--format", "mermaid", "--scope", "modules", path]);
    assert_eq!(
        modules as usize,
        flow_node_count(&modules_mermaid),
        "report modules != Mermaid module nodes"
    );

    let modules_dot = run(&["graph", "--format", "dot", "--scope", "modules", path]);
    assert_eq!(
        flow_node_count(&modules_mermaid),
        dot_node_count(&modules_dot),
        "Mermaid and DOT module nodes disagree"
    );

    let types_dot = run(&["graph", "--format", "dot", "--scope", "types", path]);
    assert_eq!(
        types as usize,
        dot_node_count(&types_dot),
        "report types != DOT type nodes"
    );

    assert_eq!(variables, 5, "fixtures/sample has 5 variables");
    assert!(symbols > 0);

    let _ = fs::remove_dir_all(&repo);
}

/// Two reports over the same warm artifact must be byte-identical: the artifact
/// round-trips and the emitters are deterministic.
#[test]
fn report_is_byte_identical_on_a_warm_artifact() {
    let repo = temp_repo("deterministic");
    let path = repo.to_str().unwrap();

    let first = run(&["report", "--json", path]);
    assert!(first.contains("\"overview\":"));
    let second = run(&["report", "--json", path]);
    assert_eq!(first, second, "report --json drifted on a warm artifact");

    let md1 = run(&["report", path]);
    let md2 = run(&["report", path]);
    assert_eq!(md1, md2, "report Markdown drifted on a warm artifact");

    let _ = fs::remove_dir_all(&repo);
}

/// Regression guard for card CORE-11 (found by VER-3). Before CORE-07's fix, an
/// unknown-version `analysis.json` was refused by `analysis::load`, but the CLI's
/// first call rewrote and persisted a version-current, *empty* artifact; the next
/// call then silently reported 0 variables / 0 types with exit 0 — a misread,
/// exactly what R-21 forbids. The artifact must instead be rebuilt.
#[test]
fn unknown_version_artifact_is_not_misread_as_empty() {
    let repo = temp_repo("unknown-version");
    let path = repo.to_str().unwrap();

    let cold = run(&["report", "--json", path]);
    assert!(json_num(&cold, "variables").unwrap() > 0, "cold baseline");

    // Tamper the artifact's version; keep the stamp sidecar current.
    let apath = repo.join(".repowalk").join("analysis.json");
    let text = fs::read_to_string(&apath).unwrap();
    let tampered = text.replacen("\"version\":1", "\"version\":99", 1);
    assert_ne!(text, tampered, "version field found");
    fs::write(&apath, tampered).unwrap();

    // First read must refuse the bad version (it may error), but it must NOT
    // leave an empty artifact that the read after that serves silently.
    let _ = Command::new(BIN)
        .args(["report", "--json", path])
        .output()
        .expect("spawn");
    let after = run(&["report", "--json", path]);
    assert!(
        json_num(&after, "variables").unwrap() > 0,
        "unknown-version artifact was misread as an empty analysis: {after}"
    );

    let _ = fs::remove_dir_all(&repo);
}
