//! Type-graph assembly (F2c).
//!
//! OWNER: CORE-05 (card CORE-5). Turns the flat `Analysis::types` /
//! `Analysis::type_edges` into a queryable graph for the Types diagram, the
//! report, and the CLI.
use crate::analysis::{Analysis, TypeEdge, TypeEdgeKind, TypeNode};

/// Queryable type graph.
#[derive(Debug, Clone, Default)]
pub struct TypeGraph {
    pub nodes: Vec<TypeNode>,
    pub edges: Vec<TypeEdge>,
}

/// Assemble the graph from an analysis artifact. Node order follows
/// `analysis.types`; edges keep only those whose endpoints both exist.
pub fn build(analysis: &Analysis) -> TypeGraph {
    let _ = analysis;
    TypeGraph::default()
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
}
