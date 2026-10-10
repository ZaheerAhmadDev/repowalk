//! Reading-order tour (plan §7.2): topological layers of import DAG,
//! score = fan_in + 0.8*entrypoint + 0.5*docs - 0.3*depth, SCCs collapsed.
//! OWNER: RSK-7.
use crate::model::{Confidence, EdgeKind, Index, Symbol};
use petgraph::algo::tarjan_scc;
use petgraph::graph::DiGraph;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TourItem {
    pub path: String,
    pub why: String, // docstring first sentence, else "fn {name} — called by N, calls M"
}

/// Top `limit` items in recommended reading order (deterministic).
pub fn tour(idx: &Index, limit: usize) -> Vec<TourItem> {
    if limit == 0 {
        return Vec::new();
    }
    let paths: Vec<String> = idx
        .files
        .iter()
        .map(|f| f.path.as_str())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .map(String::from)
        .collect();
    if paths.is_empty() {
        return Vec::new();
    }

    let mut node_of: BTreeMap<&str, petgraph::graph::NodeIndex> = BTreeMap::new();
    let mut graph = DiGraph::<(), ()>::new();
    for p in &paths {
        node_of.insert(p.as_str(), graph.add_node(()));
    }

    // Import DAG: importer -> imported, deduped, file-level nodes only.
    let mut dag: BTreeSet<(petgraph::graph::NodeIndex, petgraph::graph::NodeIndex)> =
        BTreeSet::new();
    for e in &idx.edges {
        if e.kind == EdgeKind::Import {
            if let (Some(&a), Some(&b)) = (
                node_of.get(e.caller.as_str()),
                node_of.get(e.callee.as_str()),
            ) {
                if a != b {
                    dag.insert((a, b));
                }
            }
        }
    }
    for im in &idx.imports {
        if let (Some(&a), Some(&b)) =
            (node_of.get(im.file.as_str()), node_of.get(im.target.as_str()))
        {
            if a != b {
                dag.insert((a, b));
            }
        }
    }
    for &(a, b) in &dag {
        graph.add_edge(a, b, ());
    }

    let n = paths.len();
    let mut fan_in = vec![0usize; n];
    let mut fan_out = vec![0usize; n];
    for &(a, b) in &dag {
        fan_in[b.index()] += 1;
        fan_out[a.index()] += 1;
    }

    // Collapse SCCs, then layer the condensed DAG with Kahn waves.
    let mut sccs = tarjan_scc(&graph);
    for c in &mut sccs {
        c.sort();
    }
    sccs.sort_by_key(|c| c[0].index());
    let mut comp_of = vec![0usize; n];
    for (ci, c) in sccs.iter().enumerate() {
        for &m in c {
            comp_of[m.index()] = ci;
        }
    }
    let ncomp = sccs.len();
    let mut cedges: BTreeSet<(usize, usize)> = BTreeSet::new();
    for &(a, b) in &dag {
        let (ca, cb) = (comp_of[a.index()], comp_of[b.index()]);
        if ca != cb {
            cedges.insert((ca, cb));
        }
    }
    let mut succ: Vec<Vec<usize>> = vec![Vec::new(); ncomp];
    let mut indeg = vec![0usize; ncomp];
    for &(ca, cb) in &cedges {
        succ[ca].push(cb);
        indeg[cb] += 1;
    }
    let mut depth = vec![0usize; ncomp];
    let mut placed = vec![false; ncomp];
    let mut layer_idx = 0usize;
    loop {
        let wave: Vec<usize> = (0..ncomp)
            .filter(|&c| !placed[c] && indeg[c] == 0)
            .collect();
        if wave.is_empty() {
            break;
        }
        for &c in &wave {
            placed[c] = true;
            depth[c] = layer_idx;
        }
        for &c in &wave {
            for &s in &succ[c] {
                indeg[s] -= 1;
            }
        }
        layer_idx += 1;
    }

    let mut has_docs = vec![false; n];
    for s in &idx.symbols {
        if !s.doc.trim().is_empty() {
            if let Some(&m) = node_of.get(s.file.as_str()) {
                has_docs[m.index()] = true;
            }
        }
    }
    let mut entry = vec![false; n];
    for f in &idx.files {
        if f.is_entrypoint {
            if let Some(&m) = node_of.get(f.path.as_str()) {
                entry[m.index()] = true;
            }
        }
    }
    let mut score = vec![0f64; n];
    for m in 0..n {
        score[m] = fan_in[m] as f64
            + 0.8 * entry[m] as f64
            + 0.5 * has_docs[m] as f64
            - 0.3 * depth[comp_of[m]] as f64;
    }

    let mut waves: Vec<Vec<usize>> = vec![Vec::new(); layer_idx];
    for (c, d) in depth.iter().enumerate() {
        waves[*d].push(c);
    }

    let mut out: Vec<TourItem> = Vec::new();
    'emit: for wave in waves.iter() {
        let mut comps = wave.clone();
        comps.sort_by(|&a, &b| {
            let sa = sccs[a]
                .iter()
                .map(|m| score[m.index()])
                .fold(f64::MIN, f64::max);
            let sb = sccs[b]
                .iter()
                .map(|m| score[m.index()])
                .fold(f64::MIN, f64::max);
            sb.partial_cmp(&sa)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| paths[sccs[a][0].index()].cmp(&paths[sccs[b][0].index()]))
        });
        for &c in &comps {
            let mut members: Vec<usize> = sccs[c].iter().map(|m| m.index()).collect();
            members.sort_by(|&a, &b| {
                score[b]
                    .partial_cmp(&score[a])
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| paths[a].cmp(&paths[b]))
            });
            for m in members {
                if out.len() >= limit {
                    break 'emit;
                }
                out.push(TourItem {
                    path: paths[m].clone(),
                    why: why_line(idx, &paths[m], fan_in[m], fan_out[m]),
                });
            }
        }
    }
    out
}

fn why_line(idx: &Index, path: &str, fan_in: usize, fan_out: usize) -> String {
    let mut syms: Vec<&Symbol> = idx.symbols.iter().filter(|s| s.file == path).collect();
    syms.sort_by(|a, b| a.start_line.cmp(&b.start_line).then_with(|| a.id.cmp(&b.id)));
    if let Some(s) = syms.iter().find(|s| !s.doc.trim().is_empty()) {
        return first_sentence(&s.doc);
    }
    if let Some(s) = syms.first() {
        let callers = idx
            .edges
            .iter()
            .filter(|e| {
                e.kind == EdgeKind::Call
                    && e.confidence != Confidence::Unresolved
                    && e.callee == s.id
            })
            .map(|e| e.caller.as_str())
            .collect::<BTreeSet<_>>()
            .len();
        let calls = idx
            .edges
            .iter()
            .filter(|e| {
                e.kind == EdgeKind::Call
                    && e.confidence != Confidence::Unresolved
                    && e.caller == s.id
            })
            .map(|e| e.callee.as_str())
            .collect::<BTreeSet<_>>()
            .len();
        return format!("fn {} — called by {}, calls {}", s.name, callers, calls);
    }
    let name = path.rsplit('/').next().unwrap_or(path);
    let stem = name.rsplit_once('.').map(|(a, _)| a).unwrap_or(name);
    format!("fn {} — called by {}, calls {}", stem, fan_in, fan_out)
}

fn first_sentence(doc: &str) -> String {
    let flat = doc.split_whitespace().collect::<Vec<_>>().join(" ");
    match flat.find(". ") {
        Some(i) => flat[..=i].to_string(),
        None => flat,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Edge, FileEntry, Import, SymbolKind};

    fn file(path: &str, entrypoint: bool) -> FileEntry {
        FileEntry {
            path: path.into(),
            lang: "rust".into(),
            lines: 10,
            mtime: 0,
            is_entrypoint: entrypoint,
        }
    }

    fn symbol(file: &str, line: usize, name: &str, doc: &str) -> Symbol {
        Symbol {
            id: format!("{}:{}", file, line),
            name: name.into(),
            kind: SymbolKind::Function,
            signature: format!("fn {}()", name),
            doc: doc.into(),
            start_line: line,
            end_line: line + 1,
            container: None,
            file: file.into(),
        }
    }

    fn import_edge(caller: &str, callee: &str) -> Edge {
        Edge {
            caller: caller.into(),
            callee: callee.into(),
            line: 1,
            kind: EdgeKind::Import,
            confidence: Confidence::Exact,
        }
    }

    fn call_edge(caller: &str, callee: &str) -> Edge {
        Edge {
            caller: caller.into(),
            callee: callee.into(),
            line: 1,
            kind: EdgeKind::Call,
            confidence: Confidence::Exact,
        }
    }

    fn diamond_index() -> Index {
        let mut idx = Index::new("/repo");
        idx.files = vec![
            file("src/a.rs", false),
            file("src/b.rs", false),
            file("src/c.rs", false),
            file("src/main.rs", true),
            file("src/z.rs", false),
        ];
        idx.symbols = vec![
            symbol("src/main.rs", 1, "main", ""),
            symbol("src/a.rs", 1, "load_config", "Reads the config. Then parses it."),
            symbol("src/b.rs", 1, "helper", ""),
        ];
        idx.edges = vec![
            import_edge("src/main.rs", "src/a.rs"),
            import_edge("src/main.rs", "src/b.rs"),
            import_edge("src/a.rs", "src/c.rs"),
            call_edge("src/main.rs:1", "src/a.rs:1"),
            call_edge("src/main.rs:1", "src/b.rs:1"),
        ];
        idx
    }

    fn cycle_index() -> Index {
        let mut idx = Index::new("/repo");
        idx.files = vec![
            file("src/main.rs", true),
            file("src/x.rs", false),
            file("src/y.rs", false),
        ];
        idx.symbols = vec![
            symbol("src/main.rs", 1, "main", ""),
            symbol("src/x.rs", 1, "xcycle", ""),
            symbol("src/y.rs", 1, "ycycle", ""),
        ];
        idx.imports = vec![
            Import {
                file: "src/main.rs".into(),
                target: "src/x.rs".into(),
            },
            Import {
                file: "src/x.rs".into(),
                target: "src/y.rs".into(),
            },
            Import {
                file: "src/y.rs".into(),
                target: "src/x.rs".into(),
            },
            Import {
                file: "src/x.rs".into(),
                target: "crate::nowhere".into(),
            },
        ];
        idx
    }

    #[test]
    fn layers_entrypoint_first_deterministic() {
        let idx = diamond_index();
        let paths: Vec<String> = tour(&idx, 10).iter().map(|t| t.path.clone()).collect();
        assert_eq!(
            paths,
            vec![
                "src/main.rs",
                "src/z.rs",
                "src/a.rs",
                "src/b.rs",
                "src/c.rs"
            ]
        );
        assert_eq!(paths[0], "src/main.rs");

        let again: Vec<String> = tour(&idx, 10).iter().map(|t| t.path.clone()).collect();
        assert_eq!(paths, again);
    }

    #[test]
    fn why_lines_doc_or_callers() {
        let idx = diamond_index();
        let items = tour(&idx, 10);
        let why = |p: &str| {
            items
                .iter()
                .find(|t| t.path == p)
                .map(|t| t.why.clone())
                .unwrap()
        };
        assert_eq!(why("src/a.rs"), "Reads the config.");
        assert_eq!(why("src/main.rs"), "fn main — called by 0, calls 2");
        assert_eq!(why("src/b.rs"), "fn helper — called by 1, calls 0");
        assert_eq!(why("src/z.rs"), "fn z — called by 0, calls 0");
        assert_eq!(why("src/c.rs"), "fn c — called by 1, calls 0");
    }

    #[test]
    fn cycle_collapsed_and_unresolvable_import_ignored() {
        let idx = cycle_index();
        let items = tour(&idx, 10);
        let paths: Vec<String> = items.iter().map(|t| t.path.clone()).collect();
        assert_eq!(paths, vec!["src/main.rs", "src/x.rs", "src/y.rs"]);
    }

    #[test]
    fn limit_and_empty_index() {
        let idx = diamond_index();
        let top: Vec<String> = tour(&idx, 2).iter().map(|t| t.path.clone()).collect();
        assert_eq!(top, vec!["src/main.rs", "src/z.rs"]);
        assert!(tour(&idx, 0).is_empty());

        let empty = Index::new("/repo");
        assert!(tour(&empty, 10).is_empty());
    }
}
