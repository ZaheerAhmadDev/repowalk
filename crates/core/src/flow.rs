//! Reusable call-flow primitives shared by GUI and CLI: A→B BFS trace and
//! entrypoint wave layers over `EdgeKind::Call` edges (plan §7.3).
use crate::model::{EdgeKind, Index};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

/// Deterministic `EdgeKind::Call` adjacency: self-edges skipped, neighbors
/// sorted + deduped so traversal order is stable for identical inputs.
fn call_adj(idx: &Index) -> BTreeMap<&str, Vec<&str>> {
    let mut adj: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for e in &idx.edges {
        if e.kind == EdgeKind::Call && e.callee != e.caller {
            adj.entry(e.caller.as_str())
                .or_default()
                .push(e.callee.as_str());
        }
    }
    for v in adj.values_mut() {
        v.sort_unstable();
        v.dedup();
    }
    adj
}

/// BFS shortest call path from `a` to `b` over resolved call edges
/// (endpoint-inclusive, deterministic), `None` when unreachable.
pub fn trace(idx: &Index, a: &str, b: &str) -> Option<Vec<String>> {
    if a == b {
        return Some(vec![a.to_string()]);
    }
    let adj = call_adj(idx);
    let mut prev: BTreeMap<&str, &str> = BTreeMap::new();
    let mut seen: BTreeSet<&str> = BTreeSet::from([a]);
    let mut queue = VecDeque::from([a]);
    while let Some(cur) = queue.pop_front() {
        for next in adj.get(cur).into_iter().flatten() {
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
    let adj = call_adj(idx);
    let known = idx
        .edges
        .iter()
        .any(|e| e.kind == EdgeKind::Call && (e.caller == source || e.callee == source));
    if !known {
        return Vec::new();
    }
    let mut layers = vec![vec![source.to_string()]];
    let mut seen: BTreeSet<&str> = BTreeSet::from([source]);
    let mut frontier = vec![source];
    for _ in 0..max_depth {
        let mut next: Vec<&str> = Vec::new();
        for cur in frontier {
            for callee in adj.get(cur).into_iter().flatten() {
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
