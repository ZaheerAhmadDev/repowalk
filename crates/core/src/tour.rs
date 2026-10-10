//! Reading-order tour (plan §7.2): topological layers of import DAG,
//! score = fan_in + 0.8*entrypoint + 0.5*docs - 0.3*depth, SCCs collapsed.
//! OWNER: RSK-7.
use crate::model::Index;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TourItem {
    pub path: String,
    pub why: String, // docstring first sentence, else "fn {name} — called by N, calls M"
}

/// Top `limit` items in recommended reading order (deterministic).
pub fn tour(idx: &Index, limit: usize) -> Vec<TourItem> {
    unimplemented!("RSK-7")
}
