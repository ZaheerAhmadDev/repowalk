//! TypeScript/JavaScript analysis: variables, control flow, types, references (INTEL-4).
//!
//! Fills the frozen [`crate::analysis`] contract from tree-sitter-typescript /
//! tree-sitter-javascript. Uses the shared helpers in [`super`]
//! (`enclosing_symbol`, `snippet`).
//!
//! Status: interface stub. The INTEL-4 working group implements this AFTER BLD-2
//! has pinned the grammar crates in `crates/core/Cargo.toml` (until then the
//! `tree_sitter_typescript` import will not resolve).
use crate::analysis::Analysis;
use crate::model::Symbol;

/// Append TS/JS variables/CFG/types/use-sites for `file` into `out`.
pub fn analyze(file: &str, source: &str, symbols: &[Symbol], out: &mut Analysis) {
    let _ = (file, source, symbols, out);
}
