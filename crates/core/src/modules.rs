//! Crate/dir/module structure graph (F2d).
//!
//! OWNER: CORE-04 (card CORE-4). Builds the ModuleNode/ModuleEdge set the
//! Modules diagram and the report render from: a deterministic tree of
//! crate → dir → module → file nodes, plus Import/Submodule edges aggregated
//! with counts.
//!
//! ## Node ids
//!
//! `Crate` nodes use the crate **name** (`crates/<name>/…` → `<name>`; a plain
//! repo has one root crate named after its directory). Every other node uses
//! its repo-relative path. A directory becomes a `Module` when it directly
//! contains a module-root file (`mod.rs`, `lib.rs`, `main.rs`, `__init__.py`,
//! `index.ts`, …), otherwise a `Dir`; files are `File`.
//!
//! ## Counts
//!
//! `File` nodes carry the file's line count and the number of symbols the index
//! attributed to it. `Crate`/`Dir`/`Module` nodes roll those totals up over all
//! of their descendants, so the diagram can size nodes by subtree weight.
//!
//! ## Edges
//!
//! `Submodule` edges mirror the parent → child tree (one per relationship,
//! `count = 1`). `Import` edges reuse [`crate::resolve::resolve_imports`], so a
//! target becomes a `File` node when it resolves to a known file and the finest
//! containing `Dir`/`Module`/`Crate` node otherwise; each edge's `count` is the
//! number of underlying imports it aggregates. Unresolvable imports are dropped
//! (the frozen `ModuleEdge` has no confidence field to carry them).
//!
//! Everything here is deterministic: same index → byte-identical output, even
//! for an empty index, a file with no directories, self-imports, or cycles.
use crate::analysis::{ModuleEdge, ModuleEdgeKind, ModuleKind, ModuleNode};
use crate::model::Index;
use petgraph::algo::tarjan_scc;
use petgraph::graph::DiGraph;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// Basenames that mark their directory as a module root.
const MODULE_ROOT_FILES: &[&str] = &[
    "mod.rs",
    "lib.rs",
    "main.rs",
    "__init__.py",
    "__init__.pyi",
    "index.ts",
    "index.tsx",
    "index.js",
    "index.jsx",
    "index.mjs",
    "index.cjs",
    "mod.ts",
];

/// Build `(nodes, edges)` for the whole repo.
///
/// Nodes are deterministic (same repo → same ids/order). `Import` edges come
/// from `Index::imports`, `Submodule` edges from the file/dir tree.
pub fn build(index: &Index) -> (Vec<ModuleNode>, Vec<ModuleEdge>) {
    // Deterministic, de-duplicated file order (input order must not matter).
    let mut paths: Vec<&str> = index.files.iter().map(|f| f.path.as_str()).collect();
    paths.sort_unstable();
    paths.dedup();

    // Direct children of each directory, used to classify Dir vs Module.
    let mut dir_files: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for p in &paths {
        if let Some((dir, base)) = p.rsplit_once('/') {
            dir_files.entry(dir).or_default().insert(base);
        }
    }
    let is_module_dir = |dir: &str| {
        dir_files
            .get(dir)
            .is_some_and(|bases| bases.iter().any(|b| MODULE_ROOT_FILES.contains(b)))
    };

    // Symbols per file and lines per file (first wins on a duplicate path).
    let mut symbols_of: BTreeMap<&str, usize> = BTreeMap::new();
    for s in &index.symbols {
        *symbols_of.entry(s.file.as_str()).or_insert(0) += 1;
    }
    let mut lines_of: BTreeMap<&str, usize> = BTreeMap::new();
    for f in &index.files {
        lines_of.entry(f.path.as_str()).or_insert(f.lines);
    }

    let mut nodes: BTreeMap<String, ModuleNode> = BTreeMap::new();
    let mut submodule: BTreeSet<(String, String)> = BTreeSet::new();

    for p in &paths {
        let croot = crate_root_of(p);
        let cid = crate_name(&croot, index);
        let loc = lines_of.get(p).copied().unwrap_or(0);
        let syms = symbols_of.get(p).copied().unwrap_or(0);

        // Crate node (rolled up).
        {
            let n = nodes.entry(cid.clone()).or_insert_with(|| ModuleNode {
                id: cid.clone(),
                kind: ModuleKind::Crate,
                loc: 0,
                symbols: 0,
            });
            n.loc += loc;
            n.symbols += syms;
        }

        // Directory chain between the crate root and the file.
        let mut parent = cid.clone();
        for dir_path in ancestor_dirs(p, &croot) {
            let kind = if is_module_dir(&dir_path) {
                ModuleKind::Module
            } else {
                ModuleKind::Dir
            };
            {
                let n = nodes.entry(dir_path.clone()).or_insert_with(|| ModuleNode {
                    id: dir_path.clone(),
                    kind,
                    loc: 0,
                    symbols: 0,
                });
                n.loc += loc;
                n.symbols += syms;
            }
            submodule.insert((parent.clone(), dir_path.clone()));
            parent = dir_path;
        }

        // File node (leaf; exact line/symbol counts).
        {
            let n = nodes.entry((*p).to_string()).or_insert_with(|| ModuleNode {
                id: (*p).to_string(),
                kind: ModuleKind::File,
                loc: 0,
                symbols: 0,
            });
            n.loc = loc;
            n.symbols = syms;
        }
        submodule.insert((parent, (*p).to_string()));
    }

    // Import edges: file-level resolution, then map to the finest node present.
    let mut import_counts: BTreeMap<(String, String), usize> = BTreeMap::new();
    for e in crate::resolve::resolve_imports(&index.files, &index.imports) {
        let from = endpoint_node(&e.caller, &nodes);
        let to = endpoint_node(&e.callee, &nodes);
        if let (Some(from), Some(to)) = (from, to) {
            *import_counts.entry((from, to)).or_insert(0) += 1;
        }
    }

    let mut edges: Vec<ModuleEdge> = Vec::with_capacity(submodule.len() + import_counts.len());
    for (from, to) in submodule {
        edges.push(ModuleEdge {
            from,
            to,
            kind: ModuleEdgeKind::Submodule,
            count: 1,
        });
    }
    for ((from, to), count) in import_counts {
        edges.push(ModuleEdge {
            from,
            to,
            kind: ModuleEdgeKind::Import,
            count,
        });
    }
    edges.sort_by(|a, b| {
        a.from
            .cmp(&b.from)
            .then_with(|| a.to.cmp(&b.to))
            .then_with(|| kind_rank(a.kind).cmp(&kind_rank(b.kind)))
    });

    let mut node_list: Vec<ModuleNode> = nodes.into_values().collect();
    node_list.sort_by(|a, b| {
        kind_rank_kind(a.kind)
            .cmp(&kind_rank_kind(b.kind))
            .then_with(|| a.id.cmp(&b.id))
    });

    (node_list, edges)
}

/// Files/dirs that participate in at least one cycle, given the module edges.
///
/// Strongly connected components with more than one member, via Tarjan. Only
/// edges whose endpoints are both known nodes are considered, and both the
/// member order within a group and the group order are sorted, so the result is
/// deterministic for identical input. Self loops are single-node SCCs and are
/// therefore not reported.
pub fn cycles(nodes: &[ModuleNode], edges: &[ModuleEdge]) -> Vec<Vec<String>> {
    let known: BTreeSet<&str> = nodes.iter().map(|n| n.id.as_str()).collect();
    let mut index_of: BTreeMap<&str, petgraph::graph::NodeIndex> = BTreeMap::new();
    let mut graph = DiGraph::<&str, ()>::new();

    for e in edges {
        if !known.contains(e.from.as_str()) || !known.contains(e.to.as_str()) {
            continue;
        }
        let a = *index_of
            .entry(e.from.as_str())
            .or_insert_with(|| graph.add_node(e.from.as_str()));
        let b = *index_of
            .entry(e.to.as_str())
            .or_insert_with(|| graph.add_node(e.to.as_str()));
        graph.add_edge(a, b, ());
    }

    let mut out: Vec<Vec<String>> = tarjan_scc(&graph)
        .into_iter()
        .filter(|scc| scc.len() > 1)
        .map(|scc| {
            let mut members: Vec<String> = scc.iter().map(|n| graph[*n].to_string()).collect();
            members.sort();
            members
        })
        .collect();
    out.sort();
    out
}

/// `crates/<name>/…` → `crates/<name>`; everything else belongs to the root crate.
fn crate_root_of(path: &str) -> String {
    let mut it = path.split('/');
    match (it.next(), it.next()) {
        (Some("crates"), Some(name)) if !name.is_empty() => format!("crates/{name}"),
        _ => String::new(),
    }
}

/// Crate display id: the crate directory name, or the repo directory name for
/// the single root crate (`"root"` when even that is unavailable).
fn crate_name(crate_root: &str, index: &Index) -> String {
    if crate_root.is_empty() {
        Path::new(&index.repo_root)
            .file_name()
            .and_then(|n| n.to_str())
            .filter(|n| !n.is_empty())
            .unwrap_or("root")
            .to_string()
    } else {
        crate_root.rsplit('/').next().unwrap_or("root").to_string()
    }
}

/// Repo-relative directories strictly between a crate root and a file, top-down.
fn ancestor_dirs(path: &str, crate_root: &str) -> Vec<String> {
    let Some((dir, _)) = path.rsplit_once('/') else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut cur = dir;
    while !cur.is_empty() && cur != crate_root {
        out.push(cur.to_string());
        match cur.rsplit_once('/') {
            Some((parent, _)) => cur = parent,
            None => break,
        }
    }
    out.reverse();
    out
}

/// Map an import endpoint (a resolved file path, a raw target, or a directory)
/// onto the finest node that exists for it, or `None` when nothing matches.
fn endpoint_node(s: &str, nodes: &BTreeMap<String, ModuleNode>) -> Option<String> {
    if nodes.contains_key(s) {
        return Some(s.to_string());
    }
    // A resolved file path that is not itself a node: fall back to its directory.
    match s.rsplit_once('/') {
        Some((parent, _)) if !parent.is_empty() && nodes.contains_key(parent) => {
            return Some(parent.to_string());
        }
        _ => {}
    }
    // A raw module path (`crate::analyze`, `pkg.dep`, …): normalise and suffix-match.
    let norm = s
        .trim()
        .trim_matches(|c| c == '"' || c == '\'')
        .trim_start_matches("crate::")
        .trim_start_matches("crate.")
        .trim_start_matches("self::")
        .trim_start_matches("self.");
    let slash = norm.replace("::", "/");
    for cand in [slash.clone(), slash.replace('.', "/")] {
        if cand.is_empty() {
            continue;
        }
        if nodes.contains_key(&cand) {
            return Some(cand);
        }
        let suffix = format!("/{cand}");
        if let Some(id) = nodes.keys().find(|id| id.ends_with(&suffix)) {
            return Some(id.to_string());
        }
    }
    None
}

fn kind_rank_kind(kind: ModuleKind) -> u8 {
    match kind {
        ModuleKind::Crate => 0,
        ModuleKind::Dir => 1,
        ModuleKind::Module => 2,
        ModuleKind::File => 3,
    }
}

fn kind_rank(kind: ModuleEdgeKind) -> u8 {
    match kind {
        ModuleEdgeKind::Submodule => 0,
        ModuleEdgeKind::Import => 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{FileEntry, Import, Symbol, SymbolKind};

    fn file(path: &str, lines: usize) -> FileEntry {
        FileEntry {
            path: path.into(),
            lang: "rust".into(),
            lines,
            mtime: 0,
            is_entrypoint: false,
        }
    }

    fn sym(file: &str, line: usize) -> Symbol {
        Symbol {
            id: format!("{file}:{line}"),
            name: "f".into(),
            kind: SymbolKind::Function,
            signature: String::new(),
            doc: String::new(),
            start_line: line,
            end_line: line,
            container: None,
            file: file.into(),
        }
    }

    fn imp(file: &str, target: &str) -> Import {
        Import {
            file: file.into(),
            target: target.into(),
        }
    }

    fn node<'a>(nodes: &'a [ModuleNode], id: &str) -> &'a ModuleNode {
        nodes.iter().find(|n| n.id == id).expect("node exists")
    }

    #[test]
    fn synthetic_tree_has_expected_nodes_edges_and_counts() {
        // 3 files, 2 dirs, 1 import.
        let mut idx = Index::new("/repo");
        idx.files = vec![
            file("src/lib.rs", 30),
            file("src/analyze/rust.rs", 20),
            file("src/analyze/python.rs", 10),
        ];
        idx.symbols = vec![
            sym("src/lib.rs", 1),
            sym("src/lib.rs", 10),
            sym("src/analyze/rust.rs", 3),
        ];
        idx.imports = vec![imp("src/analyze/rust.rs", "crate::analyze::python")];

        let (nodes, edges) = build(&idx);

        // Nodes: Crate first, then Dir, Module, then Files (sorted by id).
        let ids: Vec<&str> = nodes.iter().map(|n| n.id.as_str()).collect();
        assert_eq!(
            ids,
            vec![
                "repo",
                "src/analyze",
                "src",
                "src/analyze/python.rs",
                "src/analyze/rust.rs",
                "src/lib.rs",
            ]
        );
        assert_eq!(node(&nodes, "repo").kind, ModuleKind::Crate);
        assert_eq!(node(&nodes, "src").kind, ModuleKind::Module); // has lib.rs
        assert_eq!(node(&nodes, "src/analyze").kind, ModuleKind::Dir); // no mod.rs
        assert_eq!(node(&nodes, "src/lib.rs").kind, ModuleKind::File);

        // Roll-ups: loc + symbols over descendants.
        assert_eq!(node(&nodes, "repo").loc, 60);
        assert_eq!(node(&nodes, "src").loc, 60);
        assert_eq!(node(&nodes, "src/analyze").loc, 30);
        assert_eq!(node(&nodes, "repo").symbols, 3);
        assert_eq!(node(&nodes, "src").symbols, 3);
        assert_eq!(node(&nodes, "src/analyze").symbols, 1);
        assert_eq!(node(&nodes, "src/analyze/python.rs").loc, 10);
        assert_eq!(node(&nodes, "src/analyze/python.rs").symbols, 0);

        // Submodule edges mirror the tree.
        let sub: Vec<(&str, &str, usize)> = edges
            .iter()
            .filter(|e| e.kind == ModuleEdgeKind::Submodule)
            .map(|e| (e.from.as_str(), e.to.as_str(), e.count))
            .collect();
        assert_eq!(
            sub,
            vec![
                ("repo", "src", 1),
                ("src", "src/analyze", 1),
                ("src", "src/lib.rs", 1),
                ("src/analyze", "src/analyze/python.rs", 1),
                ("src/analyze", "src/analyze/rust.rs", 1),
            ]
        );

        // Exactly one Import edge, file → file, count 1.
        let imports: Vec<(&str, &str, usize)> = edges
            .iter()
            .filter(|e| e.kind == ModuleEdgeKind::Import)
            .map(|e| (e.from.as_str(), e.to.as_str(), e.count))
            .collect();
        assert_eq!(
            imports,
            vec![("src/analyze/rust.rs", "src/analyze/python.rs", 1)]
        );

        // Same index → identical output.
        assert_eq!(build(&idx), (nodes, edges));
    }

    #[test]
    fn duplicate_imports_aggregate_into_one_edge_with_a_count() {
        let mut idx = Index::new("/repo");
        idx.files = vec![file("src/lib.rs", 5), file("src/util.rs", 5)];
        idx.imports = vec![
            imp("src/lib.rs", "crate::util"),
            imp("src/lib.rs", "crate::util"),
        ];

        let (_, edges) = build(&idx);
        let imports: Vec<(&str, &str, usize)> = edges
            .iter()
            .filter(|e| e.kind == ModuleEdgeKind::Import)
            .map(|e| (e.from.as_str(), e.to.as_str(), e.count))
            .collect();
        assert_eq!(imports, vec![("src/lib.rs", "src/util.rs", 2)]);
    }

    #[test]
    fn derives_crates_from_workspace_layout() {
        let mut idx = Index::new("/repo");
        idx.files = vec![
            file("crates/core/src/lib.rs", 5),
            file("crates/gui/src/main.rs", 7),
        ];

        let (nodes, edges) = build(&idx);
        assert_eq!(node(&nodes, "core").kind, ModuleKind::Crate);
        assert_eq!(node(&nodes, "gui").kind, ModuleKind::Crate);
        assert_eq!(node(&nodes, "core").loc, 5);
        assert_eq!(node(&nodes, "gui").loc, 7);
        assert!(edges.contains(&ModuleEdge {
            from: "core".into(),
            to: "crates/core/src".into(),
            kind: ModuleEdgeKind::Submodule,
            count: 1,
        }));
    }

    #[test]
    fn file_without_directories_hangs_off_its_crate() {
        let mut idx = Index::new("/repo");
        idx.files = vec![file("main.rs", 3)];

        let (nodes, edges) = build(&idx);
        assert_eq!(
            nodes.iter().map(|n| n.id.as_str()).collect::<Vec<_>>(),
            vec!["repo", "main.rs"]
        );
        assert_eq!(
            edges,
            vec![ModuleEdge {
                from: "repo".into(),
                to: "main.rs".into(),
                kind: ModuleEdgeKind::Submodule,
                count: 1,
            }]
        );
    }

    #[test]
    fn finds_a_two_cycle_and_ignores_self_loops() {
        let nodes = vec![
            ModuleNode {
                id: "a".into(),
                kind: ModuleKind::File,
                loc: 0,
                symbols: 0,
            },
            ModuleNode {
                id: "b".into(),
                kind: ModuleKind::File,
                loc: 0,
                symbols: 0,
            },
            ModuleNode {
                id: "c".into(),
                kind: ModuleKind::File,
                loc: 0,
                symbols: 0,
            },
        ];
        let edges = vec![
            ModuleEdge {
                from: "a".into(),
                to: "b".into(),
                kind: ModuleEdgeKind::Import,
                count: 1,
            },
            ModuleEdge {
                from: "b".into(),
                to: "a".into(),
                kind: ModuleEdgeKind::Import,
                count: 2,
            },
            ModuleEdge {
                from: "a".into(),
                to: "a".into(),
                kind: ModuleEdgeKind::Import,
                count: 1,
            },
            ModuleEdge {
                from: "a".into(),
                to: "c".into(),
                kind: ModuleEdgeKind::Import,
                count: 1,
            },
        ];

        assert_eq!(
            cycles(&nodes, &edges),
            vec![vec!["a".to_string(), "b".to_string()]]
        );
    }

    #[test]
    fn empty_index_yields_empty_vectors() {
        let (nodes, edges) = build(&Index::new("/repo"));
        assert!(nodes.is_empty());
        assert!(edges.is_empty());
        assert!(cycles(&nodes, &edges).is_empty());
    }
}
