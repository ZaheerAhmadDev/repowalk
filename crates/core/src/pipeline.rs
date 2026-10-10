//! Full index pipeline: walk → parse by lang → resolve → timestamp (+ store cache).
//! OWNER: RSK-9. This is what CLI and GUI call.
use crate::model::Index;
use std::path::Path;

/// Build (or load from valid cache) the index for `root`.
/// `progress(files_done)` is called during walks/parses for UI overlays.
pub fn build_index(root: &Path, progress: &dyn Fn(usize)) -> anyhow::Result<Index> {
    unimplemented!("RSK-9")
}
