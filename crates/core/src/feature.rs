//! Feature slices (F2b): "what happens when this runs", as an ordered call tree.
//!
//! OWNER: CORE-06 (card CORE-5). Reuses `flow`'s call-graph *semantics* (a call
//! edge keys its `caller` by the enclosing *name* while `callee` is a symbol id,
//! so both ends must be canonicalized before traversal) but not `flow`'s
//! `waves`/`CallGraph` themselves: those are private and frozen, and `waves`
//! sorts each layer lexicographically while dropping the call-site line and
//! confidence — it cannot express the contract this module renders. The tiny
//! caller-canonicalization below mirrors `flow::CallGraph::caller_key` exactly;
//! the BFS is written here because the ordered, line-carrying contract needs it.
//!
//! Filtering (documented, not silent): only `EdgeKind::Call` edges are followed,
//! and `Confidence::Unresolved` edges are skipped because they name no known
//! symbol (`resolve.rs` keeps the raw callee there) and would fabricate nodes in
//! the call tree. `Exact`, `NameOnly` and `Ambiguous` edges are all walked; when
//! several edges reach the same symbol, the smallest call line wins, ties break
//! toward the higher-confidence edge, then the lexicographically smaller parent.
use crate::model::{Confidence, EdgeKind, Index, Symbol};
use std::collections::{BTreeMap, BTreeSet};

/// One step of a feature slice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeatureStep {
    /// Symbol id reached by this step (`file:line`).
    pub symbol: String,
    /// Display name of the symbol.
    pub name: String,
    /// Source line of the call that reached it (0 for the entry itself).
    pub line: usize,
    /// Distance from the entrypoint (entry = 0).
    pub depth: usize,
}

/// An ordered, cycle-safe feature slice.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FeatureSlice {
    /// Symbol id of the entrypoint.
    pub entry: String,
    /// Steps in the order they are reached (entry first).
    pub steps: Vec<FeatureStep>,
}

impl FeatureSlice {
    /// Symbols in the slice, in order.
    pub fn symbols(&self) -> Vec<&str> {
        self.steps.iter().map(|s| s.symbol.as_str()).collect()
    }
}

/// Higher = less trusted. Used only to break line ties when picking the edge
/// that "reached" a symbol; `Unresolved` is filtered before it is ever ranked.
fn confidence_rank(c: Confidence) -> u8 {
    match c {
        Confidence::Exact => 0,
        Confidence::NameOnly => 1,
        Confidence::Ambiguous => 2,
        Confidence::Unresolved => 3,
    }
}

/// Slice the call tree from `entry` down to `depth` hops.
///
/// `None` only when `entry` is not a symbol id declared in `index.symbols`.
/// Otherwise returns an ordered, deduped, cycle-safe sequence: the entry at
/// depth 0, then each depth layer sorted by (call line, symbol id). Never
/// panics on an empty index, self-recursion, 2-cycles, `depth == 0`, or a huge
/// `depth` (it stops as soon as no new symbol is reachable).
pub fn slice(index: &Index, entry: &str, depth: usize) -> Option<FeatureSlice> {
    let entry_node = index.symbols.iter().find(|s| s.id == entry)?;

    let by_id: BTreeMap<&str, &Symbol> = index.symbols.iter().map(|s| (s.id.as_str(), s)).collect();
    let mut by_name: BTreeMap<&str, Vec<&Symbol>> = BTreeMap::new();
    for s in &index.symbols {
        by_name.entry(s.name.as_str()).or_default().push(s);
    }

    // `caller` may be keyed by enclosing *name*; resolve it to a unique symbol id
    // whose range contains the call line (exactly `flow::CallGraph::caller_key`).
    let caller_key = |raw: &str, line: usize| -> String {
        if by_id.contains_key(raw) {
            return raw.to_string();
        }
        if let Some(cands) = by_name.get(raw) {
            let mut hits = cands
                .iter()
                .copied()
                .filter(|s| s.start_line <= line && line <= s.end_line);
            if let (Some(hit), None) = (hits.next(), hits.next()) {
                return hit.id.clone();
            }
        }
        raw.to_string()
    };
    // `callee` is normally an id; fall back to a unique name, else keep it raw.
    let callee_key = |raw: &str| -> String {
        if by_id.contains_key(raw) {
            return raw.to_string();
        }
        if let Some(list) = by_name.get(raw) {
            if let [only] = list.as_slice() {
                return only.id.clone();
            }
        }
        raw.to_string()
    };
    let name_of = |id: &str| -> String {
        by_id
            .get(id)
            .map(|s| s.name.clone())
            .unwrap_or_else(|| id.to_string())
    };

    // Canonical adjacency: caller id -> (callee, call line, confidence).
    let mut adj: BTreeMap<String, Vec<(String, usize, Confidence)>> = BTreeMap::new();
    for e in &index.edges {
        if e.kind != EdgeKind::Call || e.confidence == Confidence::Unresolved {
            continue;
        }
        let caller = caller_key(&e.caller, e.line);
        let callee = callee_key(&e.callee);
        adj.entry(caller)
            .or_default()
            .push((callee, e.line, e.confidence));
    }

    let mut steps = vec![FeatureStep {
        symbol: entry.to_string(),
        name: entry_node.name.clone(),
        line: 0,
        depth: 0,
    }];
    let mut seen: BTreeSet<String> = BTreeSet::from([entry.to_string()]);
    let mut frontier: Vec<String> = vec![entry.to_string()];

    for d in 1..=depth {
        // Best reaching edge per not-yet-seen child: (line, confidence rank, parent).
        let mut best: BTreeMap<String, (usize, u8, String)> = BTreeMap::new();
        for cur in &frontier {
            let Some(edges) = adj.get(cur) else { continue };
            for (callee, line, conf) in edges {
                if seen.contains(callee) {
                    continue;
                }
                let key = (*line, confidence_rank(*conf), cur.clone());
                if best.get(callee).is_none_or(|prev| prev > &key) {
                    best.insert(callee.clone(), key);
                }
            }
        }
        if best.is_empty() {
            break; // no new symbols within `depth` (or the graph is exhausted)
        }
        // Layer order the contract promises to callers: (call line, symbol id).
        let mut layer: Vec<(String, usize)> = best
            .into_iter()
            .map(|(child, (line, _, _))| (child, line))
            .collect();
        layer.sort_by(|a, b| a.1.cmp(&b.1).then_with(|| a.0.cmp(&b.0)));
        for (child, line) in &layer {
            steps.push(FeatureStep {
                symbol: child.clone(),
                name: name_of(child),
                line: *line,
                depth: d,
            });
            seen.insert(child.clone());
        }
        frontier = layer.into_iter().map(|(c, _)| c).collect();
    }

    Some(FeatureSlice {
        entry: entry.to_string(),
        steps,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Edge, SymbolKind};

    fn sym(file: &str, line: usize, name: &str) -> Symbol {
        Symbol {
            id: format!("{file}:{line}"),
            name: name.into(),
            kind: SymbolKind::Function,
            signature: String::new(),
            doc: String::new(),
            start_line: line,
            end_line: line + 50,
            container: None,
            file: file.into(),
        }
    }

    fn call(caller: &str, callee: &str, line: usize) -> Edge {
        Edge {
            caller: caller.into(),
            callee: callee.into(),
            line,
            kind: EdgeKind::Call,
            confidence: Confidence::Exact,
        }
    }

    /// a → b (line 3) → c (line 7), b → a (line 4) forming a 2-cycle.
    fn chain_index() -> Index {
        let mut idx = Index::new("/repo");
        idx.symbols = vec![
            sym("src/a.rs", 1, "a"),
            sym("src/b.rs", 1, "b"),
            sym("src/c.rs", 1, "c"),
        ];
        idx.edges = vec![
            call("src/a.rs:1", "src/b.rs:1", 3),
            call("src/b.rs:1", "src/c.rs:1", 7),
            call("src/b.rs:1", "src/a.rs:1", 4), // 2-cycle back-edge
        ];
        idx
    }

    #[test]
    fn breadth_first_by_depth_with_names_lines_and_depths() {
        let idx = chain_index();
        let s = slice(&idx, "src/a.rs:1", 3).expect("known entry");
        assert_eq!(s.entry, "src/a.rs:1");
        let got: Vec<(&str, &str, usize, usize)> = s
            .steps
            .iter()
            .map(|t| (t.symbol.as_str(), t.name.as_str(), t.line, t.depth))
            .collect();
        assert_eq!(
            got,
            vec![
                ("src/a.rs:1", "a", 0, 0),
                ("src/b.rs:1", "b", 3, 1),
                ("src/c.rs:1", "c", 7, 2),
            ]
        );
        assert_eq!(s.symbols(), vec!["src/a.rs:1", "src/b.rs:1", "src/c.rs:1"]);
    }

    #[test]
    fn depth_caps_the_walk_and_depth_zero_is_just_the_entry() {
        let idx = chain_index();
        let d1 = slice(&idx, "src/a.rs:1", 1).unwrap();
        assert_eq!(d1.symbols(), vec!["src/a.rs:1", "src/b.rs:1"]);
        let d0 = slice(&idx, "src/a.rs:1", 0).unwrap();
        assert_eq!(d0.steps.len(), 1);
        assert_eq!(d0.steps[0].depth, 0);
        assert_eq!(d0.steps[0].line, 0);
        // Huge depth terminates on the 2-cycle instead of looping.
        let huge = slice(&idx, "src/a.rs:1", usize::MAX).unwrap();
        assert_eq!(
            huge.symbols(),
            vec!["src/a.rs:1", "src/b.rs:1", "src/c.rs:1"]
        );
    }

    #[test]
    fn entry_known_but_edgeless_is_a_single_step() {
        let mut idx = chain_index();
        idx.symbols.push(sym("src/z.rs", 1, "z"));
        let s = slice(&idx, "src/z.rs:1", 5).unwrap();
        assert_eq!(s.steps.len(), 1);
        assert_eq!(s.steps[0].name, "z");
    }

    #[test]
    fn unknown_entry_is_none_and_empty_index_never_panics() {
        let idx = chain_index();
        assert_eq!(slice(&idx, "src/nope.rs:1", 3), None);
        assert_eq!(slice(&idx, "", 3), None);
        assert_eq!(slice(&Index::new("/repo"), "src/a.rs:1", 3), None);
    }

    #[test]
    fn self_recursion_and_two_cycle_terminate() {
        let mut idx = Index::new("/repo");
        idx.symbols = vec![sym("src/a.rs", 1, "a"), sym("src/b.rs", 1, "b")];
        idx.edges = vec![
            call("src/a.rs:1", "src/a.rs:1", 2), // self-recursion
            call("src/a.rs:1", "src/b.rs:1", 3),
            call("src/b.rs:1", "src/a.rs:1", 4), // 2-cycle
        ];
        let s = slice(&idx, "src/a.rs:1", usize::MAX).unwrap();
        assert_eq!(s.symbols(), vec!["src/a.rs:1", "src/b.rs:1"]);
    }

    #[test]
    fn layers_are_sorted_by_call_line_then_symbol_id() {
        let mut idx = Index::new("/repo");
        idx.symbols = vec![
            sym("src/a.rs", 1, "a"),
            sym("src/p.rs", 1, "p"),
            sym("src/q.rs", 1, "q"),
            sym("src/r.rs", 1, "q"), // same id order tie-break, different file
        ];
        idx.symbols[3].id = "src/r.rs:1".into();
        idx.edges = vec![
            call("src/a.rs:1", "src/q.rs:1", 9), // later line
            call("src/a.rs:1", "src/p.rs:1", 2), // earlier line
            call("src/a.rs:1", "src/r.rs:1", 2), // same line, id sorts after p
        ];
        let s = slice(&idx, "src/a.rs:1", 1).unwrap();
        let ids: Vec<&str> = s.symbols();
        assert_eq!(
            ids,
            vec!["src/a.rs:1", "src/p.rs:1", "src/r.rs:1", "src/q.rs:1"]
        );
    }

    #[test]
    fn name_keyed_caller_resolves_and_unresolved_edges_are_not_followed() {
        let mut idx = Index::new("/repo");
        idx.symbols = vec![
            sym("src/a.rs", 1, "outer"),
            sym("src/b.rs", 1, "inner"),
            sym("src/c.rs", 1, "ghost_target"),
        ];
        idx.edges = vec![
            // caller keyed by enclosing name (pipeline style), callee by id
            call("outer", "src/b.rs:1", 5),
            // Unresolved edge to a real-looking id but low confidence: skipped.
            Edge {
                caller: "src/b.rs:1".into(),
                callee: "src/c.rs:1".into(),
                line: 6,
                kind: EdgeKind::Call,
                confidence: Confidence::Unresolved,
            },
        ];
        let s = slice(&idx, "src/a.rs:1", 5).unwrap();
        assert_eq!(s.symbols(), vec!["src/a.rs:1", "src/b.rs:1"]);
    }

    #[test]
    fn prefer_earlier_line_then_exact_on_tie() {
        let mut idx = Index::new("/repo");
        idx.symbols = vec![sym("src/a.rs", 1, "a"), sym("src/b.rs", 1, "b")];
        idx.edges = vec![
            Edge {
                caller: "src/a.rs:1".into(),
                callee: "src/b.rs:1".into(),
                line: 4,
                kind: EdgeKind::Call,
                confidence: Confidence::NameOnly,
            },
            Edge {
                caller: "src/a.rs:1".into(),
                callee: "src/b.rs:1".into(),
                line: 4,
                kind: EdgeKind::Call,
                confidence: Confidence::Exact,
            },
        ];
        let s = slice(&idx, "src/a.rs:1", 1).unwrap();
        assert_eq!(s.steps[1].line, 4);
        assert_eq!(s.symbols(), vec!["src/a.rs:1", "src/b.rs:1"]);
    }
}
