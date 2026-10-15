//! Shared artifact loading for the v2 query commands (PROD-3).
//!
//! The v2 analysis artifact (`.repowalk/analysis.json`) is only ever produced
//! by the CORE-7 `learn` driver, so every analysis-backed command loads through
//! here instead of re-deriving anything:
//!
//! 1. reuse the cached artifact when present (`analysis::load`) — fast, and it
//!    never re-indexes behind the user's back;
//! 2. otherwise run `learn` once to build it (the documented "index first"
//!    step) and return both artifacts.
//!
//! If a repo has analyzable files but the resulting analysis is empty, that is
//! surfaced as a clear "no analysis yet" error rather than silently printing
//! empty tables as if they were results (CORE-7 regression guard).
use anyhow::{anyhow, bail};
use repowalk_core::analysis::{self, Analysis};
use repowalk_core::model::Index;
use repowalk_core::{analyze, learn, pipeline};
use std::path::{Path, PathBuf};

/// The frozen index plus the v2 analysis for one repo root.
pub struct Artifacts {
    pub index: Index,
    pub analysis: Analysis,
    /// The repo root the artifacts describe (kept for error messages / paths).
    pub root: PathBuf,
}

/// Load `(index, analysis)` for `root`, building the analysis when absent.
pub fn load(root: &Path) -> anyhow::Result<Artifacts> {
    if !root.is_dir() {
        bail!("repo path not found or not a directory: {}", root.display());
    }
    if let Some(analysis) = analysis::load(root) {
        let index = pipeline::build_index(root, &|_| {})?;
        return Ok(Artifacts {
            index,
            analysis,
            root: root.to_path_buf(),
        });
    }

    let (index, analysis) = learn::learn(root, &|_| {})?;
    if analysis_is_empty(&analysis) && analyzable_files(&index) > 0 {
        return Err(anyhow!(
            "no analysis yet for {}: analyzers produced no variables/CFGs/uses \
             for {} analyzable file(s); index the repo first (e.g. `repowalk map {}`)",
            root.display(),
            analyzable_files(&index),
            root.display()
        ));
    }
    Ok(Artifacts {
        index,
        analysis,
        root: root.to_path_buf(),
    })
}

fn analysis_is_empty(a: &Analysis) -> bool {
    a.variables.is_empty() && a.cfgs.is_empty() && a.types.is_empty() && a.uses.is_empty()
}

fn analyzable_files(index: &Index) -> usize {
    index
        .files
        .iter()
        .filter(|f| analyze::Lang::from_walk(&f.lang).is_some())
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(tag: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("rw-cli-art-{}-{}", std::process::id(), tag));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn load_builds_and_caches_the_analysis() {
        let root = temp_root("basic");
        std::fs::write(
            root.join("main.rs"),
            "fn main() { helper(); }\nfn helper() {}\n",
        )
        .unwrap();

        let first = load(&root).unwrap();
        assert!(
            !first.analysis.cfgs.is_empty() || !first.analysis.variables.is_empty(),
            "learn should have produced analysis data"
        );
        assert!(analysis::analysis_path(&root).exists(), "artifact written");

        // Second load takes the cache path and yields the same data.
        let second = load(&root).unwrap();
        assert_eq!(first.analysis, second.analysis);

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn missing_root_is_err_not_panic() {
        let root = temp_root("missing").join("nope");
        assert!(load(&root).is_err());
    }
}
