//! Repository walk: gitignore-aware traversal, skip noise dirs, LOC + entrypoint flags.
//! OWNER: RSK-1. Signature is contract — callers: index pipeline, tests.
use crate::model::FileEntry;
use std::path::Path;

/// Walk `root` (honors .gitignore; skips .git, target, node_modules, .venv).
/// Fills path, lang (by extension), lines, mtime, is_entrypoint (main.rs/lib.rs/fn main/__main__).
pub fn walk(root: &Path) -> anyhow::Result<Vec<FileEntry>> {
    unimplemented!("RSK-1")
}
