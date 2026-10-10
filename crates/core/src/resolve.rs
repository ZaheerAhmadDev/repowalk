//! Call-site → symbol resolution with explicit confidence tiers (plan §7.1).
//! OWNER: RSK-5.
//!
//! Tier order, exactly as the plan specifies: no candidates → Unresolved,
//! same file → Exact, same module (shared path prefix ≥ 1 dir) → Exact,
//! unique global candidate → NameOnly, several candidates → Ambiguous.
//! Candidate lists and match sets are sorted, so the result never depends
//! on input ordering.
use crate::model::{CallSite, Confidence, Edge, EdgeKind, FileEntry, Import, Symbol};
use std::collections::{BTreeMap, BTreeSet};

/// Resolve call sites to edges: Exact (same file/module), NameOnly (unique global),
/// Ambiguous (multiple candidates), Unresolved (none). Deterministic.
pub fn resolve(files: &[FileEntry], symbols: &[Symbol], sites: &[CallSite]) -> Vec<Edge> {
    let known: BTreeSet<&str> = files.iter().map(|f| f.path.as_str()).collect();
    let mut by_name: BTreeMap<&str, Vec<&Symbol>> = BTreeMap::new();
    let mut by_qualified: BTreeMap<String, Vec<&Symbol>> = BTreeMap::new();
    for sym in symbols {
        // Only resolve into files the index actually knows.
        if !known.contains(sym.file.as_str()) {
            continue;
        }
        by_name.entry(sym.name.as_str()).or_default().push(sym);
        if let Some(container) = &sym.container {
            by_qualified
                .entry(format!("{container}::{}", sym.name))
                .or_default()
                .push(sym);
        }
    }
    for cands in by_name.values_mut().chain(by_qualified.values_mut()) {
        cands.sort_by(|a, b| a.id.cmp(&b.id));
    }

    sites
        .iter()
        .map(|site| {
            let empty = Vec::new();
            let cands = by_qualified
                .get(site.callee.as_str())
                .or_else(|| by_name.get(site.callee.as_str()))
                .unwrap_or(&empty);
            let (callee, confidence) = if cands.is_empty() {
                (site.callee.clone(), Confidence::Unresolved)
            } else if let Some(sym) = cands.iter().find(|s| s.file == site.file) {
                (sym.id.clone(), Confidence::Exact)
            } else if let Some(sym) = cands.iter().find(|s| same_module(&site.file, &s.file)) {
                (sym.id.clone(), Confidence::Exact)
            } else if cands.len() == 1 {
                (cands[0].id.clone(), Confidence::NameOnly)
            } else {
                (cands[0].id.clone(), Confidence::Ambiguous)
            };
            Edge {
                caller: site
                    .enclosing
                    .clone()
                    .unwrap_or_else(|| site.file.clone()),
                callee,
                line: site.line,
                kind: EdgeKind::Call,
                confidence,
            }
        })
        .collect()
}

/// Resolve imports to file-level Import edges where the target is identifiable.
/// Direct path hit → Exact, unique repo-suffix hit → NameOnly, several hits →
/// Ambiguous, none → Unresolved (callee keeps the raw target).
pub fn resolve_imports(files: &[FileEntry], imports: &[Import]) -> Vec<Edge> {
    let known: BTreeSet<&str> = files.iter().map(|f| f.path.as_str()).collect();
    let crate_roots: BTreeSet<String> = files
        .iter()
        .filter(|f| matches!(file_name(&f.path), "main.rs" | "lib.rs"))
        .map(|f| parent_path(&f.path).to_string())
        .collect();

    imports
        .iter()
        .map(|imp| {
            let cands = import_candidates(&imp.target, &imp.file, &crate_roots);
            let (callee, confidence) = match import_tier(&cands, &known) {
                Some((path, tier)) => (path.to_string(), tier),
                None => (imp.target.clone(), Confidence::Unresolved),
            };
            Edge {
                caller: imp.file.clone(),
                callee,
                line: 0,
                kind: EdgeKind::Import,
                confidence,
            }
        })
        .collect()
}

/// True when both paths share at least one leading directory (plan §7.1:
/// "same module (shared path prefix ≥ 1 dir)").
fn same_module(a: &str, b: &str) -> bool {
    let (pa, pb) = (parent_dirs(a), parent_dirs(b));
    pa.iter().zip(pb.iter()).take_while(|(x, y)| x == y).count() >= 1
}

fn parent_dirs(path: &str) -> Vec<&str> {
    match path.rsplit_once('/') {
        Some((parent, _)) => parent.split('/').filter(|c| !c.is_empty()).collect(),
        None => Vec::new(),
    }
}

fn parent_path(path: &str) -> &str {
    path.rsplit_once('/').map(|(parent, _)| parent).unwrap_or("")
}

fn file_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

fn path_join(base: &str, rest: &str) -> String {
    if base.is_empty() {
        rest.to_string()
    } else {
        format!("{base}/{rest}")
    }
}

/// Candidate files for one import target, sorted and deduplicated.
fn import_candidates(target: &str, from_file: &str, crate_roots: &BTreeSet<String>) -> Vec<String> {
    let target = target.trim();
    let from_dir = parent_path(from_file);
    let mut bases: Vec<String> = Vec::new();
    let rest: &str;
    let mut rust = false;
    let mut python = false;

    if target.starts_with('.') {
        // Python relative import: n leading dots → n-1 dirs up from the file.
        let dots = target.chars().take_while(|&c| c == '.').count();
        let mut base = from_dir.to_string();
        for _ in 1..dots {
            base = parent_path(&base).to_string();
        }
        bases.push(base);
        rest = &target[dots..];
        python = true;
    } else if let Some(r) = target.strip_prefix("crate::") {
        rust = true;
        rest = r;
        push_crate_roots(&mut bases, crate_roots);
    } else if let Some(r) = target.strip_prefix("self::") {
        rust = true;
        rest = r;
        bases.push(from_dir.to_string());
    } else if target.starts_with("super::") || target.starts_with("super.") {
        let mut r = target;
        let mut up = 0;
        while let Some(next) = r.strip_prefix("super::").or_else(|| r.strip_prefix("super.")) {
            up += 1;
            r = next;
        }
        let mut base = from_dir.to_string();
        for _ in 0..up {
            base = parent_path(&base).to_string();
        }
        bases.push(base);
        rest = r;
        rust = target.contains("::");
        python = !rust;
    } else if let Some(r) = target.strip_prefix("mod ") {
        // Rust `mod foo;` declaration → sibling module file.
        rust = true;
        rest = r.trim();
        bases.push(from_dir.to_string());
    } else if target.contains("::") {
        rust = true;
        rest = target;
        push_crate_roots(&mut bases, crate_roots);
    } else {
        // Bare / Python absolute path: repo root plus crate roots.
        rust = true;
        python = true;
        rest = target;
        push_crate_roots(&mut bases, crate_roots);
    }

    let segs: Vec<&str> = rest
        .split(|c| c == ':' || c == '.')
        .filter(|s| !s.is_empty())
        .collect();

    let mut out: BTreeSet<String> = BTreeSet::new();
    for base in &bases {
        if segs.is_empty() {
            if rust {
                out.insert(path_join(base, "mod.rs"));
            }
            if python {
                out.insert(path_join(base, "__init__.py"));
            }
            continue;
        }
        let joined = segs.join("/");
        if rust {
            out.insert(path_join(base, &format!("{joined}.rs")));
            out.insert(path_join(base, &format!("{joined}/mod.rs")));
            if segs.len() > 1 {
                // Last segment may be an inline module inside its parent's file.
                out.insert(path_join(base, &format!("{}.rs", segs[..segs.len() - 1].join("/"))));
            }
        }
        if python {
            out.insert(path_join(base, &format!("{joined}.py")));
            out.insert(path_join(base, &format!("{joined}/__init__.py")));
        }
    }
    out.into_iter().collect()
}

fn push_crate_roots(bases: &mut Vec<String>, crate_roots: &BTreeSet<String>) {
    if crate_roots.is_empty() {
        bases.push(String::new());
    } else {
        bases.extend(crate_roots.iter().cloned());
    }
}

/// Classify candidate files against the known file set.
fn import_tier<'a>(cands: &[String], known: &'a BTreeSet<&'a str>) -> Option<(&'a str, Confidence)> {
    let mut exact: Vec<&str> = cands
        .iter()
        .filter_map(|c| known.get(c.as_str()).copied())
        .collect();
    exact.sort_unstable();
    exact.dedup();
    if exact.len() == 1 {
        return Some((exact[0], Confidence::Exact));
    }
    if exact.len() > 1 {
        return Some((exact[0], Confidence::Ambiguous));
    }

    // No direct hit: fall back to a unique path-suffix match anywhere in the repo.
    let mut suffix: Vec<&str> = Vec::new();
    for &path in known.iter() {
        if cands
            .iter()
            .any(|c| path == c.as_str() || path.ends_with(&format!("/{c}")))
        {
            suffix.push(path);
        }
    }
    match suffix.len() {
        0 => None,
        1 => Some((suffix[0], Confidence::NameOnly)),
        _ => Some((suffix[0], Confidence::Ambiguous)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::SymbolKind;

    fn file(path: &str) -> FileEntry {
        FileEntry {
            path: path.into(),
            lang: "rust".into(),
            lines: 50,
            mtime: 0,
            is_entrypoint: false,
        }
    }

    fn entrypoint(path: &str) -> FileEntry {
        FileEntry {
            is_entrypoint: true,
            ..file(path)
        }
    }

    fn sym(file: &str, line: usize, name: &str) -> Symbol {
        Symbol {
            id: format!("{file}:{line}"),
            name: name.into(),
            kind: SymbolKind::Function,
            signature: format!("fn {name}()"),
            doc: String::new(),
            start_line: line,
            end_line: line + 1,
            container: None,
            file: file.into(),
        }
    }

    fn call(file: &str, callee: &str, line: usize, enclosing: Option<&str>) -> CallSite {
        CallSite {
            file: file.into(),
            callee: callee.into(),
            line,
            enclosing: enclosing.map(str::to_string),
        }
    }

    fn imp(file: &str, target: &str) -> Import {
        Import {
            file: file.into(),
            target: target.into(),
        }
    }

    // ---- resolve(): all four confidence tiers ----

    #[test]
    fn tier_exact_same_file() {
        let files = vec![file("src/a.rs")];
        let symbols = vec![sym("src/a.rs", 1, "caller"), sym("src/a.rs", 10, "helper")];
        let sites = vec![call("src/a.rs", "helper", 5, Some("src/a.rs:1"))];

        let edges = resolve(&files, &symbols, &sites);
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].caller, "src/a.rs:1");
        assert_eq!(edges[0].callee, "src/a.rs:10");
        assert_eq!(edges[0].line, 5);
        assert_eq!(edges[0].kind, EdgeKind::Call);
        assert_eq!(edges[0].confidence, Confidence::Exact);
    }

    #[test]
    fn tier_exact_same_module_shared_dir_prefix() {
        // Callers in src/app, callee in src/lib: shared prefix "src" ≥ 1 dir.
        let files = vec![file("src/app/main.rs"), file("src/lib/util.rs")];
        let symbols = vec![sym("src/lib/util.rs", 7, "util_fn")];
        let sites = vec![call("src/app/main.rs", "util_fn", 3, Some("src/app/main.rs:1"))];

        let edges = resolve(&files, &symbols, &sites);
        assert_eq!(edges[0].callee, "src/lib/util.rs:7");
        assert_eq!(edges[0].confidence, Confidence::Exact);
    }

    #[test]
    fn tier_name_only_unique_global() {
        // lib/ vs helpers/ share no directory → not same module, but unique.
        let files = vec![file("lib/main.rs"), file("helpers/util.rs")];
        let symbols = vec![sym("helpers/util.rs", 4, "util_fn")];
        let sites = vec![call("lib/main.rs", "util_fn", 9, Some("lib/main.rs:1"))];

        let edges = resolve(&files, &symbols, &sites);
        assert_eq!(edges[0].callee, "helpers/util.rs:4");
        assert_eq!(edges[0].confidence, Confidence::NameOnly);
    }

    #[test]
    fn tier_ambiguous_multiple_candidates() {
        let files = vec![
            file("lib/main.rs"),
            file("helpers/a.rs"),
            file("helpers/b.rs"),
        ];
        let symbols = vec![
            sym("helpers/b.rs", 1, "util_fn"),
            sym("helpers/a.rs", 3, "util_fn"),
        ];
        let sites = vec![call("lib/main.rs", "util_fn", 9, None)];

        let edges = resolve(&files, &symbols, &sites);
        assert_eq!(edges[0].confidence, Confidence::Ambiguous);
        // Deterministic pick: lowest symbol id, independent of input order.
        assert_eq!(edges[0].callee, "helpers/a.rs:3");
        // Caller falls back to the file when no enclosing symbol is known.
        assert_eq!(edges[0].caller, "lib/main.rs");

        let reversed: Vec<Symbol> = symbols.iter().rev().cloned().collect();
        let again = resolve(&files, &reversed, &sites);
        assert_eq!(again[0].callee, edges[0].callee);
        assert_eq!(again[0].confidence, Confidence::Ambiguous);
    }

    #[test]
    fn tier_unresolved_no_candidates() {
        let files = vec![file("src/main.rs")];
        let symbols = vec![sym("src/main.rs", 1, "main")];
        let sites = vec![call("src/main.rs", "nowhere", 12, Some("src/main.rs:1"))];

        let edges = resolve(&files, &symbols, &sites);
        assert_eq!(edges[0].callee, "nowhere");
        assert_eq!(edges[0].confidence, Confidence::Unresolved);
    }

    #[test]
    fn qualified_map_beats_ambiguous_plain_name() {
        // Two `render` symbols: only the Widget::render one matches the callee.
        let mut widget = sym("ui/button.rs", 5, "render");
        widget.container = Some("Widget".into());
        let mut panel = sym("ui/panel.rs", 5, "render");
        panel.container = Some("Panel".into());
        let files = vec![file("app/main.rs"), file("ui/button.rs"), file("ui/panel.rs")];
        let symbols = vec![panel, widget];
        let sites = vec![call("app/main.rs", "Widget::render", 2, None)];

        let edges = resolve(&files, &symbols, &sites);
        assert_eq!(edges[0].callee, "ui/button.rs:5");
        assert_eq!(edges[0].confidence, Confidence::NameOnly);
    }

    #[test]
    fn symbols_outside_the_index_are_ignored() {
        let files = vec![file("src/main.rs")];
        let symbols = vec![sym("ghost/mod.rs", 1, "phantom")];
        let sites = vec![call("src/main.rs", "phantom", 4, None)];

        let edges = resolve(&files, &symbols, &sites);
        assert_eq!(edges[0].confidence, Confidence::Unresolved);
    }

    // ---- resolve_imports(): file-level resolution ----

    #[test]
    fn import_exact_crate_path() {
        let files = vec![entrypoint("src/main.rs"), file("src/model.rs")];
        let imports = vec![imp("src/main.rs", "crate::model")];

        let edges = resolve_imports(&files, &imports);
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].caller, "src/main.rs");
        assert_eq!(edges[0].callee, "src/model.rs");
        assert_eq!(edges[0].line, 0);
        assert_eq!(edges[0].kind, EdgeKind::Import);
        assert_eq!(edges[0].confidence, Confidence::Exact);
    }

    #[test]
    fn import_exact_python_relative() {
        let files = vec![file("pkg/mod.py"), file("pkg/dep.py")];
        let imports = vec![imp("pkg/mod.py", ".dep")];

        let edges = resolve_imports(&files, &imports);
        assert_eq!(edges[0].callee, "pkg/dep.py");
        assert_eq!(edges[0].confidence, Confidence::Exact);
    }

    #[test]
    fn import_name_only_unique_suffix() {
        let files = vec![file("libs/util.py")];
        let imports = vec![imp("app/main.py", "util")];

        let edges = resolve_imports(&files, &imports);
        assert_eq!(edges[0].callee, "libs/util.py");
        assert_eq!(edges[0].confidence, Confidence::NameOnly);
    }

    #[test]
    fn import_ambiguous_across_crate_roots() {
        let files = vec![
            entrypoint("bin/a/main.rs"),
            entrypoint("bin/b/main.rs"),
            file("bin/a/shared.rs"),
            file("bin/b/shared.rs"),
        ];
        let imports = vec![imp("bin/a/main.rs", "crate::shared")];

        let edges = resolve_imports(&files, &imports);
        assert_eq!(edges[0].confidence, Confidence::Ambiguous);
        assert_eq!(edges[0].callee, "bin/a/shared.rs");
    }

    #[test]
    fn import_unresolved_keeps_raw_target() {
        let files = vec![entrypoint("src/main.rs")];
        let imports = vec![imp("src/main.rs", "crate::missing")];

        let edges = resolve_imports(&files, &imports);
        assert_eq!(edges[0].callee, "crate::missing");
        assert_eq!(edges[0].confidence, Confidence::Unresolved);
    }

    #[test]
    fn imports_are_deterministic() {
        let files = vec![
            entrypoint("bin/a/main.rs"),
            entrypoint("bin/b/main.rs"),
            file("bin/a/shared.rs"),
            file("bin/b/shared.rs"),
        ];
        let imports = vec![imp("bin/a/main.rs", "crate::shared")];
        let first = resolve_imports(&files, &imports);
        let second = resolve_imports(&files, &imports);
        assert_eq!(first[0].callee, second[0].callee);
        assert_eq!(first[0].confidence, second[0].confidence);
    }
}
