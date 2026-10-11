//! `repowalk tour <path>` — numbered reading order with why-lines (core::tour).
//! OWNER: RSK-12.
use repowalk_core::pipeline;
use repowalk_core::tour::{self, TourItem};
use std::path::Path;

pub fn run(path: &Path) -> anyhow::Result<()> {
    let idx = pipeline::build_index(path, &|_done| {})?;
    print!("{}", render_tour(&tour::tour(&idx, 20)));
    Ok(())
}

/// Numbered reading order with why-lines, one per line. Deterministic.
pub fn render_tour(items: &[TourItem]) -> String {
    use crate::render::head;
    let mut out = String::new();
    out.push_str(&head("Tour — start here"));
    out.push('\n');
    for (i, item) in items.iter().enumerate() {
        out.push_str(&format!("{}. {} — {}\n", i + 1, item.path, item.why));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use repowalk_core::model::Index;
    use std::path::PathBuf;

    fn temp_root(tag: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("rw-cli-tour-{}-{}", std::process::id(), tag));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn tour_lists_numbered_paths_with_why_lines() {
        let root = temp_root("content");
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(
            root.join("src/main.rs"),
            "fn main() { helper(); }\nfn helper() {}\n",
        )
        .unwrap();
        std::fs::write(root.join("src/lib.rs"), "pub fn helper() {}\n").unwrap();
        let idx = pipeline::build_index(&root, &|_| {}).unwrap();
        let items = tour::tour(&idx, 10);
        assert!(!items.is_empty());

        let out = render_tour(&items);
        assert!(out.contains("1. "), "numbered: {out}");
        for item in &items {
            assert!(out.contains(&item.path), "path {} listed: {out}", item.path);
            assert!(out.contains(&item.why), "why-line for {}: {out}", item.path);
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn tour_of_empty_index_renders_header_only() {
        let idx = Index::new("/tmp/empty");
        let out = render_tour(&tour::tour(&idx, 10));
        assert!(out.contains("Tour"));
        assert!(!out.contains("1. "), "no items: {out}");
    }
}
