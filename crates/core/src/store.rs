//! `.repowalk/index.json` cache: read/write + mtime-based invalidation (plan §7.6).
//! OWNER: RSK-6.
use crate::model::{FileEntry, Index, SCHEMA_VERSION};
use std::path::{Path, PathBuf};

/// `<root>/.repowalk/index.json`
pub fn cache_path(root: &Path) -> PathBuf {
    root.join(".repowalk").join("index.json")
}

/// Load cache if present and parseable, else None.
pub fn load(root: &Path) -> Option<Index> {
    let data = std::fs::read(cache_path(root)).ok()?;
    serde_json::from_slice(&data).ok()
}

/// Atomic write (tmp + rename). Creates `.repowalk/`.
pub fn save(root: &Path, idx: &Index) -> anyhow::Result<()> {
    let dir = root.join(".repowalk");
    std::fs::create_dir_all(&dir)?;
    let dest = dir.join("index.json");
    let tmp = dir.join("index.json.tmp");
    std::fs::write(&tmp, serde_json::to_vec(idx)?)?;
    std::fs::rename(&tmp, &dest)?;
    Ok(())
}

/// Valid iff cache exists, version matches, and max(mtime of files) unchanged.
pub fn cache_valid(root: &Path, files: &[FileEntry]) -> bool {
    let Some(idx) = load(root) else {
        return false;
    };
    if idx.version != SCHEMA_VERSION {
        return false;
    }
    let cached_max = idx.files.iter().map(|f| f.mtime).max().unwrap_or(0);
    let current_max = files.iter().map(|f| f.mtime).max().unwrap_or(0);
    cached_max == current_max
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::SymbolKind;

    fn temp_root(tag: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!(
            "rw-store-test-{}-{}",
            std::process::id(),
            tag
        ));
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

    #[test]
    fn cache_path_is_repowalk_index_json() {
        let root = Path::new("/repo");
        assert_eq!(cache_path(root), PathBuf::from("/repo/.repowalk/index.json"));
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
        // atomic write leaves no tmp behind
        assert!(!root.join(".repowalk").join("index.json.tmp").exists());
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
        let files = vec![file_entry("src/a.rs", 2000), file_entry("src/b.rs", 3000)];
        let mut idx = sample_index(&root);
        idx.files = files.clone();
        save(&root, &idx).unwrap();

        assert!(cache_valid(&root, &files));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn cache_invalidated_after_touched_file() {
        let root = temp_root("touched");
        let files = vec![file_entry("src/a.rs", 2000)];
        let mut idx = sample_index(&root);
        idx.files = files.clone();
        save(&root, &idx).unwrap();
        assert!(cache_valid(&root, &files));

        // simulate touching a file: new mtime > cached max
        let touched = vec![file_entry("src/a.rs", 2001)];
        assert!(!cache_valid(&root, &touched));

        // real file touch via rewrite bumps mtime past the saved max
        let real = temp_root("touched-real");
        let fpath = real.join("a.rs");
        std::fs::write(&fpath, "fn a() {}").unwrap();
        let mtime = std::fs::metadata(&fpath)
            .unwrap()
            .modified()
            .unwrap()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let real_files = vec![FileEntry {
            path: "a.rs".into(),
            lang: "rust".into(),
            lines: 1,
            mtime,
            is_entrypoint: false,
        }];
        let mut ridx = sample_index(&real);
        ridx.files = real_files.clone();
        save(&real, &ridx).unwrap();
        assert!(cache_valid(&real, &real_files));

        std::thread::sleep(std::time::Duration::from_millis(1100));
        std::fs::write(&fpath, "fn a() { /* touched */ }").unwrap();
        let new_mtime = std::fs::metadata(&fpath)
            .unwrap()
            .modified()
            .unwrap()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let after = vec![FileEntry {
            path: "a.rs".into(),
            lang: "rust".into(),
            lines: 1,
            mtime: new_mtime,
            is_entrypoint: false,
        }];
        assert!(!cache_valid(&real, &after));

        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_dir_all(&real);
    }

    #[test]
    fn cache_invalidated_on_version_mismatch_or_missing() {
        let root = temp_root("version");
        let files = vec![file_entry("src/a.rs", 2000)];
        // no cache yet
        assert!(!cache_valid(&root, &files));

        let mut idx = sample_index(&root);
        idx.files = files.clone();
        idx.version = SCHEMA_VERSION + 1;
        save(&root, &idx).unwrap();
        assert!(!cache_valid(&root, &files));
        let _ = std::fs::remove_dir_all(&root);
    }
}
