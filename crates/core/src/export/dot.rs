//! Graphviz DOT emitter. OWNER: PROD-2. Status: interface stub — replace the body.
use super::Diagram;
use crate::analysis::Analysis;
use crate::model::Index;

/// Render `diagram` as DOT source (`digraph repowalk { … }`). Deterministic;
/// every node id sanitized so names with punctuation stay valid.
pub fn render(index: &Index, analysis: &Analysis, diagram: &Diagram) -> String {
    let _ = (index, analysis, diagram);
    String::new()
}
