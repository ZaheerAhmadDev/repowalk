//! Reference index (F7): every use site of a symbol or variable.
//!
//! OWNER: TEXT-03 (card TEXT-3) for the consuming panel; INTEL for the
//! `UseSite` population. The lookups here are the agreed starting contract.
use crate::analysis::{Analysis, UseKind, UseSite};

/// All use sites of `target` (a symbol id or `Variable::id`), in file/line order.
pub fn references(analysis: &Analysis, target: &str) -> Vec<UseSite> {
    let _ = (analysis, target);
    Vec::new()
}

/// Use sites of `target` narrowed to one kind.
pub fn references_of_kind(analysis: &Analysis, target: &str, kind: UseKind) -> Vec<UseSite> {
    let _ = (analysis, target, kind);
    Vec::new()
}

/// Reference counts per target, highest first (ties broken by target id).
pub fn reference_counts(analysis: &Analysis) -> Vec<(String, usize)> {
    let _ = analysis;
    Vec::new()
}

/// Distinct files a target is used in.
pub fn referencing_files(analysis: &Analysis, target: &str) -> Vec<String> {
    let _ = (analysis, target);
    Vec::new()
}
