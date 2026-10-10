//! `.repowalk/index.json` cache: read/write + mtime-based invalidation (plan §7.6).
//! OWNER: RSK-6.
use crate::model::{FileEntry, Index};
use std::path::{Path, PathBuf};

/// `<root>/.repowalk/index.json`
pub fn cache_path(root: &Path) -> PathBuf {
    unimplemented!("RSK-6")
}

/// Load cache if present and parseable, else None.
pub fn load(root: &Path) -> Option<Index> {
    unimplemented!("RSK-6")
}

/// Atomic write (tmp + rename). Creates `.repowalk/`.
pub fn save(root: &Path, idx: &Index) -> anyhow::Result<()> {
    unimplemented!("RSK-6")
}

/// Valid iff cache exists, version matches, and max(mtime of files) unchanged.
pub fn cache_valid(root: &Path, files: &[FileEntry]) -> bool {
    unimplemented!("RSK-6")
}
