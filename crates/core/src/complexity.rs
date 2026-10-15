//! Complexity + hotspot aggregates over the v2 analysis (F6).
//!
//! OWNER: CORE-03 (card CORE-4). The names below are the agreed starting
//! contract for health-v2 / CLI / report consumers; extend them, but do not
//! rename or change the meaning of an existing one without messaging PMO.
use crate::analysis::{Analysis, Variable};

/// One ranked control-flow hotspot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hotspot {
    /// Symbol id (`file:line`).
    pub symbol: String,
    pub file: String,
    pub line: usize,
    pub cyclomatic: u32,
    pub max_nesting: u32,
}

/// Per-file complexity roll-up.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileComplexity {
    pub file: String,
    pub total: u32,
    pub max: u32,
    pub functions: usize,
}

/// Split a canonical symbol id (`src/main.rs:12`) into `(file, line)`.
/// Returns `(id, 0)` when the id has no trailing line number.
pub fn parse_symbol_id(id: &str) -> (String, usize) {
    match id.rsplit_once(':') {
        Some((file, line)) if !file.is_empty() => match line.parse::<usize>() {
            Ok(n) => (file.to_string(), n),
            Err(_) => (id.to_string(), 0),
        },
        _ => (id.to_string(), 0),
    }
}

/// Most complex functions first, capped at `top` (0 = all).
pub fn hotspots(analysis: &Analysis, top: usize) -> Vec<Hotspot> {
    let _ = (analysis, top);
    Vec::new()
}

/// Per-file roll-up, sorted by total complexity (desc), then file path.
pub fn per_file(analysis: &Analysis) -> Vec<FileComplexity> {
    let _ = analysis;
    Vec::new()
}

/// Variables with `reads == 0` — dead-code candidates (F4/F6 surface).
pub fn unused_variables(analysis: &Analysis) -> Vec<&Variable> {
    let _ = analysis;
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_symbol_id_splits_file_and_line() {
        assert_eq!(
            parse_symbol_id("src/main.rs:12"),
            ("src/main.rs".into(), 12)
        );
        assert_eq!(parse_symbol_id("main"), ("main".into(), 0));
        assert_eq!(parse_symbol_id(""), ("".into(), 0));
    }
}
