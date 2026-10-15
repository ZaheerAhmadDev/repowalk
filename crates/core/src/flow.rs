//! Reusable call-flow primitives shared by GUI and CLI: A→B BFS trace and
//! entrypoint wave layers over `EdgeKind::Call` edges (plan §7.3).
use crate::model::{EdgeKind, Index, Symbol};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

/// Canonical call-graph view for traversal. The pipeline keys a Call edge's
/// `caller` by the enclosing *name* while `callee` is a symbol id, which
/// truncates multi-hop BFS at the name/id boundary; this view rewrites
/// resolvable callers to their ids and resolves user-facing endpoints the
/// same way, so traces cross that boundary honestly.
struct CallGraph<'a> {
    /// Deterministic adjacency: canonical keys, self-edges skipped,
    /// neighbors sorted + deduped so traversal order is stable.
    adj: BTreeMap<&'a str, Vec<&'a str>>,
    /// Every canonical edge endpoint — the "known source" test for waves.
    endpoints: BTreeSet<&'a str>,
    ids: BTreeSet<&'a str>,
    by_name: BTreeMap<&'a str, Vec<&'a Symbol>>,
}

impl<'a> CallGraph<'a> {
    fn build(idx: &'a Index) -> Self {
        let ids: BTreeSet<&str> = idx.symbols.iter().map(|s| s.id.as_str()).collect();
        let mut by_name: BTreeMap<&str, Vec<&Symbol>> = BTreeMap::new();
        for s in &idx.symbols {
            by_name.entry(s.name.as_str()).or_default().push(s);
        }
        let mut g = CallGraph {
            adj: BTreeMap::new(),
            endpoints: BTreeSet::new(),
            ids,
            by_name,
        };
        for e in &idx.edges {
            if e.kind != EdgeKind::Call {
                continue;
            }
            let caller = g.caller_key(&e.caller, e.line);
            if caller == e.callee {
                continue; // self-edge, after canonicalization
            }
            g.endpoints.insert(caller);
            g.endpoints.insert(e.callee.as_str());
            g.adj.entry(caller).or_default().push(e.callee.as_str());
        }
        for v in g.adj.values_mut() {
            v.sort_unstable();
            v.dedup();
        }
        g
    }

    /// Call-edge caller → symbol id: ids round-trip; a name-keyed caller is
    /// rewritten to the unique symbol with that name whose range contains the
    /// call line. Ambiguous or file-level callers stay raw — honest partial
    /// resolution beats a wrong path.
    fn caller_key(&self, raw: &'a str, line: usize) -> &'a str {
        if self.ids.contains(raw) {
            return raw;
        }
        let mut cands = self
            .by_name
            .get(raw)
            .into_iter()
            .flatten()
            .filter(|s| s.start_line <= line && line <= s.end_line);
        match (cands.next(), cands.next()) {
            (Some(hit), None) => &hit.id,
            _ => raw,
        }
    }

    /// User-facing endpoint (A, B, wave source) → canonical id: exact id →
    /// unique symbol name → raw string that appears in an edge → miss ("").
    /// No line context exists for user input, so ambiguous names stay
    /// unresolved instead of being guessed.
    fn resolve(&self, raw: &str) -> &'a str {
        if let Some(id) = self.ids.get(raw) {
            return id;
        }
        if let Some(list) = self.by_name.get(raw) {
            if let [only] = list.as_slice() {
                return &only.id;
            }
        }
        self.endpoints.get(raw).copied().unwrap_or("")
    }
}

/// BFS shortest call path from `a` to `b` over resolved call edges
/// (endpoint-inclusive, deterministic), `None` when unreachable.
pub fn trace(idx: &Index, a: &str, b: &str) -> Option<Vec<String>> {
    if a == b {
        return Some(vec![a.to_string()]);
    }
    let g = CallGraph::build(idx);
    let a = g.resolve(a);
    let b = g.resolve(b);
    let mut prev: BTreeMap<&str, &str> = BTreeMap::new();
    let mut seen: BTreeSet<&str> = BTreeSet::from([a]);
    let mut queue = VecDeque::from([a]);
    while let Some(cur) = queue.pop_front() {
        for next in g.adj.get(cur).into_iter().flatten() {
            if !seen.insert(next) {
                continue;
            }
            prev.insert(next, cur);
            if *next == b {
                let mut path = vec![b.to_string()];
                let mut step = b;
                while let Some(&p) = prev.get(step) {
                    path.push(p.to_string());
                    step = p;
                }
                path.reverse();
                return Some(path);
            }
            queue.push_back(*next);
        }
    }
    None
}

/// BFS layers from `source` (layer 0 = `[source]`), cycle-safe, deterministic
/// (each layer sorted lexicographically), capped at `max_depth` expansion
/// rounds — layer `k` exists only for `k <= max_depth`, nodes not reachable
/// within that cap are excluded. Empty when `source` is empty or unknown
/// (not an endpoint of any `EdgeKind::Call` edge).
pub fn waves(idx: &Index, source: &str, max_depth: usize) -> Vec<Vec<String>> {
    if source.is_empty() {
        return Vec::new();
    }
    let g = CallGraph::build(idx);
    let source = g.resolve(source);
    if source.is_empty() || !g.endpoints.contains(source) {
        return Vec::new();
    }
    let mut layers = vec![vec![source.to_string()]];
    let mut seen: BTreeSet<&str> = BTreeSet::from([source]);
    let mut frontier = vec![source];
    for _ in 0..max_depth {
        let mut next: Vec<&str> = Vec::new();
        for cur in frontier {
            for callee in g.adj.get(cur).into_iter().flatten() {
                if seen.insert(callee) {
                    next.push(callee);
                }
            }
        }
        if next.is_empty() {
            break;
        }
        next.sort_unstable();
        layers.push(next.iter().map(|s| s.to_string()).collect());
        frontier = next;
    }
    layers
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Confidence, Edge};

    fn call(caller: &str, callee: &str) -> Edge {
        Edge {
            caller: caller.into(),
            callee: callee.into(),
            line: 1,
            kind: EdgeKind::Call,
            confidence: Confidence::Exact,
        }
    }

    fn idx_with(edges: Vec<Edge>) -> Index {
        let mut idx = Index::new("/repo");
        idx.edges = edges;
        idx
    }

    fn sym(id: &str, name: &str, start: usize, end: usize) -> Symbol {
        Symbol {
            id: id.into(),
            name: name.into(),
            kind: crate::model::SymbolKind::Function,
            signature: String::new(),
            doc: String::new(),
            start_line: start,
            end_line: end,
            container: None,
            file: id.split(':').next().unwrap_or(id).to_string(),
        }
    }

    #[test]
    fn trace_crosses_the_name_id_boundary() {
        let mut idx = idx_with(vec![
            // Pipeline style: caller keyed by enclosing name, callee by id.
            Edge {
                caller: "outer".into(),
                callee: "src/a.rs:10".into(),
                line: 3,
                kind: EdgeKind::Call,
                confidence: Confidence::Exact,
            },
            Edge {
                caller: "src/a.rs:10".into(),
                callee: "src/b.rs:1".into(),
                line: 12,
                kind: EdgeKind::Call,
                confidence: Confidence::Exact,
            },
        ]);
        idx.symbols = vec![
            sym("src/a.rs:1", "outer", 1, 8),
            sym("src/a.rs:10", "inner", 10, 20),
            sym("src/b.rs:1", "leaf", 1, 5),
        ];
        // The name-keyed caller resolves to its id, so the second hop (already
        // an id) is reachable — the old contract truncated BFS here.
        assert_eq!(
            trace(&idx, "src/a.rs:1", "src/b.rs:1"),
            Some(vec![
                "src/a.rs:1".to_string(),
                "src/a.rs:10".to_string(),
                "src/b.rs:1".to_string()
            ])
        );
        // Wave fan-out from the bare name resolves and crosses the same way.
        assert_eq!(
            waves(&idx, "outer", 2),
            vec![
                vec!["src/a.rs:1".to_string()],
                vec!["src/a.rs:10".to_string()],
                vec!["src/b.rs:1".to_string()],
            ]
        );
    }

    #[test]
    fn ambiguous_name_keyed_caller_stays_raw() {
        let mut idx = idx_with(vec![Edge {
            caller: "dup".into(),
            callee: "dup.rs:1".into(),
            line: 5,
            kind: EdgeKind::Call,
            confidence: Confidence::NameOnly,
        }]);
        idx.symbols = vec![sym("one.rs:1", "dup", 1, 9), sym("two.rs:1", "dup", 1, 9)];
        // Two candidates contain line 5 → no silent guess: the caller key
        // stays raw and traversal still works from that raw key …
        assert_eq!(
            trace(&idx, "dup", "dup.rs:1"),
            Some(vec!["dup".to_string(), "dup.rs:1".to_string()])
        );
        // … while a real id never gets a wrong path through the ambiguity.
        assert_eq!(trace(&idx, "two.rs:1", "dup.rs:1"), None);
    }

    #[test]
    fn trace_same_node_returns_single_endpoint() {
        let idx = idx_with(vec![call("a", "b"), call("b", "a")]); // 2-cycle
        assert_eq!(trace(&idx, "a", "a"), Some(vec!["a".to_string()]));
        // never panics on an empty index
        assert_eq!(
            trace(&Index::new("/repo"), "x", "x"),
            Some(vec!["x".to_string()])
        );
    }

    #[test]
    fn trace_finds_shortest_multi_hop_call_path() {
        let mut idx = idx_with(vec![
            call("a", "b"),
            call("b", "c"),
            call("c", "d"),
            call("a", "c"), // shortcut → shortest is a → c → d
        ]);
        // Non-call edges and self-edges must not influence the path.
        idx.edges.push(Edge {
            caller: "a".into(),
            callee: "d".into(),
            line: 2,
            kind: EdgeKind::Import,
            confidence: Confidence::Exact,
        });
        idx.edges.push(call("a", "a"));
        assert_eq!(
            trace(&idx, "a", "d"),
            Some(vec![
                "a".to_string(),
                "c".to_string(),
                "d".to_string()
            ])
        );
    }

    #[test]
    fn trace_unreachable_is_none() {
        let idx = idx_with(vec![call("a", "b"), call("b", "c")]);
        assert_eq!(trace(&idx, "a", "z"), None);
        assert_eq!(trace(&idx, "z", "a"), None);
        assert_eq!(trace(&Index::new("/repo"), "a", "b"), None);
    }

    #[test]
    fn waves_layer_bfs_and_terminate_on_two_cycle() {
        let idx = idx_with(vec![
            call("s", "a"),
            call("s", "b"),
            call("a", "s"), // 2-cycle s ↔ a must not loop forever
            call("a", "c"),
            call("b", "d"),
        ]);
        assert_eq!(
            waves(&idx, "s", 10),
            vec![
                vec!["s".to_string()],
                vec!["a".to_string(), "b".to_string()],
                vec!["c".to_string(), "d".to_string()],
            ]
        );
    }

    #[test]
    fn waves_respect_depth_cap_and_reject_unknown_source() {
        let idx = idx_with(vec![call("a", "b"), call("b", "c"), call("c", "d")]);
        assert_eq!(waves(&idx, "a", 0), vec![vec!["a".to_string()]]);
        assert_eq!(
            waves(&idx, "a", 1),
            vec![vec!["a".to_string()], vec!["b".to_string()]]
        );
        assert_eq!(
            waves(&idx, "a", 2),
            vec![
                vec!["a".to_string()],
                vec!["b".to_string()],
                vec!["c".to_string()]
            ]
        );
        assert_eq!(waves(&idx, "", 5), Vec::<Vec<String>>::new());
        assert_eq!(waves(&idx, "zzz", 5), Vec::<Vec<String>>::new());
        assert_eq!(
            waves(&Index::new("/repo"), "a", 5),
            Vec::<Vec<String>>::new()
        );
    }
}
