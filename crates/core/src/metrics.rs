//! Derived repo metrics: LOC, languages, entrypoints, top files, fan-in,
//! SCC cycles, god-files, unresolved-edge % (plan §7.4), flow reachability.
//! OWNER: RSK-2.
use crate::model::{Confidence, Index};
use petgraph::algo::tarjan_scc;
use petgraph::graph::DiGraph;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Metrics {
    pub total_loc: usize,
    pub langs: Vec<(String, usize)>, // (lang, file count) sorted desc
    pub entrypoints: Vec<String>,
    pub top_files: Vec<(String, usize)>, // (path, lines) top 10
    pub fan_in: Vec<(String, usize)>,    // (symbol id, callers) sorted desc
    pub cycles: Vec<Vec<String>>,        // SCCs with len > 1 (symbol ids)
    pub god_files: Vec<String>,          // lines > max(500, p99)
    pub unresolved_pct: f64,             // confidence in {Ambiguous, Unresolved} / total edges
}

/// Compute all metrics from a finished index (deterministic).
///
/// `unresolved_pct` is the fraction `count({Ambiguous, Unresolved}) / total
/// edges` in `0..1` (multiply by 100 for a percent); `0.0` when there are no
/// edges. `god_files` uses the nearest-rank p99 of file line counts and keeps
/// files with `lines > max(500, p99)`. Every list is explicitly sorted so the
/// output is stable for identical inputs.
pub fn compute(idx: &Index) -> Metrics {
    let total_loc = idx.files.iter().map(|f| f.lines).sum();

    let mut lang_counts: BTreeMap<&str, usize> = BTreeMap::new();
    for f in &idx.files {
        *lang_counts.entry(f.lang.as_str()).or_insert(0) += 1;
    }
    let mut langs: Vec<(String, usize)> = lang_counts
        .into_iter()
        .map(|(l, n)| (l.to_string(), n))
        .collect();
    langs.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

    let mut entrypoints: Vec<String> = idx
        .files
        .iter()
        .filter(|f| f.is_entrypoint)
        .map(|f| f.path.clone())
        .collect();
    entrypoints.sort();

    let mut top_files: Vec<(String, usize)> = idx
        .files
        .iter()
        .map(|f| (f.path.clone(), f.lines))
        .collect();
    top_files.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    top_files.truncate(10);

    // Fan-in: distinct callers per callee symbol id.
    let mut callers: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for e in &idx.edges {
        callers
            .entry(e.callee.as_str())
            .or_default()
            .insert(e.caller.as_str());
    }
    let mut fan_in: Vec<(String, usize)> = callers
        .into_iter()
        .map(|(c, n)| (c.to_string(), n.len()))
        .collect();
    fan_in.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

    // Cycles: Tarjan SCC over the full edge graph, keep components len > 1.
    let mut node_of: BTreeMap<&str, petgraph::graph::NodeIndex> = BTreeMap::new();
    let mut graph = DiGraph::<&str, ()>::new();
    let mut pairs: BTreeSet<(petgraph::graph::NodeIndex, petgraph::graph::NodeIndex)> =
        BTreeSet::new();
    for e in &idx.edges {
        let a = *node_of
            .entry(e.caller.as_str())
            .or_insert_with(|| graph.add_node(e.caller.as_str()));
        let b = *node_of
            .entry(e.callee.as_str())
            .or_insert_with(|| graph.add_node(e.callee.as_str()));
        pairs.insert((a, b));
    }
    for &(a, b) in &pairs {
        graph.add_edge(a, b, ());
    }
    let mut cycles: Vec<Vec<String>> = tarjan_scc(&graph)
        .into_iter()
        .filter(|scc| scc.len() > 1)
        .map(|scc| {
            let mut members: Vec<String> = scc.iter().map(|n| graph[*n].to_string()).collect();
            members.sort();
            members
        })
        .collect();
    cycles.sort();

    // God-files: lines > max(500, nearest-rank p99 of file lines).
    let mut all_lines: Vec<usize> = idx.files.iter().map(|f| f.lines).collect();
    all_lines.sort_unstable();
    let p99 = if all_lines.is_empty() {
        0usize
    } else {
        let rank = (all_lines.len() as f64 * 0.99).ceil() as usize;
        all_lines[rank.max(1) - 1]
    };
    let threshold = 500usize.max(p99);
    let mut god_files: Vec<(String, usize)> = idx
        .files
        .iter()
        .filter(|f| f.lines > threshold)
        .map(|f| (f.path.clone(), f.lines))
        .collect();
    god_files.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    let god_files: Vec<String> = god_files.into_iter().map(|(p, _)| p).collect();

    let weak = idx
        .edges
        .iter()
        .filter(|e| matches!(e.confidence, Confidence::Ambiguous | Confidence::Unresolved))
        .count();
    let unresolved_pct = if idx.edges.is_empty() {
        0.0
    } else {
        weak as f64 / idx.edges.len() as f64
    };

    Metrics {
        total_loc,
        langs,
        entrypoints,
        top_files,
        fan_in,
        cycles,
        god_files,
        unresolved_pct,
    }
}

/// Reachability of the call graph from one source symbol (flow sources).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReachStats {
    pub reachable: usize, // unique symbols across BFS layers
    pub max_depth: usize, // depth of the last non-empty layer
    pub symbols: usize,   // total symbols in the index
}

/// BFS-reachability from `source` capped at `max_depth`; honest zeros on
/// unknown source.
///
/// Layers come from [`crate::flow::waves`]: layer `k` holds the symbols first
/// reached at depth `k`, so `max_depth` is the depth of the last non-empty
/// layer (`0` when only the source layer exists) and `reachable` counts unique
/// symbols across all layers. `symbols` is `idx.symbols.len()`. An empty or
/// unknown `source` (no `waves` layers) yields all zeros.
pub fn reachability(idx: &Index, source: &str, max_depth: usize) -> ReachStats {
    let layers = crate::flow::waves(idx, source, max_depth);
    if layers.is_empty() {
        return ReachStats {
            reachable: 0,
            max_depth: 0,
            symbols: 0,
        };
    }
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    let mut depth = 0usize;
    for (k, layer) in layers.iter().enumerate() {
        if !layer.is_empty() {
            depth = k;
            seen.extend(layer.iter().map(String::as_str));
        }
    }
    ReachStats {
        reachable: seen.len(),
        max_depth: depth,
        symbols: idx.symbols.len(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Edge, EdgeKind, FileEntry, Symbol, SymbolKind};

    fn file(path: &str, lang: &str, lines: usize, entrypoint: bool) -> FileEntry {
        FileEntry {
            path: path.into(),
            lang: lang.into(),
            lines,
            mtime: 0,
            is_entrypoint: entrypoint,
        }
    }

    fn edge(caller: &str, callee: &str, confidence: Confidence) -> Edge {
        Edge {
            caller: caller.into(),
            callee: callee.into(),
            line: 1,
            kind: EdgeKind::Call,
            confidence,
        }
    }

    fn many_files(n: usize, lines: usize) -> Vec<FileEntry> {
        (0..n)
            .map(|i| file(&format!("src/f{}.rs", i), "rust", lines, false))
            .collect()
    }

    fn symbol(id: &str) -> Symbol {
        Symbol {
            id: id.into(),
            name: id.into(),
            kind: SymbolKind::Function,
            signature: String::new(),
            doc: String::new(),
            start_line: 1,
            end_line: 1,
            container: None,
            file: "src/lib.rs".into(),
        }
    }

    #[test]
    fn aggregates_loc_langs_entrypoints_and_fan_in() {
        let mut idx = Index::new("/repo");
        idx.files = vec![
            file("src/main.rs", "rust", 3, true),
            file("src/lib.rs", "rust", 5, false),
            file("tool.py", "python", 2, false),
            file("notes.md", "markdown", 1, false),
        ];
        idx.edges = vec![
            edge("src/main.rs:1", "src/lib.rs:1", Confidence::Exact),
            edge("src/main.rs:1", "src/lib.rs:1", Confidence::Exact),
            edge("tool.py:1", "src/lib.rs:1", Confidence::Exact),
            edge("src/main.rs:1", "tool.py:1", Confidence::Exact),
        ];

        let m = compute(&idx);
        assert_eq!(m.total_loc, 11);
        assert_eq!(
            m.langs,
            vec![
                ("rust".to_string(), 2),
                ("markdown".to_string(), 1),
                ("python".to_string(), 1)
            ]
        );
        assert_eq!(m.entrypoints, vec!["src/main.rs".to_string()]);
        assert_eq!(
            m.top_files,
            vec![
                ("src/lib.rs".to_string(), 5),
                ("src/main.rs".to_string(), 3),
                ("tool.py".to_string(), 2),
                ("notes.md".to_string(), 1)
            ]
        );
        // duplicate caller counted once: lib.rs has 2 distinct callers.
        assert_eq!(
            m.fan_in,
            vec![
                ("src/lib.rs:1".to_string(), 2),
                ("tool.py:1".to_string(), 1)
            ]
        );
        assert!(m.cycles.is_empty());
        assert!(m.god_files.is_empty());
        assert_eq!(m.unresolved_pct, 0.0);
    }

    #[test]
    fn top_files_capped_at_ten_deterministically() {
        let mut idx = Index::new("/repo");
        idx.files = (0..12)
            .map(|i| file(&format!("src/f{:02}.rs", i), "rust", i, false))
            .collect();

        let m = compute(&idx);
        assert_eq!(m.top_files.len(), 10);
        assert_eq!(m.top_files[0], ("src/f11.rs".to_string(), 11));
        assert_eq!(m.top_files[9], ("src/f02.rs".to_string(), 2));
    }

    #[test]
    fn cycles_only_sccs_with_more_than_one_node() {
        let mut idx = Index::new("/repo");
        idx.edges = vec![
            edge("a", "b", Confidence::Exact),
            edge("b", "c", Confidence::Exact),
            edge("c", "a", Confidence::Exact),
            edge("d", "e", Confidence::Exact),
            edge("e", "d", Confidence::Exact),
            edge("f", "g", Confidence::Exact),
            edge("h", "h", Confidence::Exact),
        ];

        let m = compute(&idx);
        assert_eq!(
            m.cycles,
            vec![
                vec!["a".to_string(), "b".to_string(), "c".to_string()],
                vec!["d".to_string(), "e".to_string()]
            ]
        );
    }

    #[test]
    fn god_files_threshold_is_max_of_500_and_p99() {
        // p99 = 10 < 500 → threshold 500: 450 is fine, 501 (strictly over) is god.
        let mut idx = Index::new("/repo");
        idx.files = many_files(99, 10);
        idx.files.push(file("src/cap.rs", "rust", 450, false));
        assert!(compute(&idx).god_files.is_empty());

        let mut idx = Index::new("/repo");
        idx.files = many_files(99, 10);
        idx.files.push(file("src/god.rs", "rust", 501, false));
        assert_eq!(compute(&idx).god_files, vec!["src/god.rs".to_string()]);

        // p99 = 550 > 500 → threshold 550: only 700 qualifies, the 550s do not.
        let mut idx = Index::new("/repo");
        idx.files = many_files(99, 550);
        idx.files.push(file("src/god.rs", "rust", 700, false));
        assert_eq!(compute(&idx).god_files, vec!["src/god.rs".to_string()]);
    }

    #[test]
    fn unresolved_pct_counts_ambiguous_and_unresolved() {
        let mut idx = Index::new("/repo");
        idx.edges = vec![
            edge("a", "b", Confidence::Exact),
            edge("a", "c", Confidence::NameOnly),
            edge("b", "c", Confidence::Ambiguous),
            edge("c", "a", Confidence::Unresolved),
            edge("d", "a", Confidence::Exact),
        ];

        let m = compute(&idx);
        assert!((m.unresolved_pct - 0.4).abs() < 1e-12);

        let empty = Index::new("/repo");
        assert_eq!(compute(&empty).unresolved_pct, 0.0);
    }

    #[test]
    fn reachability_walks_known_chain() {
        let mut idx = Index::new("/repo");
        idx.symbols = vec![symbol("a"), symbol("b"), symbol("c"), symbol("d")];
        idx.edges = vec![
            edge("a", "b", Confidence::Exact),
            edge("b", "c", Confidence::Exact),
        ];

        // a (0) → b (1) → c (2); `d` has no edge so is never reached.
        let r = reachability(&idx, "a", 10);
        assert_eq!(
            r,
            ReachStats {
                reachable: 3,
                max_depth: 2,
                symbols: 4
            }
        );
        // `c` is only a callee: still a known source, alone at depth 0.
        let r = reachability(&idx, "c", 10);
        assert_eq!(
            r,
            ReachStats {
                reachable: 1,
                max_depth: 0,
                symbols: 4
            }
        );
    }

    #[test]
    fn reachability_unknown_source_is_all_zeros() {
        let mut idx = Index::new("/repo");
        idx.symbols = vec![symbol("a"), symbol("b")];
        idx.edges = vec![edge("a", "b", Confidence::Exact)];

        assert_eq!(
            reachability(&idx, "ghost", 5),
            ReachStats {
                reachable: 0,
                max_depth: 0,
                symbols: 0
            }
        );
        assert_eq!(
            reachability(&idx, "", 5),
            ReachStats {
                reachable: 0,
                max_depth: 0,
                symbols: 0
            }
        );
    }

    #[test]
    fn reachability_respects_depth_cap() {
        let mut idx = Index::new("/repo");
        idx.symbols = vec![symbol("a"), symbol("b"), symbol("c"), symbol("d")];
        idx.edges = vec![
            edge("a", "b", Confidence::Exact),
            edge("b", "c", Confidence::Exact),
            edge("c", "d", Confidence::Exact),
        ];

        // cap 1 → a (depth 0) + b (depth 1); c/d are past the cap.
        let r = reachability(&idx, "a", 1);
        assert_eq!(
            r,
            ReachStats {
                reachable: 2,
                max_depth: 1,
                symbols: 4
            }
        );
        // cap 0 → source layer only.
        let r = reachability(&idx, "a", 0);
        assert_eq!(
            r,
            ReachStats {
                reachable: 1,
                max_depth: 0,
                symbols: 4
            }
        );
    }
}
