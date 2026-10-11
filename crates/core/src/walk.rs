//! Repository walk: gitignore-aware traversal, skip noise dirs, LOC + entrypoint flags.
//! OWNER: RSK-1. Signature is contract — callers: index pipeline, tests.
use crate::model::FileEntry;
use ignore::WalkBuilder;
use std::ffi::OsStr;
use std::path::Path;

const SKIP_DIRS: [&str; 5] = [".git", ".repowalk", "target", "node_modules", ".venv"];

/// Walk `root` (honors .gitignore; skips .git, target, node_modules, .venv).
/// Fills path, lang (by extension), lines, mtime, is_entrypoint (main.rs/lib.rs/fn main/__main__).
pub fn walk(root: &Path) -> anyhow::Result<Vec<FileEntry>> {
    let walker = WalkBuilder::new(root)
        .hidden(false)
        .require_git(false)
        .filter_entry(|e| e.depth() == 0 || !is_noise(e.file_name()))
        .build();
    let mut entries = Vec::new();
    for result in walker {
        let entry = result?;
        if !entry.file_type().is_some_and(|t| t.is_file()) {
            continue;
        }
        let path = entry.path();
        let rel = path.strip_prefix(root)?;
        let bytes = std::fs::read(path)?;
        let Ok(text) = String::from_utf8(bytes) else {
            continue;
        };
        entries.push(FileEntry {
            path: rel.to_string_lossy().replace('\\', "/"),
            lang: lang_of(path),
            lines: text.lines().count(),
            mtime: mtime_secs(path),
            is_entrypoint: is_entrypoint(rel, &text),
        });
    }
    entries.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(entries)
}

fn is_noise(name: &OsStr) -> bool {
    name.to_str().is_some_and(|n| SKIP_DIRS.contains(&n))
}

fn lang_of(path: &Path) -> String {
    let Some(ext) = path.extension().and_then(OsStr::to_str) else {
        return "other".to_string();
    };
    let ext = ext.to_ascii_lowercase();
    match ext.as_str() {
        "rs" => "rust".to_string(),
        "py" | "pyi" => "python".to_string(),
        "md" => "markdown".to_string(),
        "toml" => "toml".to_string(),
        "json" => "json".to_string(),
        "yml" | "yaml" => "yaml".to_string(),
        "sh" | "bash" => "shell".to_string(),
        "js" | "mjs" | "cjs" => "javascript".to_string(),
        "ts" | "tsx" => "typescript".to_string(),
        "html" => "html".to_string(),
        "css" => "css".to_string(),
        "c" | "h" => "c".to_string(),
        "cpp" | "cxx" | "cc" | "hpp" => "cpp".to_string(),
        _ => ext,
    }
}

fn mtime_secs(path: &Path) -> u64 {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn is_entrypoint(rel: &Path, text: &str) -> bool {
    let name = rel.file_name().and_then(OsStr::to_str).unwrap_or("");
    name == "main.rs" || name == "lib.rs" || name == "__main__.py"
        || text.contains("fn main")
        || text.contains("__main__")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    fn fixture(name: &str) -> PathBuf {
        let mut dir = std::env::temp_dir();
        dir.push(format!("repowalk-walk-{}-{}", std::process::id(), name));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn paths(entries: &[FileEntry]) -> Vec<&str> {
        entries.iter().map(|e| e.path.as_str()).collect()
    }

    #[test]
    fn skips_noise_dirs_and_honors_gitignore() {
        let root = fixture("gitignore");
        for dir in [
            "src",
            "target",
            "node_modules/pkg",
            ".venv",
            ".git",
            "ignored",
        ] {
            fs::create_dir_all(root.join(dir)).unwrap();
        }
        fs::write(root.join(".gitignore"), "ignored/\n*.log\n").unwrap();
        fs::write(root.join("src/main.rs"), "fn main() {}\n").unwrap();
        fs::write(root.join("target/junk.rs"), "fn x() {}\n").unwrap();
        fs::write(root.join("node_modules/pkg/dep.js"), "let x = 1;\n").unwrap();
        fs::write(root.join(".venv/mod.py"), "x = 1\n").unwrap();
        fs::write(root.join(".git/config"), "core = true\n").unwrap();
        fs::write(root.join("ignored/secret.rs"), "fn s() {}\n").unwrap();
        fs::write(root.join("debug.log"), "noise\n").unwrap();

        let entries = walk(&root).unwrap();
        assert_eq!(paths(&entries), vec![".gitignore", "src/main.rs"]);

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn fills_lang_lines_mtime_and_entrypoints() {
        let root = fixture("fields");
        fs::create_dir_all(root.join("src")).unwrap();
        fs::create_dir_all(root.join("tools")).unwrap();
        fs::write(root.join("src/main.rs"), "fn main() {\n    run();\n}\n").unwrap();
        fs::write(
            root.join("src/lib.rs"),
            "pub fn add(a: i32, b: i32) -> i32 { a + b }",
        )
        .unwrap();
        fs::write(
            root.join("tools/report.py"),
            "if __name__ == \"__main__\":\n    main()\n",
        )
        .unwrap();
        fs::write(root.join("README.md"), "# title\n").unwrap();
        fs::write(root.join("empty.rs"), "").unwrap();

        let entries = walk(&root).unwrap();
        let get = |p: &str| entries.iter().find(|e| e.path == p).unwrap();

        let main = get("src/main.rs");
        assert_eq!(main.lang, "rust");
        assert_eq!(main.lines, 3);
        assert!(main.mtime > 0);
        assert!(main.is_entrypoint);

        assert_eq!(get("src/lib.rs").lines, 1);
        assert!(get("src/lib.rs").is_entrypoint);

        let py = get("tools/report.py");
        assert_eq!(py.lang, "python");
        assert_eq!(py.lines, 2);
        assert!(py.is_entrypoint);

        assert_eq!(get("README.md").lang, "markdown");
        assert!(!get("README.md").is_entrypoint);
        assert_eq!(get("empty.rs").lines, 0);

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn missing_root_is_an_error() {
        assert!(walk(Path::new("/definitely-missing-rsk1")).is_err());
    }
}
