//! Architecture guardrails (F15).
//!
//! OWNER: CORE-07 (card CORE-6). Reads a small, hand-written rule config and
//! turns it into deterministic [`Finding`]s that health-v2, the report, and the
//! CLI render (the CLI exits non-zero on any `Error`-severity finding).
//!
//! ## Where rules come from
//!
//! [`load`] reads `<root>/.repowalk/rules.json` first; when that file does not
//! exist it falls back to `<root>/rules.json` (the committed sample lives at the
//! repo root because `.repowalk/` is generated and git-ignored). A missing,
//! unreadable, or corrupt file yields [`Rules::default`] — an empty rule set —
//! and **never** panics.
//!
//! ## The rules
//!
//! - `forbid_imports: [[from, to], …]` — flag every import edge whose *from*
//!   side matches the `from` glob and whose *to* side matches the `to` glob
//!   (severity `Error`).
//! - `max_cyclomatic: N` — flag every analyzed function whose cyclomatic
//!   complexity is greater than `N` (severity `Warning`).
//! - `max_file_loc: N` — flag every indexed file with more than `N` lines
//!   (severity `Warning`).
//! - `no_cycles: [glob, …]` — for each glob, consider the module-graph nodes it
//!   selects and flag every import cycle among them (severity `Error`). Cycles
//!   are found with [`crate::modules::cycles`].
//!
//! ## Glob matching (documented contract)
//!
//! Rule patterns are matched against *labels* (see [`ImportLabels`]) with a
//! tiny, dependency-free matcher, [`glob_match`]:
//!
//! - `?` matches exactly one character other than `/`;
//! - `*` matches a run (possibly empty) of characters other than `/`, i.e. one
//!   path segment — `crates/*/src` matches `crates/core/src` but not
//!   `crates/a/b/src`;
//! - `**` matches any run of characters *including* `/`, i.e. any number of
//!   segments — `crates/**/src` matches both;
//! - every other character is a literal, case-sensitive match.
//!
//! A label is a candidate string for one side of an import. For the importing
//! file we offer its repo-relative path, every ancestor directory, the crate
//! directory name (`crates/core/…` → `core`), and the repo directory name. For
//! the imported target we offer the raw target, the target with a trailing
//! `::{…}`/`.{…}` group removed, a separator-normalised form (`crate::a::b` and
//! `.a.b` → `a/b`), the first segment of that form, a crate-directory alias for
//! that segment (`repowalk_gui` → `gui`), and — when the target resolves to a
//! known file — the same path/ancestor/crate labels as the importing side. A
//! `forbid_imports` pair matches when *any* from-label matches the `from` glob
//! **and** any to-label matches the `to` glob.
//!
//! Everything is deterministic: the same `(index, analysis, rules)` always
//! yields the same findings, ordered by severity, file, line, then rule.
use crate::analysis::Analysis;
use crate::model::{Confidence, Index};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// A rule config, tolerant of missing fields (every field is `#[serde(default)]`).
///
/// JSON shape:
/// ```json
/// {
///   "forbid_imports": [["core", "gui"]],
///   "max_cyclomatic": 40,
///   "max_file_loc": 5000,
///   "no_cycles": ["crates/core/src/**"]
/// }
/// ```
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rules {
    /// `(from_glob, to_glob)` import pairs that must never appear.
    #[serde(default)]
    pub forbid_imports: Vec<(String, String)>,
    /// Maximum cyclomatic complexity per function (`None` disables the check).
    #[serde(default)]
    pub max_cyclomatic: Option<u32>,
    /// Maximum lines per file (`None` disables the check).
    #[serde(default)]
    pub max_file_loc: Option<usize>,
    /// Module sets (globs) that must be free of import cycles.
    #[serde(default)]
    pub no_cycles: Vec<String>,
}

/// How serious a [`Finding`] is. `Error` sorts before `Warning`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Severity {
    /// A hard rule violation — the CLI exits non-zero.
    Error,
    /// A soft threshold breach — reported, but not fatal.
    Warning,
}

/// One rule violation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Finding {
    /// Stable rule id, e.g. `max_cyclomatic` or `forbid_imports[0] core -> gui`.
    pub rule: String,
    pub severity: Severity,
    /// Repo-relative file path the finding is attached to.
    pub file: String,
    /// 1-based line, or `0` when the rule has no line (imports, cycles, file LOC).
    pub line: usize,
    /// Human-readable explanation.
    pub message: String,
}

/// `<root>/.repowalk/rules.json` — the preferred location.
pub fn rules_path(root: &Path) -> PathBuf {
    root.join(".repowalk").join("rules.json")
}

/// Load the rule config for `root`.
///
/// Prefers `.repowalk/rules.json`, then `<root>/rules.json`. A missing,
/// unreadable, or corrupt file yields [`Rules::default`] (empty rules); this
/// function never panics.
pub fn load(root: &Path) -> Rules {
    let primary = rules_path(root);
    if primary.is_file() {
        return read(&primary).unwrap_or_default();
    }
    let fallback = root.join("rules.json");
    if fallback.is_file() {
        return read(&fallback).unwrap_or_default();
    }
    Rules::default()
}

/// Read + parse one config file; `None` on any I/O or JSON error.
fn read(path: &Path) -> Option<Rules> {
    let bytes = std::fs::read(path).ok()?;
    serde_json::from_slice::<Rules>(&bytes).ok()
}

/// Run every configured rule over `index` + `analysis`.
///
/// Deterministic: the result is ordered by `(severity, file, line, rule)`.
/// The module graph for the import/cycle rules is rebuilt from `index` via
/// [`crate::modules::build`] so the function is a pure read of its inputs.
pub fn check(index: &Index, analysis: &Analysis, rules: &Rules) -> Vec<Finding> {
    let (nodes, edges) = crate::modules::build(index);
    let crate_ids: BTreeSet<String> = nodes
        .iter()
        .filter(|n| n.kind == crate::analysis::ModuleKind::Crate)
        .map(|n| n.id.clone())
        .collect();
    let root_name = Path::new(&index.repo_root)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_string();

    let mut out = Vec::new();
    check_forbid_imports(index, rules, &crate_ids, &root_name, &mut out);
    check_max_cyclomatic(analysis, rules, &mut out);
    check_max_file_loc(index, rules, &mut out);
    check_no_cycles(rules, &nodes, &edges, &mut out);

    out.sort_by(|a, b| {
        a.severity
            .cmp(&b.severity)
            .then_with(|| a.file.cmp(&b.file))
            .then_with(|| a.line.cmp(&b.line))
            .then_with(|| a.rule.cmp(&b.rule))
            .then_with(|| a.message.cmp(&b.message))
    });
    out
}

/// `forbid_imports`: match forbidden import edges against their label sets.
fn check_forbid_imports(
    index: &Index,
    rules: &Rules,
    crate_ids: &BTreeSet<String>,
    root_name: &str,
    out: &mut Vec<Finding>,
) {
    if rules.forbid_imports.is_empty() {
        return;
    }
    // `resolve_imports` maps 1:1 over `index.imports` in order (see resolve.rs);
    // keep the same guard so a future change cannot silently desync the zip.
    let resolved = crate::resolve::resolve_imports(&index.files, &index.imports);
    for (i, imp) in index.imports.iter().enumerate() {
        let mut from_labels = BTreeSet::new();
        location_labels(&imp.file, root_name, &mut from_labels);
        let mut to_labels = BTreeSet::new();
        target_labels(&imp.target, crate_ids, &mut to_labels);
        // When the raw target resolves to a real file, its path/ancestors/crate
        // are matchable labels too (`repowalk_gui::x` → `gui`).
        if let Some(edge) = resolved.get(i) {
            if edge.confidence != Confidence::Unresolved && edge.callee != imp.target {
                location_labels(&edge.callee, root_name, &mut to_labels);
            }
        }
        for (n, (from_glob, to_glob)) in rules.forbid_imports.iter().enumerate() {
            let f = from_labels.iter().any(|l| glob_match(from_glob, l));
            let t = to_labels.iter().any(|l| glob_match(to_glob, l));
            if f && t {
                out.push(Finding {
                    rule: format!("forbid_imports[{n}] {from_glob} -> {to_glob}"),
                    severity: Severity::Error,
                    file: imp.file.clone(),
                    line: 0,
                    message: format!(
                        "forbidden import '{}' in {} (rule '{}' -> '{}')",
                        imp.target, imp.file, from_glob, to_glob
                    ),
                });
            }
        }
    }
}

/// `max_cyclomatic`: flag over-complex functions.
fn check_max_cyclomatic(analysis: &Analysis, rules: &Rules, out: &mut Vec<Finding>) {
    let Some(max) = rules.max_cyclomatic else {
        return;
    };
    for cfg in &analysis.cfgs {
        if cfg.cyclomatic > max {
            let (file, line) = crate::complexity::parse_symbol_id(&cfg.symbol);
            out.push(Finding {
                rule: "max_cyclomatic".to_string(),
                severity: Severity::Warning,
                file,
                line,
                message: format!(
                    "cyclomatic complexity {} exceeds max {} at {}",
                    cfg.cyclomatic, max, cfg.symbol
                ),
            });
        }
    }
}

/// `max_file_loc`: flag over-long files.
fn check_max_file_loc(index: &Index, rules: &Rules, out: &mut Vec<Finding>) {
    let Some(max) = rules.max_file_loc else {
        return;
    };
    for f in &index.files {
        if f.lines > max {
            out.push(Finding {
                rule: "max_file_loc".to_string(),
                severity: Severity::Warning,
                file: f.path.clone(),
                line: 0,
                message: format!("file has {} LOC, exceeds max {}", f.lines, max),
            });
        }
    }
}

/// `no_cycles`: flag import cycles inside each selected module set.
fn check_no_cycles(
    rules: &Rules,
    nodes: &[crate::analysis::ModuleNode],
    edges: &[crate::analysis::ModuleEdge],
    out: &mut Vec<Finding>,
) {
    for pattern in &rules.no_cycles {
        let selected: BTreeSet<&str> = nodes
            .iter()
            .filter(|n| glob_match(pattern, &n.id))
            .map(|n| n.id.as_str())
            .collect();
        if selected.len() < 2 {
            continue;
        }
        let nodes_sel: Vec<crate::analysis::ModuleNode> = nodes
            .iter()
            .filter(|n| selected.contains(n.id.as_str()))
            .cloned()
            .collect();
        let edges_sel: Vec<crate::analysis::ModuleEdge> = edges
            .iter()
            .filter(|e| selected.contains(e.from.as_str()) && selected.contains(e.to.as_str()))
            .cloned()
            .collect();
        for cyc in crate::modules::cycles(&nodes_sel, &edges_sel) {
            out.push(Finding {
                rule: format!("no_cycles {pattern}"),
                severity: Severity::Error,
                file: cyc.first().cloned().unwrap_or_default(),
                line: 0,
                message: format!(
                    "import cycle in module set '{}': {}",
                    pattern,
                    cyc.join(" -> ")
                ),
            });
        }
    }
}

/// Add the label set for a location (file path or module node id).
fn location_labels(path: &str, root_name: &str, out: &mut BTreeSet<String>) {
    if path.is_empty() {
        return;
    }
    out.insert(path.to_string());
    let mut cur = path;
    while let Some((parent, _)) = cur.rsplit_once('/') {
        if parent.is_empty() {
            break;
        }
        out.insert(parent.to_string());
        cur = parent;
    }
    if let Some(rest) = path.strip_prefix("crates/") {
        if let Some(id) = rest.split('/').next() {
            if !id.is_empty() {
                out.insert(id.to_string());
            }
        }
    }
    if !root_name.is_empty() {
        out.insert(root_name.to_string());
    }
}

/// Add the label set for an import target string.
fn target_labels(target: &str, crate_ids: &BTreeSet<String>, out: &mut BTreeSet<String>) {
    let raw = target.trim();
    if raw.is_empty() {
        return;
    }
    out.insert(raw.to_string());

    // `crate::x::{a, b}` → `crate::x`
    if let Some(head) = head_before_group(raw) {
        out.insert(head.to_string());
    }

    // Normalise separators / leading qualifiers into a slash path.
    let mut norm = raw.trim_matches(|c| c == '"' || c == '\'');
    loop {
        let next = norm
            .strip_prefix("crate::")
            .or_else(|| norm.strip_prefix("self::"))
            .or_else(|| norm.strip_prefix("super::"))
            .or_else(|| norm.strip_prefix("crate."))
            .or_else(|| norm.strip_prefix("self."))
            .or_else(|| norm.strip_prefix("super."));
        match next {
            Some(n) => norm = n.trim_start_matches('.'),
            None => break,
        }
    }
    if let Some(head) = head_before_group(norm) {
        norm = head;
    }
    let slash = norm.replace("::", "/").replace('.', "/");
    let slash = slash.trim_matches('/');
    if slash.is_empty() {
        return;
    }
    out.insert(slash.to_string());
    if let Some(first) = slash.split('/').next() {
        if !first.is_empty() {
            out.insert(first.to_string());
            if let Some(alias) = crate_alias(first, crate_ids) {
                out.insert(alias);
            }
        }
    }
}

/// The part of `s` before a `{…}` group, with trailing `::`/`.`/spaces trimmed.
fn head_before_group(s: &str) -> Option<&str> {
    let idx = s.find('{')?;
    let head = s[..idx].trim_end_matches([':', '.', ' ']);
    (!head.is_empty()).then_some(head)
}

/// Map a target segment onto a known crate id: `repowalk_gui` → `gui`.
fn crate_alias(seg: &str, crate_ids: &BTreeSet<String>) -> Option<String> {
    crate_ids.iter().find_map(|id| {
        (seg == id || seg.ends_with(&format!("_{id}")) || seg.ends_with(&format!("-{id}")))
            .then(|| id.clone())
    })
}

/// Glob match: `?` = one non-`/` char, `*` = one segment, `**` = any (incl. `/`).
///
/// See the module docs for the full matching contract. Pure, deterministic, and
/// iterative (no recursion, no allocation beyond the pattern/text buffers).
pub fn glob_match(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let t: Vec<char> = text.chars().collect();
    let n = t.len();
    // dp[j] = "pattern consumed so far matches text[..j]".
    let mut dp = vec![false; n + 1];
    dp[0] = true;
    let mut i = 0;
    while i < p.len() {
        let mut ndp = vec![false; n + 1];
        match p[i] {
            '*' => {
                let double = i + 1 < p.len() && p[i + 1] == '*';
                ndp[0] = dp[0];
                for j in 1..=n {
                    ndp[j] = if double {
                        dp[j] || ndp[j - 1]
                    } else {
                        dp[j] || (ndp[j - 1] && t[j - 1] != '/')
                    };
                }
                i += if double { 2 } else { 1 };
                dp = ndp;
                continue;
            }
            '?' => {
                for j in 1..=n {
                    ndp[j] = dp[j - 1] && t[j - 1] != '/';
                }
            }
            c => {
                for j in 1..=n {
                    ndp[j] = dp[j - 1] && t[j - 1] == c;
                }
            }
        }
        dp = ndp;
        i += 1;
    }
    dp[n]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::{Analysis, FunctionCfg};
    use crate::model::{FileEntry, Import, Index};

    fn file(path: &str, lines: usize) -> FileEntry {
        FileEntry {
            path: path.into(),
            lang: "rust".into(),
            lines,
            mtime: 0,
            is_entrypoint: false,
        }
    }

    fn imp(file: &str, target: &str) -> Import {
        Import {
            file: file.into(),
            target: target.into(),
        }
    }

    fn cfg(symbol: &str, cyclomatic: u32) -> FunctionCfg {
        FunctionCfg {
            symbol: symbol.into(),
            nodes: Vec::new(),
            edges: Vec::new(),
            cyclomatic,
            max_nesting: 0,
        }
    }

    /// Create a private temp dir for config-file tests.
    fn tmpdir(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("repowalk_rules_{}_{}", std::process::id(), tag));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("mkdir");
        dir
    }

    // ---- glob matching ----

    #[test]
    fn glob_semantics_single_double_and_literal() {
        assert!(glob_match("core", "core"));
        assert!(!glob_match("core", "crate"));
        assert!(glob_match("crates/*/src", "crates/core/src"));
        assert!(!glob_match("crates/*/src", "crates/a/b/src"));
        assert!(glob_match("crates/**/src", "crates/a/b/src"));
        assert!(glob_match("**/gui/**", "crates/gui/src/app.rs"));
        assert!(glob_match("crates/core/**", "crates/core/src/learn.rs"));
        assert!(glob_match("c*e", "core"));
        assert!(!glob_match("c*x", "crate"));
        assert!(glob_match("?ore", "core"));
        assert!(!glob_match("?ore", "c/re"));
        assert!(glob_match("*", "segment"));
        assert!(!glob_match("*", "a/b"));
        assert!(glob_match("", ""));
        assert!(!glob_match("", "x"));
    }

    // ---- one finding per rule kind, on a synthetic violating graph ----

    #[test]
    fn synthetic_violating_graph_yields_one_finding_per_rule() {
        let mut idx = Index::new("/repo");
        idx.files = vec![
            file("crates/core/src/lib.rs", 10),
            file("crates/gui/src/app.rs", 5),
            file("crates/core/src/huge.rs", 500),
        ];
        // core -> gui forbidden import (raw crate target).
        idx.imports = vec![imp("crates/core/src/lib.rs", "repowalk_gui::app")];

        // A cycle inside crates/core/src: a <-> b.
        idx.files.push(file("crates/core/src/a.rs", 3));
        idx.files.push(file("crates/core/src/b.rs", 3));
        idx.imports.push(imp("crates/core/src/a.rs", "crate::b"));
        idx.imports.push(imp("crates/core/src/b.rs", "crate::a"));

        let mut analysis = Analysis::new();
        analysis.cfgs = vec![cfg("crates/core/src/huge.rs:1", 42)];

        let rules = Rules {
            forbid_imports: vec![("core".into(), "gui".into())],
            max_cyclomatic: Some(10),
            max_file_loc: Some(100),
            no_cycles: vec!["crates/core/src/**".into()],
        };

        let findings = check(&idx, &analysis, &rules);

        // Exactly one finding of each kind.
        assert_eq!(
            findings
                .iter()
                .filter(|f| f.rule.starts_with("forbid_imports"))
                .count(),
            1
        );
        assert_eq!(
            findings
                .iter()
                .filter(|f| f.rule == "max_cyclomatic")
                .count(),
            1
        );
        assert_eq!(
            findings.iter().filter(|f| f.rule == "max_file_loc").count(),
            1
        );
        assert_eq!(
            findings
                .iter()
                .filter(|f| f.rule.starts_with("no_cycles"))
                .count(),
            1
        );
        assert_eq!(findings.len(), 4);

        let imp = findings
            .iter()
            .find(|f| f.rule.starts_with("forbid_imports"))
            .unwrap();
        assert_eq!(imp.severity, Severity::Error);
        assert_eq!(imp.file, "crates/core/src/lib.rs");
        assert!(imp.message.contains("repowalk_gui::app"));

        let cyc = findings
            .iter()
            .find(|f| f.rule.starts_with("no_cycles"))
            .unwrap();
        assert_eq!(cyc.severity, Severity::Error);
        assert!(cyc.message.contains("crates/core/src/a.rs"));
        assert!(cyc.message.contains("crates/core/src/b.rs"));

        let loc = findings.iter().find(|f| f.rule == "max_file_loc").unwrap();
        assert_eq!(loc.severity, Severity::Warning);
        assert_eq!(loc.file, "crates/core/src/huge.rs");

        let cc = findings
            .iter()
            .find(|f| f.rule == "max_cyclomatic")
            .unwrap();
        assert_eq!(cc.severity, Severity::Warning);
        assert_eq!(cc.file, "crates/core/src/huge.rs");
        assert_eq!(cc.line, 1);

        // Errors sort before warnings.
        assert_eq!(findings[0].severity, Severity::Error);
        assert_eq!(findings[3].severity, Severity::Warning);
    }

    #[test]
    fn clean_graph_yields_no_findings() {
        let mut idx = Index::new("/repo");
        idx.files = vec![
            file("crates/core/src/lib.rs", 20),
            file("crates/gui/src/app.rs", 20),
        ];
        // gui depends on core — allowed; nothing forbidden, no cycle.
        idx.imports = vec![imp("crates/gui/src/app.rs", "repowalk_core::learn")];

        let mut analysis = Analysis::new();
        analysis.cfgs = vec![cfg("crates/gui/src/app.rs:1", 3)];

        let rules = Rules {
            forbid_imports: vec![("core".into(), "gui".into()), ("gui".into(), "cli".into())],
            max_cyclomatic: Some(10),
            max_file_loc: Some(100),
            no_cycles: vec!["crates/**".into()],
        };
        assert!(check(&idx, &analysis, &rules).is_empty());
    }

    #[test]
    fn findings_are_deterministic_regardless_of_input_order() {
        let mut a = Index::new("/repo");
        a.files = vec![
            file("crates/core/src/lib.rs", 10),
            file("crates/core/src/z.rs", 500),
            file("crates/core/src/a.rs", 500),
        ];
        a.imports = vec![
            imp("crates/core/src/z.rs", "repowalk_gui::a"),
            imp("crates/core/src/a.rs", "repowalk_gui::b"),
        ];
        let analysis = Analysis::new();
        let rules = Rules {
            forbid_imports: vec![("core".into(), "gui".into())],
            max_file_loc: Some(100),
            ..Rules::default()
        };
        let first = check(&a, &analysis, &rules);

        // Shuffle the inputs; the output must be identical.
        let mut b = a.clone();
        b.files.reverse();
        b.imports.reverse();
        let second = check(&b, &analysis, &rules);
        assert_eq!(first, second);
        assert!(!first.is_empty());

        // Output is sorted by (severity, file, line, rule).
        let mut sorted = first.clone();
        sorted.sort_by(|x, y| {
            x.severity
                .cmp(&y.severity)
                .then_with(|| x.file.cmp(&y.file))
                .then_with(|| x.line.cmp(&y.line))
                .then_with(|| x.rule.cmp(&y.rule))
                .then_with(|| x.message.cmp(&y.message))
        });
        assert_eq!(first, sorted);
    }

    // ---- config loading ----

    #[test]
    fn missing_rules_file_yields_empty_rules() {
        let dir = tmpdir("missing");
        assert_eq!(load(&dir), Rules::default());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn corrupt_rules_file_yields_empty_rules_and_does_not_panic() {
        let dir = tmpdir("corrupt");
        std::fs::write(dir.join("rules.json"), b"{ this is not json").unwrap();
        assert_eq!(load(&dir), Rules::default());

        // A corrupt .repowalk/rules.json must not fall back to the root sample.
        std::fs::create_dir_all(dir.join(".repowalk")).unwrap();
        std::fs::write(dir.join(".repowalk").join("rules.json"), b"nope").unwrap();
        std::fs::write(dir.join("rules.json"), b"{\"max_file_loc\": 1}").unwrap();
        assert_eq!(load(&dir), Rules::default());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn loads_preferred_then_root_fallback() {
        let dir = tmpdir("load");
        let body = br#"{"forbid_imports":[["core","gui"]],"max_file_loc":7}"#;
        // Fallback to the repo-root sample.
        std::fs::write(dir.join("rules.json"), body).unwrap();
        let r = load(&dir);
        assert_eq!(r.forbid_imports, vec![("core".into(), "gui".into())]);
        assert_eq!(r.max_file_loc, Some(7));
        assert_eq!(r.max_cyclomatic, None);
        assert!(r.no_cycles.is_empty());

        // `.repowalk/rules.json` wins when present.
        std::fs::create_dir_all(dir.join(".repowalk")).unwrap();
        std::fs::write(
            dir.join(".repowalk").join("rules.json"),
            br#"{"max_cyclomatic":3}"#,
        )
        .unwrap();
        assert_eq!(load(&dir).max_cyclomatic, Some(3));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn no_cycles_only_flags_selected_module_set() {
        let mut idx = Index::new("/repo");
        idx.files = vec![
            file("crates/core/src/a.rs", 3),
            file("crates/core/src/b.rs", 3),
        ];
        idx.imports = vec![
            imp("crates/core/src/a.rs", "crate::b"),
            imp("crates/core/src/b.rs", "crate::a"),
        ];
        let analysis = Analysis::new();

        // A glob that selects nothing -> no finding.
        let none = Rules {
            no_cycles: vec!["crates/gui/**".into()],
            ..Rules::default()
        };
        assert!(check(&idx, &analysis, &none).is_empty());

        // The right glob -> one finding.
        let some = Rules {
            no_cycles: vec!["crates/core/**".into()],
            ..Rules::default()
        };
        let f = check(&idx, &analysis, &some);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].severity, Severity::Error);
    }

    #[test]
    fn crate_target_alias_and_resolved_path_labels_both_match() {
        // A raw `repowalk_gui` target and a `crate::` target that resolves into
        // the gui crate must both trip a `core -> gui` rule.
        let mut idx = Index::new("/repo");
        idx.files = vec![
            file("crates/core/src/lib.rs", 10),
            file("crates/gui/src/main.rs", 10),
            file("crates/gui/src/app.rs", 10),
        ];
        idx.imports = vec![imp("crates/core/src/lib.rs", "crate::app")];
        let analysis = Analysis::new();
        let rules = Rules {
            forbid_imports: vec![("core".into(), "gui".into())],
            ..Rules::default()
        };
        let f = check(&idx, &analysis, &rules);
        // `crate::app` resolves to the unique suffix `.../app.rs` in gui.
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].file, "crates/core/src/lib.rs");
    }

    #[test]
    fn empty_rules_find_nothing() {
        let mut idx = Index::new("/repo");
        idx.files = vec![file("crates/core/src/lib.rs", 9999)];
        let analysis = Analysis::new();
        assert!(check(&idx, &analysis, &Rules::default()).is_empty());
    }

    // ---- self-rule over this repository (heavy; run with --ignored) ----

    /// Real run over this repo: cold-learn it, load the committed sample
    /// `rules.json`, and assert the guardrails are green. Ignored by default
    /// because a cold learn parses the whole tree; run with:
    /// `cargo test -p repowalk-core rules::tests::self_rule -- --ignored --nocapture`
    #[test]
    #[ignore = "cold-learns the whole repo; run explicitly for the self-rule evidence"]
    fn self_rule_over_this_repo_is_green() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..");
        let rules = load(&root);
        assert!(
            rules
                .forbid_imports
                .iter()
                .any(|(a, b)| a == "core" && b == "gui"),
            "committed rules.json must contain the core -> gui self-rule"
        );

        let (index, analysis) = crate::learn::learn(&root, &|_p| {}).expect("learn this repo");
        let findings = check(&index, &analysis, &rules);
        println!(
            "self-rule check: {} files, {} cfgs, {} findings",
            index.files.len(),
            analysis.cfgs.len(),
            findings.len()
        );
        for f in &findings {
            println!(
                "  [{:?}] {} {}:{} {}",
                f.severity, f.rule, f.file, f.line, f.message
            );
        }
        assert!(
            findings.is_empty(),
            "sample rules.json must pass on this repo: {findings:#?}"
        );
    }
}
