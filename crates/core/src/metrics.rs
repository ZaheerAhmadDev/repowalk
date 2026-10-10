//! Derived repo metrics: LOC, languages, entrypoints, top files, fan-in,
//! SCC cycles, god-files, unresolved-edge % (plan §7.4).
//! OWNER: RSK-2.
use crate::model::Index;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Metrics {
    pub total_loc: usize,
    pub langs: Vec<(String, usize)>,   // (lang, file count) sorted desc
    pub entrypoints: Vec<String>,
    pub top_files: Vec<(String, usize)>, // (path, lines) top 10
    pub fan_in: Vec<(String, usize)>,    // (symbol id, callers) sorted desc
    pub cycles: Vec<Vec<String>>,        // SCCs with len > 1 (symbol ids)
    pub god_files: Vec<String>,          // lines > max(500, p99)
    pub unresolved_pct: f64,             // confidence in {Ambiguous, Unresolved} / total edges
}

/// Compute all metrics from a finished index (deterministic).
pub fn compute(idx: &Index) -> Metrics {
    unimplemented!("RSK-2")
}
