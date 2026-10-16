//! PROD-5 regression: the R-27 unsupported-language count must reach the user.
//!
//! Runs the real `repowalk-cli` binary on a scratch tree with one unsupported
//! file (`script.rb`) plus TS/JS files and asserts the count is named by both
//! `map` (health line) and `report` (Overview row + additive `--json` field),
//! and that a fully-analyzable tree prints nothing extra.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_repowalk-cli");

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("rw-cli-prod5-{}-{}", std::process::id(), tag));
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

/// Ruby has no analyzer: the file is indexed (shows in the language mix and as
/// `0 symbols`) but the user must be told it was not analyzed.
#[test]
fn unsupported_language_is_named_by_map_and_report() {
    let repo = scratch("mixed");
    fs::write(
        repo.join("script.rb"),
        "def greet(name)\n  \"hi #{name}\"\nend\nputs greet(\"x\")\n",
    )
    .unwrap();
    fs::write(
        repo.join("main.ts"),
        "export function main(): number { return util(); }\n",
    )
    .unwrap();
    fs::write(
        repo.join("util.js"),
        "export function util(): number { return 1; }\n",
    )
    .unwrap();
    let path = repo.to_str().unwrap();

    let map = run(&["map", path]);
    assert!(
        map.contains("1 files not analyzed (rb)"),
        "map must name the skipped language:\n{map}"
    );
    assert!(map.contains("script.rb"), "file still indexed:\n{map}");

    let md = run(&["report", path]);
    assert!(
        md.contains("| not analyzed | rb (1 file) |"),
        "report Overview must carry the row:\n{md}"
    );

    let json = run(&["report", "--json", path]);
    assert!(
        json.contains("\"unsupported\":[{\"language\":\"rb\",\"files\":1}]"),
        "additive --json field missing:\n{json}"
    );
    // Additive only: every pre-existing top-level key is still present.
    for key in [
        "\"repo\":",
        "\"overview\":",
        "\"complexity\":",
        "\"unused_variables\":",
        "\"types\":",
        "\"top_references\":",
        "\"guardrails\":",
    ] {
        assert!(json.contains(key), "lost existing key {key}:\n{json}");
    }

    let _ = fs::remove_dir_all(&repo);
}

/// A repo where every file is analyzable must render exactly as before: no
/// fabricated warning in `map`, `report`, or `report --json`.
#[test]
fn analyzable_repo_prints_nothing_extra() {
    let repo = scratch("clean");
    fs::write(
        repo.join("main.rs"),
        "fn main() { helper(); }\nfn helper() {}\n",
    )
    .unwrap();
    fs::write(repo.join("tool.py"), "def run():\n    return 1\n").unwrap();
    let path = repo.to_str().unwrap();

    let map = run(&["map", path]);
    assert!(
        !map.contains("not analyzed"),
        "no fabricated warning:\n{map}"
    );

    let md = run(&["report", path]);
    assert!(
        !md.contains("not analyzed"),
        "no fabricated row in Markdown:\n{md}"
    );

    let json = run(&["report", "--json", path]);
    assert!(
        !json.contains("\"unsupported\""),
        "no unsupported key when nothing was skipped:\n{json}"
    );

    let _ = fs::remove_dir_all(&repo);
}
