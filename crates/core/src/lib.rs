//! repowalk core — indexing, model, and algorithms. No GUI/CLI deps.
pub mod analysis;
pub mod analyze;
pub mod flow;
pub mod metrics;
pub mod model;
pub mod parse_py;
pub mod parse_rust;
pub mod pipeline;
pub mod resolve;
pub mod search;
pub mod store;
pub mod tour;
pub mod walk;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
