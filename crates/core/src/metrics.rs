//! Derived repo metrics: LOC, languages, entrypoints, top files, fan-in,
//! SCC cycles, god-files, unresolved-edge % (plan §7.4), flow reachability.
//! OWNER: RSK-2 (v1); v2 aggregates (F6) added by INTEL-5.
//!
//! The v1 [`compute`] surface is frozen: existing items and their behaviour
//! stay byte-identical. Everything under the "v2 aggregates" banner is
//! additive and reads [`crate::analysis::Analysis`] alongside [`Index`].
use crate::analysis::{Analysis, ModuleEdgeKind, ModuleKind, UseKind};
use crate::complexity::Hotspot;
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
    let all_lines: Vec<usize> = idx.files.iter().map(|f| f.lines).collect();
    let threshold = p99_threshold(&all_lines, 500);
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

/// Nearest-rank p99 threshold with a floor, shared by the LOC god-file rule
/// and the v2 fan-out rule (callers compare with a strict `>`).
///
/// Empty input yields the floor. For `n` values, rank = `ceil(n * 0.99)` and
/// the value that many entries in (1-based) is the p99; the result is
/// `max(floor, p99)`.
fn p99_threshold(values: &[usize], floor: usize) -> usize {
    if values.is_empty() {
        return floor;
    }
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    let rank = (sorted.len() as f64 * 0.99).ceil() as usize;
    floor.max(sorted[rank.max(1) - 1])
}

// ---------------------------------------------------------------------------
// v2 aggregates (F6) — additive; the v1 items above are unchanged.
// ---------------------------------------------------------------------------

/// Complexity rolled up the module tree (F6).
///
/// `total`/`max`/`functions` come from [`crate::complexity::per_file`]; a
/// `Crate`/`Dir`/`Module` node aggregates every descendant file. File-level
/// detail stays in [`crate::complexity::per_file`]. Sorted by `total` (desc),
/// then id (asc), so the output is stable for identical inputs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleComplexity {
    /// Module node id from [`crate::modules::build`] (a crate name or repo path).
    pub id: String,
    pub kind: ModuleKind,
    /// Sum of the subtree's function complexities.
    pub total: u32,
    /// Most complex single function in the subtree.
    pub max: u32,
    /// Number of functions in the subtree.
    pub functions: usize,
    /// Number of files with at least one CFG in the subtree.
    pub files: usize,
}

/// Roll [`crate::complexity::per_file`] up the [`crate::modules::build`] tree.
pub fn complexity_by_module(idx: &Index, analysis: &Analysis) -> Vec<ModuleComplexity> {
    let (nodes, edges) = crate::modules::build(idx);

    // child -> parent from the Submodule tree; each non-root node has one parent.
    let parent_of: BTreeMap<&str, &str> = edges
        .iter()
        .filter(|e| e.kind == ModuleEdgeKind::Submodule)
        .map(|e| (e.to.as_str(), e.from.as_str()))
        .collect();

    // module id -> (total, max, functions, files)
    let per_file = crate::complexity::per_file(analysis);
    let mut agg: BTreeMap<String, (u32, u32, usize, usize)> = BTreeMap::new();
    for fc in &per_file {
        let mut cur = Some(fc.file.as_str());
        while let Some(id) = cur {
            let entry = agg.entry(id.to_string()).or_insert((0, 0, 0, 0));
            entry.0 = entry.0.saturating_add(fc.total);
            entry.1 = entry.1.max(fc.max);
            entry.2 += fc.functions;
            entry.3 += 1;
            cur = parent_of.get(id).copied();
        }
    }

    let mut out: Vec<ModuleComplexity> = nodes
        .iter()
        .filter(|n| n.kind != ModuleKind::File)
        .map(|n| {
            let (total, max, functions, files) =
                agg.get(n.id.as_str()).copied().unwrap_or_default();
            ModuleComplexity {
                id: n.id.clone(),
                kind: n.kind,
                total,
                max,
                functions,
                files,
            }
        })
        .collect();
    out.sort_by(|a, b| b.total.cmp(&a.total).then_with(|| a.id.cmp(&b.id)));
    out
}

/// A file flagged as oversized and/or overly connected (F6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GodFile {
    pub path: String,
    pub loc: usize,
    /// Distinct callees reached from the file (fan-out).
    pub fan_out: usize,
    /// Why it was flagged: `"loc"` and/or `"fan-out"`.
    pub reasons: Vec<String>,
}

/// God files by LOC (v1 rule) and/or fan-out, ranked by LOC then path (F6).
///
/// LOC reuses the v1 nearest-rank p99 rule with a floor of 500 lines:
/// `loc > max(500, p99)`. Fan-out applies the same rule with a floor of 20
/// distinct callees. Fan-out counts distinct callee targets from call edges
/// ([`Index::edges`]) **and** call-kind use sites ([`Analysis::uses`]), so
/// unresolved calls still count. A file appears when either rule trips.
pub fn god_files(idx: &Index, analysis: &Analysis) -> Vec<GodFile> {
    // Distinct callees per caller file.
    let mut callees: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for e in &idx.edges {
        let (file, _) = crate::complexity::parse_symbol_id(&e.caller);
        callees.entry(file).or_default().insert(e.callee.clone());
    }
    for u in &analysis.uses {
        if u.kind == UseKind::Call {
            callees
                .entry(u.file.clone())
                .or_default()
                .insert(u.target.clone());
        }
    }

    let locs: Vec<usize> = idx.files.iter().map(|f| f.lines).collect();
    let loc_threshold = p99_threshold(&locs, 500);
    let fan_outs: Vec<usize> = idx
        .files
        .iter()
        .map(|f| callees.get(&f.path).map_or(0, BTreeSet::len))
        .collect();
    let fan_threshold = p99_threshold(&fan_outs, 20);

    let mut out: Vec<GodFile> = Vec::new();
    for f in &idx.files {
        let fan_out = callees.get(&f.path).map_or(0, BTreeSet::len);
        let mut reasons = Vec::new();
        if f.lines > loc_threshold {
            reasons.push("loc".to_string());
        }
        if fan_out > fan_threshold {
            reasons.push("fan-out".to_string());
        }
        if !reasons.is_empty() {
            out.push(GodFile {
                path: f.path.clone(),
                loc: f.lines,
                fan_out,
                reasons,
            });
        }
    }
    out.sort_by(|a, b| b.loc.cmp(&a.loc).then_with(|| a.path.cmp(&b.path)));
    out
}

/// Ranked complexity hotspots — a thin delegate to
/// [`crate::complexity::hotspots`] so consumers have one metrics entry point
/// (F6). `top == 0` returns every CFG.
pub fn hotspots(analysis: &Analysis, top: usize) -> Vec<Hotspot> {
    crate::complexity::hotspots(analysis, top)
}

/// Number of variables with no reads — dead-code candidates (F4/F6).
pub fn unused_variable_count(analysis: &Analysis) -> usize {
    crate::complexity::unused_variables(analysis).len()
}

/// The v2 metrics bundle the health-v2 panel, report and CLI read (F6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetricsV2 {
    pub complexity_by_module: Vec<ModuleComplexity>,
    pub god_files: Vec<GodFile>,
    /// Ranked hotspots; empty when there are no CFGs.
    pub hotspots: Vec<Hotspot>,
    pub unused_variables: usize,
}

/// Compute every v2 aggregate from a finished `(Index, Analysis)` pair.
pub fn compute_v2(idx: &Index, analysis: &Analysis) -> MetricsV2 {
    MetricsV2 {
        complexity_by_module: complexity_by_module(idx, analysis),
        god_files: god_files(idx, analysis),
        hotspots: hotspots(analysis, 0),
        unused_variables: unused_variable_count(analysis),
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

    // ---- v2 aggregates (INTEL-5) -----------------------------------------

    fn cfg(symbol: &str, cyclomatic: u32, max_nesting: u32) -> crate::analysis::FunctionCfg {
        crate::analysis::FunctionCfg {
            symbol: symbol.into(),
            nodes: Vec::new(),
            edges: Vec::new(),
            cyclomatic,
            max_nesting,
        }
    }

    fn var(file: &str, line: usize, name: &str, reads: usize) -> crate::analysis::Variable {
        crate::analysis::Variable {
            id: format!("{file}:{line}:{name}"),
            name: name.into(),
            kind: crate::analysis::VarKind::Local,
            file: file.into(),
            decl_line: line,
            scope_start: line,
            scope_end: line,
            type_name: None,
            reads,
            writes: 0,
            first_use: None,
            last_use: None,
            enclosing: String::new(),
        }
    }

    #[test]
    fn complexity_by_module_rolls_up_the_module_tree() {
        let mut idx = Index::new("/repo");
        idx.files = vec![
            file("crates/core/src/lib.rs", "rust", 40, false),
            file("crates/core/src/analyze/rust.rs", "rust", 20, false),
        ];
        let mut a = Analysis::new();
        a.cfgs = vec![
            cfg("crates/core/src/lib.rs:1", 3, 1),
            cfg("crates/core/src/lib.rs:9", 4, 1),
            cfg("crates/core/src/analyze/rust.rs:1", 6, 2),
        ];

        let mc = complexity_by_module(&idx, &a);
        let find = |id: &str| mc.iter().find(|m| m.id == id).expect("module node");

        let core = find("core");
        assert_eq!(core.kind, ModuleKind::Crate);
        assert_eq!(
            (core.total, core.max, core.functions, core.files),
            (13, 6, 3, 2)
        );

        let src = find("crates/core/src");
        assert_eq!(
            (src.total, src.max, src.functions, src.files),
            (13, 6, 3, 2)
        );

        let analyze = find("crates/core/src/analyze");
        assert_eq!(
            (analyze.total, analyze.max, analyze.functions, analyze.files),
            (6, 6, 1, 1)
        );

        // File nodes are the per-file view, not repeated here.
        assert!(mc.iter().all(|m| m.kind != ModuleKind::File));
        // Deterministic: identical inputs → identical output.
        assert_eq!(complexity_by_module(&idx, &a), mc);
    }

    #[test]
    fn complexity_by_module_empty_inputs_are_empty() {
        assert!(complexity_by_module(&Index::new("/repo"), &Analysis::new()).is_empty());
    }

    #[test]
    fn god_files_flags_loc_and_fan_out() {
        let mut idx = Index::new("/repo");
        idx.files = (0..99)
            .map(|i| file(&format!("src/f{:02}.rs", i), "rust", 10, false))
            .collect();
        idx.files.push(file("src/big.rs", "rust", 501, false));
        idx.files.push(file("src/fan.rs", "rust", 10, false));
        // 25 distinct callees out of src/fan.rs.
        idx.edges = (0..25)
            .map(|i| edge("src/fan.rs:1", &format!("callee{i}"), Confidence::Exact))
            .collect();

        let gf = god_files(&idx, &Analysis::new());
        assert_eq!(
            gf.iter().map(|g| g.path.as_str()).collect::<Vec<_>>(),
            vec!["src/big.rs", "src/fan.rs"]
        );
        assert_eq!(gf[0].reasons, vec!["loc".to_string()]);
        assert_eq!(gf[1].reasons, vec!["fan-out".to_string()]);
        assert_eq!(gf[1].fan_out, 25);
    }

    #[test]
    fn god_files_counts_call_use_sites_too() {
        // 200 files keeps the p99 fan-out at 0, so the 20-callee floor applies.
        let mut idx = Index::new("/repo");
        idx.files = (0..200)
            .map(|i| file(&format!("src/f{:03}.rs", i), "rust", 10, false))
            .collect();
        let mut a = Analysis::new();
        a.uses = (0..21)
            .map(|i| crate::analysis::UseSite {
                target: format!("t{i}"),
                file: "src/f000.rs".into(),
                line: i + 1,
                col: 0,
                kind: UseKind::Call,
                snippet: String::new(),
            })
            .collect();

        let gf = god_files(&idx, &a);
        assert_eq!(gf.len(), 1);
        assert_eq!(gf[0].path, "src/f000.rs");
        assert_eq!(gf[0].reasons, vec!["fan-out".to_string()]);
    }

    #[test]
    fn hotspots_and_unused_variable_count_delegate() {
        let mut a = Analysis::new();
        a.cfgs = vec![cfg("src/a.rs:1", 5, 2), cfg("src/b.rs:1", 9, 1)];
        a.variables = vec![
            var("src/a.rs", 1, "used", 3),
            var("src/a.rs", 9, "dead", 0),
            var("src/b.rs", 2, "dead2", 0),
        ];

        assert_eq!(hotspots(&a, 1), crate::complexity::hotspots(&a, 1));
        assert_eq!(hotspots(&a, 0).len(), 2);
        assert_eq!(unused_variable_count(&a), 2);
        assert_eq!(unused_variable_count(&Analysis::new()), 0);
    }

    #[test]
    fn compute_v2_bundles_all_aggregates() {
        let mut idx = Index::new("/repo");
        idx.files = (0..100)
            .map(|i| file(&format!("src/f{:03}.rs", i), "rust", 10, false))
            .collect();
        idx.files.push(file("src/big.rs", "rust", 501, false));
        let mut a = Analysis::new();
        a.cfgs = vec![cfg("src/big.rs:1", 7, 3)];
        a.variables = vec![var("src/big.rs", 2, "unused", 0)];

        let m = compute_v2(&idx, &a);
        assert_eq!(m.hotspots.len(), 1);
        assert_eq!(m.unused_variables, 1);
        assert_eq!(m.god_files.len(), 1);
        assert_eq!(m.god_files[0].path, "src/big.rs");
        assert!(!m.complexity_by_module.is_empty());
    }
}
