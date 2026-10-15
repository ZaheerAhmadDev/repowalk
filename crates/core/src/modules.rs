//! Crate/dir/module structure graph (F2d).
//!
//! OWNER: CORE-04 (card CORE-4). Builds the ModuleNode/ModuleEdge set the
//! Modules diagram and the report render from: a deterministic tree of
//! crate → dir → module → file nodes, plus Import/Submodule edges aggregated
//! with counts.
use crate::analysis::{ModuleEdge, ModuleNode};
use crate::model::Index;

/// Build `(nodes, edges)` for the whole repo.
///
/// Nodes must be deterministic (same repo → same ids/order). `Import` edges
/// come from `Index::imports`, `Submodule` edges from the file/dir tree.
pub fn build(index: &Index) -> (Vec<ModuleNode>, Vec<ModuleEdge>) {
    let _ = index;
    (Vec::new(), Vec::new())
}

/// Files/dirs that participate in at least one cycle, given the module edges.
pub fn cycles(nodes: &[ModuleNode], edges: &[ModuleEdge]) -> Vec<Vec<String>> {
    let _ = (nodes, edges);
    Vec::new()
}
