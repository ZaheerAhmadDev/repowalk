//! Full index pipeline: walk → parse by lang → resolve → timestamp (+ store cache).
//! OWNER: CORE-7 (v2). This is what CLI and GUI call.
use crate::model::{FileEntry, Index};
use std::path::Path;

/// Build (or load from valid cache) the index for `root`.
/// `progress(files_done)` is called during walks/parses for UI overlays.
pub fn build_index(root: &Path, progress: &dyn Fn(usize)) -> anyhow::Result<Index> {
    let files = crate::walk::walk(root)?;
    build_index_from_files(root, files, &|phase, done, _total| {
        if phase == "parse" {
            progress(done);
        }
    })
}

/// Build the index from an already-walked file list, reporting
/// `progress(phase, done, total)` for the `parse` and `resolve` phases.
///
/// Split out of [`build_index`] so the v2 `learn` driver can reuse the walk it
/// already needed for its incremental stamps (F10) instead of walking twice,
/// while keeping all resolve/timestamp/cache logic in one place.
pub(crate) fn build_index_from_files(
    root: &Path,
    files: Vec<FileEntry>,
    progress: &dyn Fn(&'static str, usize, usize),
) -> anyhow::Result<Index> {
    let total = files.len();
    if crate::store::cache_valid(root, &files) {
        progress("parse", total, total);
        progress("resolve", total, total);
        return Ok(crate::store::load(root).expect("cache_valid implies a loadable cache"));
    }

    let mut idx = Index::new(root.to_string_lossy());
    idx.files = files;
    progress("parse", 0, total);
    for (done, file) in idx.files.iter().enumerate() {
        let text = std::fs::read_to_string(root.join(&file.path))?;
        let (symbols, sites, imports) = match file.lang.as_str() {
            "rust" => crate::parse_rust::extract(&text, &file.path),
            "python" => crate::parse_py::extract(&text, &file.path),
            _ => (Vec::new(), Vec::new(), Vec::new()),
        };
        idx.symbols.extend(symbols);
        idx.call_sites.extend(sites);
        idx.imports.extend(imports);
        progress("parse", done + 1, total);
    }

    let mut edges = crate::resolve::resolve(&idx.files, &idx.symbols, &idx.call_sites);
    edges.extend(crate::resolve::resolve_imports(&idx.files, &idx.imports));
    idx.edges = edges;
    idx.indexed_at = timestamp_now_utc();
    crate::store::save(root, &idx)?;
    progress("resolve", total, total);
    Ok(idx)
}

/// RFC 3339 UTC timestamp, hand-rolled (no chrono dep — pinned stack only).
fn timestamp_now_utc() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let (y, m, d) = civil_from_days((secs / 86_400) as i64);
    let rem = secs % 86_400;
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        (rem % 3600) / 60,
        rem % 60
    )
}

/// Howard Hinnant's civil-from-days: days since 1970-01-01 → (y, m, d).
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{EdgeKind, SymbolKind};
    use std::path::PathBuf;

    fn temp_root(tag: &str) -> PathBuf {
        let p =
            std::env::temp_dir().join(format!("rw-pipeline-test-{}-{}", std::process::id(), tag));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    fn write_sample(root: &Path) {
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(
            root.join("src/main.rs"),
            "//! entry\nfn main() {\n    helper();\n}\n\npub fn helper() {}\n",
        )
        .unwrap();
        std::fs::write(root.join("src/tool.py"), "def run():\n    pass\n").unwrap();
    }

    #[test]
    fn builds_index_with_files_symbols_and_edges() {
        let root = temp_root("build");
        write_sample(&root);

        let idx = build_index(&root, &|_| {}).unwrap();

        let paths: Vec<&str> = idx.files.iter().map(|f| f.path.as_str()).collect();
        assert!(paths.contains(&"src/main.rs"));
        assert!(paths.contains(&"src/tool.py"));
        assert_eq!(idx.version, crate::model::SCHEMA_VERSION);
        assert_eq!(idx.repo_root, root.to_string_lossy());

        let names: Vec<&str> = idx.symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"main"));
        assert!(names.contains(&"helper"));
        assert!(names.contains(&"run"));
        assert!(idx
            .symbols
            .iter()
            .any(|s| s.kind == SymbolKind::Function && s.file == "src/main.rs"));

        // helper is called from main: a resolved call edge to its symbol id must exist
        let helper = idx.symbols.iter().find(|s| s.name == "helper").unwrap();
        assert!(idx
            .edges
            .iter()
            .any(|e| e.kind == EdgeKind::Call && e.callee == helper.id));

        // cache was saved
        assert!(crate::store::load(&root).is_some());

        assert!(!idx.indexed_at.is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn loads_cache_when_valid_instead_of_rebuilding() {
        let root = temp_root("cache");
        write_sample(&root);
        let idx = build_index(&root, &|_| {}).unwrap();
        assert!(!idx.symbols.is_empty());

        // Tamper with the cache: if the pipeline re-parsed, symbols would come back.
        let mut cached = crate::store::load(&root).unwrap();
        cached.symbols.clear();
        cached.edges.clear();
        crate::store::save(&root, &cached).unwrap();

        let again = build_index(&root, &|_| {}).unwrap();
        assert!(
            again.symbols.is_empty(),
            "must load the valid cache, not rebuild"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn rebuilds_after_file_touch() {
        let root = temp_root("touch");
        write_sample(&root);
        let first = build_index(&root, &|_| {}).unwrap();
        assert!(!first.symbols.is_empty());

        // Touch one source file: mtime max changes → cache invalidated → re-parse.
        std::thread::sleep(std::time::Duration::from_millis(1100));
        std::fs::write(root.join("src/main.rs"), "fn main() {}\n").unwrap();

        let second = build_index(&root, &|_| {}).unwrap();
        let names: Vec<&str> = second.symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"main"));
        assert!(
            !names.contains(&"helper"),
            "stale symbol must be gone after re-index"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn progress_reports_monotonic_counts_ending_at_file_total() {
        let root = temp_root("progress");
        write_sample(&root);

        let seen = std::cell::RefCell::new(Vec::new());
        build_index(&root, &|n| seen.borrow_mut().push(n)).unwrap();
        let seen = seen.into_inner();

        assert!(!seen.is_empty(), "progress must be called at least once");
        assert!(
            seen.windows(2).all(|w| w[0] <= w[1]),
            "counts must be monotonic"
        );
        assert_eq!(*seen.last().unwrap(), 2, "final count == number of files");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn missing_root_is_an_error() {
        let root = temp_root("missing-root").join("does-not-exist");
        assert!(build_index(&root, &|_| {}).is_err());
    }
}
