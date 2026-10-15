//! Type-graph assembly (F2c).
//!
//! OWNER: CORE-05 (card CORE-5). Turns the flat `Analysis::types` /
//! `Analysis::type_edges` into a queryable graph for the Types diagram, the
//! report, and the CLI.
use crate::analysis::{Analysis, TypeEdge, TypeEdgeKind, TypeNode};
use std::collections::{BTreeSet, HashSet};

/// Queryable type graph.
#[derive(Debug, Clone, Default)]
pub struct TypeGraph {
    pub nodes: Vec<TypeNode>,
    pub edges: Vec<TypeEdge>,
}

/// Assemble the graph from an analysis artifact. Node order follows
/// `analysis.types`; edges keep only those whose endpoints both exist, and
/// exact duplicates (same `from`/`to`/`kind`/`line`) are collapsed to the first
/// occurrence. Input order is otherwise preserved, so the result is
/// deterministic.
pub fn build(analysis: &Analysis) -> TypeGraph {
    let ids: BTreeSet<&str> = analysis.types.iter().map(|n| n.id.as_str()).collect();
    let mut seen: HashSet<(&str, &str, TypeEdgeKind, usize)> = HashSet::new();
    let mut edges = Vec::new();
    for e in &analysis.type_edges {
        // Dangling edge: an endpoint is not a declared type node.
        if !ids.contains(e.from.as_str()) || !ids.contains(e.to.as_str()) {
            continue;
        }
        let key = (e.from.as_str(), e.to.as_str(), e.kind, e.line);
        if seen.insert(key) {
            edges.push(e.clone());
        }
    }
    TypeGraph {
        nodes: analysis.types.clone(),
        edges,
    }
}

impl TypeGraph {
    /// Look up a type by its id (`file:name`).
    pub fn node(&self, id: &str) -> Option<&TypeNode> {
        self.nodes.iter().find(|n| n.id == id)
    }

    /// Types declared in one file.
    pub fn in_file(&self, file: &str) -> Vec<&TypeNode> {
        self.nodes.iter().filter(|n| n.file == file).collect()
    }

    /// Every edge touching `id`, either direction.
    pub fn edges_of(&self, id: &str) -> Vec<&TypeEdge> {
        self.edges
            .iter()
            .filter(|e| e.from == id || e.to == id)
            .collect()
    }

    /// Edges of one kind.
    pub fn edges_of_kind(&self, kind: TypeEdgeKind) -> Vec<&TypeEdge> {
        self.edges.iter().filter(|e| e.kind == kind).collect()
    }

    /// Declared supertypes of `id` via `Inherit` edges (`from → to` means
    /// "`from` inherits from `to`"), deduped, in edge order. `Impl` and other
    /// edge kinds are not inheritance and are deliberately excluded.
    pub fn supertypes(&self, id: &str) -> Vec<&TypeNode> {
        let mut seen = BTreeSet::new();
        self.edges
            .iter()
            .filter(|e| e.kind == TypeEdgeKind::Inherit && e.from == id)
            .filter_map(|e| self.node(&e.to))
            .filter(|n| seen.insert(n.id.as_str()))
            .collect()
    }

    /// Declared subtypes of `id` (types whose `Inherit` edge points at `id`),
    /// deduped, in edge order.
    pub fn subtypes(&self, id: &str) -> Vec<&TypeNode> {
        let mut seen = BTreeSet::new();
        self.edges
            .iter()
            .filter(|e| e.kind == TypeEdgeKind::Inherit && e.to == id)
            .filter_map(|e| self.node(&e.from))
            .filter(|n| seen.insert(n.id.as_str()))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::{Field, TypeKind};

    fn node(file: &str, name: &str, kind: TypeKind) -> TypeNode {
        TypeNode {
            id: format!("{file}:{name}"),
            name: name.into(),
            kind,
            file: file.into(),
            line: 1,
            fields: vec![Field {
                name: "f".into(),
                type_name: "u32".into(),
                visibility: "pub".into(),
                line: 2,
            }],
            methods: vec!["m".into()],
            doc: String::new(),
            module_path: "crate".into(),
        }
    }

    fn edge(from: &str, to: &str, kind: TypeEdgeKind, line: usize) -> TypeEdge {
        TypeEdge {
            from: from.into(),
            to: to.into(),
            kind,
            line,
        }
    }

    /// Hand-made graph: A and B declared, C declared elsewhere in the same file,
    /// plus a dangling edge to an undeclared type and a duplicate of A→B.
    fn analysis() -> Analysis {
        let mut a = Analysis::new();
        a.types = vec![
            node("src/a.rs", "A", TypeKind::Struct),
            node("src/a.rs", "C", TypeKind::Enum),
            node("src/b.rs", "B", TypeKind::Class),
        ];
        a.type_edges = vec![
            edge("src/a.rs:A", "src/b.rs:B", TypeEdgeKind::Inherit, 3),
            edge("src/a.rs:A", "src/b.rs:B", TypeEdgeKind::Inherit, 3), // dup
            edge("src/a.rs:A", "src/ghost.rs:Ghost", TypeEdgeKind::Field, 4), // dangling `to`
            edge("src/nope.rs:Nope", "src/a.rs:C", TypeEdgeKind::Impl, 5), // dangling `from`
            edge("src/b.rs:B", "src/a.rs:A", TypeEdgeKind::Impl, 6),
        ];
        a
    }

    #[test]
    fn build_keeps_node_order_filters_dangling_and_dedupes() {
        let g = build(&analysis());
        let ids: Vec<&str> = g.nodes.iter().map(|n| n.id.as_str()).collect();
        assert_eq!(
            ids,
            vec!["src/a.rs:A", "src/a.rs:C", "src/b.rs:B"],
            "node order must follow analysis.types"
        );
        assert_eq!(g.edges.len(), 2, "duplicate + 2 dangling edges dropped");
        assert_eq!(g.edges[0].line, 3);
        assert_eq!(g.edges[1].line, 6);
    }

    #[test]
    fn query_helpers_answer_by_id_file_and_endpoint() {
        let g = build(&analysis());
        assert!(g.node("src/a.rs:A").is_some());
        assert!(g.node("missing").is_none());

        let in_a: Vec<&str> = g
            .in_file("src/a.rs")
            .iter()
            .map(|n| n.id.as_str())
            .collect();
        assert_eq!(in_a, vec!["src/a.rs:A", "src/a.rs:C"]);
        assert!(g.in_file("src/z.rs").is_empty());

        // edges_of(A) = A→B (inherit) and B→A (impl); C only appears on a
        // dangling edge that was filtered, so it has none.
        assert_eq!(g.edges_of("src/a.rs:A").len(), 2);
        assert!(g.edges_of("src/a.rs:C").is_empty());

        let inherits = g.edges_of_kind(TypeEdgeKind::Inherit);
        assert_eq!(inherits.len(), 1);
        assert_eq!(inherits[0].from, "src/a.rs:A");
    }

    #[test]
    fn supertypes_and_subtypes_follow_inherit_edges() {
        let g = build(&analysis());
        let sup: Vec<&str> = g
            .supertypes("src/a.rs:A")
            .iter()
            .map(|n| n.id.as_str())
            .collect();
        assert_eq!(sup, vec!["src/b.rs:B"]);

        let sub: Vec<&str> = g
            .subtypes("src/b.rs:B")
            .iter()
            .map(|n| n.id.as_str())
            .collect();
        assert_eq!(sub, vec!["src/a.rs:A"]);

        assert!(g.supertypes("src/b.rs:B").is_empty());
        assert!(g.subtypes("src/a.rs:A").is_empty());
        assert!(g.supertypes("missing").is_empty());
    }

    #[test]
    fn empty_analysis_is_an_empty_graph() {
        let g = build(&Analysis::new());
        assert!(g.nodes.is_empty() && g.edges.is_empty());
        assert!(g.node("x").is_none());
        assert!(g.edges_of("x").is_empty());
    }
}
