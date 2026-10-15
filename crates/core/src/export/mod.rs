//! Diagram & report emitters (F8) — shared by the GUI export menu and the CLI.
//!
//! OWNER: PROD-2 (card PROD-2). **Pure functions**: data in, text out. No file
//! I/O, no filesystem, no network — the caller decides where bytes land. That is
//! what lets `repowalk graph --format dot` (PROD-3) reuse exactly what the GUI
//! Export menu renders, instead of a second implementation drifting.
pub mod dot;
pub mod mermaid;
pub mod report;
pub mod svg;

use crate::analysis::Analysis;
use crate::model::Index;

/// Which diagram to render.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Diagram {
    /// Call graph over the index (structural `Call` edges).
    CallGraph,
    /// Control-flow graph of one function, given its symbol id (`file:line`).
    Cfg { symbol: String },
    /// Type / class diagram (structs, enums, traits, classes, interfaces).
    Types,
    /// Crate / directory / module structure with import edges.
    Modules,
}

/// Mermaid diagram **body** (no ``` fence — the caller adds one).
pub fn mermaid(index: &Index, analysis: &Analysis, diagram: &Diagram) -> String {
    mermaid::render(index, analysis, diagram)
}

/// Graphviz DOT source.
pub fn dot(index: &Index, analysis: &Analysis, diagram: &Diagram) -> String {
    dot::render(index, analysis, diagram)
}

/// Standalone SVG (hand-rolled; no SVG crate).
pub fn svg(index: &Index, analysis: &Analysis, diagram: &Diagram) -> String {
    svg::render(index, analysis, diagram)
}

/// Self-contained Markdown architecture report (overview, module map, hotspots,
/// complexity table, diagrams as Mermaid fences).
pub fn markdown_report(index: &Index, analysis: &Analysis) -> String {
    report::markdown(index, analysis)
}
