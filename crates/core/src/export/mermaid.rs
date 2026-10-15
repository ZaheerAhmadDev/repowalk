//! Mermaid emitter. OWNER: PROD-2. Status: interface stub — replace the body.
use super::Diagram;
use crate::analysis::Analysis;
use crate::model::Index;

/// Render `diagram` as a Mermaid diagram body (no fence). Must be
/// deterministic and valid: unique node ids, quoted labels, no stray fences.
pub fn render(index: &Index, analysis: &Analysis, diagram: &Diagram) -> String {
    let _ = (index, analysis, diagram);
    String::new()
}
