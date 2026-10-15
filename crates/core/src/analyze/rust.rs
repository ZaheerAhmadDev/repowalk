//! Rust analysis: variables, control flow, types, references (INTEL-2).
//!
//! Fills the frozen [`crate::analysis`] contract from tree-sitter-rust. Uses the
//! shared helpers in [`super`] (`enclosing_symbol`, `snippet`).
//! Status: interface stub — the INTEL-2 working group replaces this body.
use crate::analysis::Analysis;
use crate::model::Symbol;

/// Append Rust variables/CFG/types/use-sites for `file` into `out`.
pub fn analyze(file: &str, source: &str, symbols: &[Symbol], out: &mut Analysis) {
    let _ = (file, source, symbols, out);
}
