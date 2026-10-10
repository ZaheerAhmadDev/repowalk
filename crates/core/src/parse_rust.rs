//! tree-sitter-rust extraction: defs, doc comments, call sites, use-decls (plan §8.1).
//! OWNER: RSK-3. Produces the same Symbol shapes as parse_py via shared conventions:
//! id = "{path}:{start_line}", start_line = node.start_position.row + 1.
use crate::model::{CallSite, Import, Symbol};

/// Extract symbols, call sites and imports from one Rust source file.
pub fn extract(source: &str, path: &str) -> (Vec<Symbol>, Vec<CallSite>, Vec<Import>) {
    unimplemented!("RSK-3")
}
