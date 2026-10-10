//! Fuzzy subsequence search over symbol names + file paths (plan §7.5).
//! OWNER: RSK-8. Deterministic fzf-style scoring:
/// +4 exact substring in name, +2 subsequence, +1 consecutive/word-boundary, +0.5 doc/path hit.
use crate::model::Index;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hit {
    pub target: String, // symbol id or file path
    pub path: String,
    pub line: usize,
    pub score: f64,
}

/// Top `limit` hits for `query`, sorted score desc (ties broken by target asc).
pub fn search(idx: &Index, query: &str, limit: usize) -> Vec<Hit> {
    unimplemented!("RSK-8")
}
