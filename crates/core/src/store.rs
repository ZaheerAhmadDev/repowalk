//! `.repowalk/index.json` cache: read/write + per-file-stamp invalidation (plan
//! §7.6). OWNER: CORE-7 (v2); R-14 hardening by CORE-8.
//!
//! # Cache validity (R-14)
//!
//! `index.json` (the frozen [`Index`]) is paired with a sibling stamp sidecar,
//! `.repowalk/cache-header.json`, that records each input file's **nanosecond
//! mtime + byte size**. Validity is decided **per file**, never by a global
//! `max(mtime)` at whole-second granularity, because the old comparison missed:
//!
//! * a same-second edit (whole-second mtimes compare equal),
//! * a `git checkout` / `touch -r` that preserves mtime (edit with a different
//!   size but an identical timestamp), and
//! * an edit of any file that is not the newest one (the global max is unchanged).
//!
//! A single mismatch, addition or removal is a miss. The sidecar lives beside
//! `index.json` (not inside it) so the frozen `Index` / `index.json` schema is
//! untouched; it can be rewritten or discarded without touching the artifact
//! that GUI/CLI consumers read. `walk::mtime_secs` is *not* used here.
use crate::model::{FileEntry, Index, SCHEMA_VERSION};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Schema of the stamp sidecar. Independent of `SCHEMA_VERSION`: it is
/// pipeline-internal bookkeeping, not part of the frozen `index.json` contract.
const CACHE_HEADER_VERSION: u32 = 1;

/// `<root>/.repowalk/index.json`
pub fn cache_path(root: &Path) -> PathBuf {
    root.join(".repowalk").join("index.json")
}

/// `<root>/.repowalk/cache-header.json` — the per-file input stamps.
pub fn header_path(root: &Path) -> PathBuf {
    root.join(".repowalk").join("cache-header.json")
}

/// One file's cache stamp: nanosecond mtime + byte size.
///
/// A stat failure (missing file, unreadable path) records `(0, 0)`; such a
/// stamp only ever matches another stat failure, so a real file that vanishes
/// after the cache was written is a miss (its recorded stamp is non-zero).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct FileStamp {
    path: String,
    mtime_nanos: u64,
    size: u64,
}

/// The `.repowalk/cache-header.json` payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct CacheHeader {
    version: u32,
    files: Vec<FileStamp>,
}

/// Compute the current stamp for one repo-relative `path`.
fn stamp_of(root: &Path, path: &str) -> FileStamp {
    let full = root.join(path);
    FileStamp {
        path: path.to_string(),
        mtime_nanos: crate::walk::mtime_nanos(&full),
        size: crate::walk::file_size(&full),
    }
}

/// Load cache if present and parseable, else None.
pub fn load(root: &Path) -> Option<Index> {
    let data = std::fs::read(cache_path(root)).ok()?;
    serde_json::from_slice(&data).ok()
}

/// Atomic write (tmp + rename). Creates `.repowalk/`.
///
/// Writes the frozen `index.json` **and** the per-file stamp sidecar derived
/// from the index's file list, so the two always describe the same snapshot.
pub fn save(root: &Path, idx: &Index) -> anyhow::Result<()> {
    let dir = root.join(".repowalk");
    std::fs::create_dir_all(&dir)?;

    let dest = dir.join("index.json");
    let tmp = dir.join("index.json.tmp");
    std::fs::write(&tmp, serde_json::to_vec(idx)?)?;
    std::fs::rename(&tmp, &dest)?;

    let header = CacheHeader {
        version: CACHE_HEADER_VERSION,
        files: idx.files.iter().map(|f| stamp_of(root, &f.path)).collect(),
    };
    let hdest = header_path(root);
    let htmp = dir.join("cache-header.json.tmp");
    std::fs::write(&htmp, serde_json::to_vec(&header)?)?;
    std::fs::rename(&htmp, &hdest)?;
    Ok(())
}

fn load_header(root: &Path) -> Option<CacheHeader> {
    let data = std::fs::read(header_path(root)).ok()?;
    let header: CacheHeader = serde_json::from_slice(&data).ok()?;
    (header.version == CACHE_HEADER_VERSION).then_some(header)
}

/// Valid iff the cache **and** its stamp sidecar exist, versions match, and
/// every recorded file still exists with an identical per-file stamp.
///
/// Any addition, removal, or per-file mtime(ns)/size change is a miss. There
/// is deliberately no global-max or whole-second comparison (R-14).
pub fn cache_valid(root: &Path, files: &[FileEntry]) -> bool {
    let Some(idx) = load(root) else {
        return false;
    };
    if idx.version != SCHEMA_VERSION {
        return false;
    }
    let Some(header) = load_header(root) else {
        return false;
    };

    // The current file set must have exactly the recorded cardinality, and every
    // path must match by stamp — compared as a set so ordering cannot matter.
    if files.len() != header.files.len() {
        return false;
    }
    let mut current: Vec<FileStamp> = files.iter().map(|f| stamp_of(root, &f.path)).collect();
    current.sort_by(|a, b| a.path.cmp(&b.path));
    let mut recorded = header.files;
    recorded.sort_by(|a, b| a.path.cmp(&b.path));
    current == recorded
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::SymbolKind;
    use std::fs::FileTimes;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    fn temp_root(tag: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("rw-store-test-{}-{}", std::process::id(), tag));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    fn file_entry(path: &str, mtime: u64) -> FileEntry {
        FileEntry {
            path: path.into(),
            lang: "rust".into(),
            lines: 10,
            mtime,
            is_entrypoint: false,
        }
    }

    fn sample_index(root: &Path) -> Index {
        let mut idx = Index::new(root.to_string_lossy());
        idx.files.push(file_entry("src/main.rs", 1000));
        idx.symbols.push(crate::model::Symbol {
            id: "src/main.rs:1".into(),
            name: "main".into(),
            kind: SymbolKind::Function,
            signature: "fn main()".into(),
            doc: String::new(),
            start_line: 1,
            end_line: 3,
            container: None,
            file: "src/main.rs".into(),
        });
        idx
    }

    /// The `FileEntry` list `cache_valid` sees, keyed only by path (it stats the
    /// real files itself, so mtime/lines here are irrelevant).
    fn entries(paths: &[&str]) -> Vec<FileEntry> {
        paths.iter().map(|p| file_entry(p, 0)).collect()
    }

    fn base_time() -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(1_600_000_000)
    }

    /// Set a file's mtime (creates/open the handle for writing).
    fn set_mtime(path: &Path, t: SystemTime) {
        let f = std::fs::OpenOptions::new().write(true).open(path).unwrap();
        f.set_times(FileTimes::new().set_modified(t)).unwrap();
    }

    /// Cache an index whose file list is `paths`, then return the index.
    fn save_for(root: &Path, paths: &[&str]) -> Index {
        let mut idx = Index::new(root.to_string_lossy());
        idx.files = entries(paths);
        save(root, &idx).unwrap();
        idx
    }

    #[test]
    fn cache_path_is_repowalk_index_json() {
        let root = Path::new("/repo");
        assert_eq!(
            cache_path(root),
            PathBuf::from("/repo/.repowalk/index.json")
        );
        assert_eq!(
            header_path(root),
            PathBuf::from("/repo/.repowalk/cache-header.json")
        );
    }

    #[test]
    fn roundtrip_save_load() {
        let root = temp_root("roundtrip");
        let idx = sample_index(&root);
        save(&root, &idx).unwrap();

        let loaded = load(&root).expect("cache should load");
        assert_eq!(loaded.version, idx.version);
        assert_eq!(loaded.repo_root, idx.repo_root);
        assert_eq!(loaded.files.len(), 1);
        assert_eq!(loaded.files[0].path, "src/main.rs");
        assert_eq!(loaded.files[0].mtime, 1000);
        assert_eq!(loaded.symbols.len(), 1);
        assert_eq!(loaded.symbols[0].name, "main");
        // atomic writes leave no tmp behind (index + stamp sidecar)
        assert!(!root.join(".repowalk").join("index.json.tmp").exists());
        assert!(!root
            .join(".repowalk")
            .join("cache-header.json.tmp")
            .exists());
        assert!(header_path(&root).exists(), "save writes the stamp sidecar");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn load_returns_none_when_missing_or_corrupt() {
        let root = temp_root("load-none");
        assert!(load(&root).is_none());

        std::fs::create_dir_all(root.join(".repowalk")).unwrap();
        std::fs::write(cache_path(&root), b"{not json").unwrap();
        assert!(load(&root).is_none());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn cache_valid_true_for_unchanged_files() {
        let root = temp_root("valid");
        std::fs::write(root.join("a.rs"), "fn a() {}\n").unwrap();
        std::fs::write(root.join("b.rs"), "fn b() {}\n").unwrap();
        let files = entries(&["a.rs", "b.rs"]);
        save_for(&root, &["a.rs", "b.rs"]);

        assert!(cache_valid(&root, &files));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn cache_invalidated_after_touched_file() {
        let root = temp_root("touched");
        std::fs::write(root.join("a.rs"), "fn a() {}\n").unwrap();
        let files = entries(&["a.rs"]);
        save_for(&root, &["a.rs"]);
        assert!(cache_valid(&root, &files));

        std::fs::write(root.join("a.rs"), "fn a() { /* touched */ }\n").unwrap();
        assert!(
            !cache_valid(&root, &files),
            "a real rewrite must invalidate the cache"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn cache_invalidated_on_version_mismatch_or_missing() {
        let root = temp_root("version");
        let files = entries(&["a.rs"]);
        // no cache yet
        assert!(!cache_valid(&root, &files));

        let mut idx = sample_index(&root);
        idx.files = files.clone();
        idx.version = SCHEMA_VERSION + 1;
        save(&root, &idx).unwrap();
        assert!(!cache_valid(&root, &files));

        // a missing stamp sidecar is a miss even with a matching index version
        let mut ok = sample_index(&root);
        ok.files = files.clone();
        save(&root, &ok).unwrap();
        std::fs::remove_file(header_path(&root)).unwrap();
        assert!(!cache_valid(&root, &files));
        let _ = std::fs::remove_dir_all(&root);
    }

    // ---- R-14 regressions: the old max(mtime)-at-second-granularity misses ----

    /// Two writes inside the same whole second: the seconds mtime is unchanged,
    /// so the old `max(mtime)` comparison was a HIT. Nanosecond mtime differs.
    #[test]
    fn same_second_edit_invalidates() {
        let root = temp_root("same-second");
        let file = root.join("a.ts");
        std::fs::write(&file, "export const a = 1;\n").unwrap();
        let t0 = std::fs::metadata(&file).unwrap().modified().unwrap();
        let files = entries(&["a.ts"]);
        save_for(&root, &["a.ts"]);
        assert!(cache_valid(&root, &files), "baseline must be a HIT");

        // Same byte length, same whole second, but +1ns elapsed.
        std::fs::write(&file, "export const b = 9;\n").unwrap();
        set_mtime(&file, t0 + Duration::from_nanos(1));
        let now = std::fs::metadata(&file).unwrap().modified().unwrap();
        assert_eq!(
            now.duration_since(t0).unwrap(),
            Duration::from_nanos(1),
            "the two writes differ by <1s, so they share a whole second \
             (the old max(mtime) HIT condition)"
        );
        assert!(
            !cache_valid(&root, &files),
            "a same-second edit must invalidate (old code returned a HIT)"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A size change with the mtime preserved exactly (`touch -r` / `git
    /// checkout`): the old comparison saw identical mtimes and was a HIT.
    #[test]
    fn size_change_with_preserved_mtime_invalidates() {
        let root = temp_root("preserved-mtime");
        let file = root.join("a.rs");
        std::fs::write(&file, "fn a() {}\n").unwrap();
        let t0 = std::fs::metadata(&file).unwrap().modified().unwrap();
        let files = entries(&["a.rs"]);
        save_for(&root, &["a.rs"]);
        assert!(cache_valid(&root, &files), "baseline must be a HIT");

        std::fs::write(&file, "fn a() {}\n// a longer body\n").unwrap();
        set_mtime(&file, t0); // preserve the stamp
        assert_eq!(
            std::fs::metadata(&file).unwrap().modified().unwrap(),
            t0,
            "mtime must be byte-for-byte preserved"
        );
        assert!(
            !cache_valid(&root, &files),
            "a preserved-mtime size change must invalidate (old code returned a HIT)"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// An edit of a file that is *not* the newest one: the global max mtime is
    /// unchanged, so the old comparison was a HIT.
    #[test]
    fn edit_of_non_newest_file_invalidates() {
        let root = temp_root("non-newest");
        let older = root.join("older.rs");
        let newest = root.join("newest.rs");
        std::fs::write(&older, "fn older() {}\n").unwrap();
        std::fs::write(&newest, "fn newest() {}\n").unwrap();
        set_mtime(&older, base_time());
        set_mtime(&newest, base_time() + Duration::from_secs(1000));

        let files = entries(&["newest.rs", "older.rs"]);
        save_for(&root, &["newest.rs", "older.rs"]);
        assert!(cache_valid(&root, &files), "baseline must be a HIT");

        // Edit the OLDER file (new size) but keep its mtime below the newest.
        std::fs::write(&older, "fn older() {}\n// edited\n").unwrap();
        set_mtime(&older, base_time() + Duration::from_secs(1));
        assert!(
            !cache_valid(&root, &files),
            "editing a non-newest file must invalidate (old code returned a HIT)"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// An added or removed file is a miss (set cardinality / membership).
    #[test]
    fn added_or_removed_file_invalidates() {
        let root = temp_root("set-change");
        std::fs::write(root.join("a.rs"), "fn a() {}\n").unwrap();
        std::fs::write(root.join("b.rs"), "fn b() {}\n").unwrap();
        save_for(&root, &["a.rs"]);
        assert!(cache_valid(&root, &entries(&["a.rs"])));

        assert!(
            !cache_valid(&root, &entries(&["a.rs", "b.rs"])),
            "an added file must invalidate"
        );
        assert!(
            !cache_valid(&root, &entries(&[])),
            "a removed file must invalidate"
        );
        let _ = std::fs::remove_dir_all(&root);
    }
}
