//! tree-sitter-python extraction: defs (incl. async/class), docstrings,
//! call sites, imports (plan §8.1).
//! OWNER: RSK-4. Same Symbol conventions as parse_rust (id = "{path}:{start_line}").
use crate::model::{CallSite, Import, Symbol};

/// Extract symbols, call sites and imports from one Python source file.
pub fn extract(source: &str, path: &str) -> (Vec<Symbol>, Vec<CallSite>, Vec<Import>) {
    unimplemented!("RSK-4")
}
