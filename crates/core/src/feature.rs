//! Feature slices (F2b): "what happens when this runs", as an ordered call tree.
//!
//! OWNER: CORE-06 (card CORE-5). Reuses `flow::waves` but returns an ORDERED,
//! deduped, cycle-safe sequence with the call site each step came from — that
//! is what the Flow ▸ Feature tab and `repowalk report` render.
use crate::model::Index;

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

/// Slice the call tree from `entry` down to `depth` hops.
/// `None` when the entrypoint is unknown. Cycle-safe by construction.
pub fn slice(index: &Index, entry: &str, depth: usize) -> Option<FeatureSlice> {
    let _ = (index, entry, depth);
    None
}
