//! Python analysis: variables, control flow, types, references (INTEL-3).
//!
//! Fills the frozen [`crate::analysis`] contract from tree-sitter-python. Uses
//! the shared helpers in [`super`] (`enclosing_symbol`, `snippet`).
//! Status: interface stub — the INTEL-3 working group replaces this body.
use crate::analysis::Analysis;
use crate::model::Symbol;

/// Append Python variables/CFG/types/use-sites for `file` into `out`.
pub fn analyze(file: &str, source: &str, symbols: &[Symbol], out: &mut Analysis) {
    let _ = (file, source, symbols, out);
}
