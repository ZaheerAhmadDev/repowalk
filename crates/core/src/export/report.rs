//! Markdown architecture report. OWNER: PROD-2. Status: interface stub.
use crate::analysis::Analysis;
use crate::model::Index;

/// Build the self-contained Markdown report: repo overview (files, LOC,
/// languages, symbols), module map, top hotspots/cycles, complexity table,
/// unused-variable count, and the diagrams as embedded Mermaid fences.
/// No absolute paths, no timestamps that break diffing.
pub fn markdown(index: &Index, analysis: &Analysis) -> String {
    let _ = (index, analysis);
    String::new()
}
