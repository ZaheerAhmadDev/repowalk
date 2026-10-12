//! `repowalk map <path>` — metrics, module tree, health in one screen.
//! OWNER: RSK-12. Pipeline: repowalk_core::pipeline::build_index → print (render.rs helpers).
use repowalk_core::metrics::{self, Metrics};
use repowalk_core::model::Index;
use repowalk_core::pipeline;
use std::path::Path;

pub fn run(path: &Path) -> anyhow::Result<()> {
    let idx = pipeline::build_index(path, &|_done| {})?;
    print!("{}", render_map(&idx, &metrics::compute(&idx)));
    Ok(())
}

/// One-screen orientation: languages, LOC, entrypoints, module tree,
/// top files, hottest symbols, health line. Deterministic.
pub fn render_map(idx: &Index, m: &Metrics) -> String {
    use crate::render::{dim, head, warn};
    let mut out = String::new();
    out.push_str(&head(&format!("repowalk map — {}", idx.repo_root)));
    out.push('\n');

    let langs: Vec<String> = m
        .langs
        .iter()
        .map(|(l, n)| format!("{l} ({n} files)"))
        .collect();
    out.push_str(&format!("Languages: {}\n", langs.join(", ")));
    out.push_str(&format!("LOC: {}\n", m.total_loc));
    out.push_str(&format!(
        "Entrypoints ({}): {}\n",
        m.entrypoints.len(),
        if m.entrypoints.is_empty() {
            "none".to_string()
        } else {
            m.entrypoints.join(", ")
        }
    ));

    out.push_str(&head("Module tree"));
    out.push('\n');
    out.push_str(&render_module_tree(idx));

    out.push_str(&head("Top files"));
    out.push('\n');
    for (path, lines) in m.top_files.iter().take(5) {
        out.push_str(&format!("  {path} {} {lines} lines\n", dim("·")));
    }

    out.push_str(&head("Hottest symbols (fan-in)"));
    out.push('\n');
    if m.fan_in.is_empty() {
        out.push_str(&format!("  {}\n", dim("no call edges found")));
    }
    for (sym, n) in m.fan_in.iter().take(5) {
        out.push_str(&format!("  {sym} {} {n} callers\n", dim("·")));
    }

    out.push_str(&warn(&health_line(m)));
    out.push('\n');
    out
}

/// One-line health summary: cycles, god-files, unresolved edge %.
pub fn health_line(m: &Metrics) -> String {
    format!(
        "● {} cycles · {} god-files · {:.0}% edges unresolved",
        m.cycles.len(),
        m.god_files.len(),
        m.unresolved_pct * 100.0
    )
}

/// Deterministic directory tree: every directory that holds indexed files,
/// sorted, with per-directory file lists (plan §3.1 "module tree").
pub fn render_module_tree(idx: &Index) -> String {
    use std::collections::BTreeMap;
    let mut dirs: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for f in &idx.files {
        let (dir, name) = match f.path.rsplit_once('/') {
            Some((d, n)) => (d.to_string(), n.to_string()),
            None => (String::new(), f.path.clone()),
        };
        dirs.entry(dir).or_default().push(name);
    }
    let mut out = String::new();
    for (dir, mut files) in dirs {
        files.sort();
        let label = if dir.is_empty() { "." } else { dir.as_str() };
        out.push_str(&format!("  {label}/\n"));
        for f in files {
            out.push_str(&format!("    {f}\n"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn temp_root(tag: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("rw-cli-map-{}-{}", std::process::id(), tag));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    fn sample_index(root: &Path) -> Index {
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(
            root.join("src/main.rs"),
            "fn main() { helper(); }\nfn helper() {}\n",
        )
        .unwrap();
        std::fs::write(root.join("src/util.py"), "def run():\n    pass\n").unwrap();
        pipeline::build_index(root, &|_| {}).unwrap()
    }

    #[test]
    fn map_shows_languages_loc_entrypoints_and_health() {
        let root = temp_root("content");
        let idx = sample_index(&root);
        let m = metrics::compute(&idx);
        let out = render_map(&idx, &m);

        assert!(out.contains("Languages:"));
        assert!(out.contains("rust (1 files)"));
        assert!(out.contains("python (1 files)"));
        assert!(out.contains("LOC: 4"));
        assert!(out.contains("Entrypoints (1): src/main.rs"));
        assert!(out.contains("Top files"));
        assert!(out.contains("Hottest symbols"));
        assert!(
            out.contains("cycles") && out.contains("unresolved"),
            "health line missing: {out}"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn map_module_tree_groups_by_directory() {
        let root = temp_root("tree");
        let idx = sample_index(&root);
        let tree = render_module_tree(&idx);
        assert!(tree.contains("src/"));
        assert!(tree.contains("main.rs"));
        assert!(tree.contains("util.py"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn health_line_formats_counts_and_percent() {
        let root = temp_root("health");
        let idx = sample_index(&root);
        let m = metrics::compute(&idx);
        let line = health_line(&m);
        assert!(line.starts_with("● "), "{line}");
        assert!(line.contains("0 cycles"), "{line}");
        let _ = std::fs::remove_dir_all(&root);
    }
}
