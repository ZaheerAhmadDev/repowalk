//! Call-site → symbol resolution with explicit confidence tiers (plan §7.1).
//! OWNER: RSK-5.
use crate::model::{CallSite, Edge, FileEntry, Import, Symbol};

/// Resolve call sites to edges: Exact (same file/module), NameOnly (unique global),
/// Ambiguous (multiple candidates), Unresolved (none). Deterministic.
pub fn resolve(files: &[FileEntry], symbols: &[Symbol], sites: &[CallSite]) -> Vec<Edge> {
    unimplemented!("RSK-5")
}

/// Resolve imports to file-level Import edges where the target is identifiable.
pub fn resolve_imports(files: &[FileEntry], imports: &[Import]) -> Vec<Edge> {
    unimplemented!("RSK-5")
}
