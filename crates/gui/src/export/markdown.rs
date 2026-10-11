//! Tour + health report → Markdown (hand-rolled). OWNER: RSK-22.
//! Deterministic, copy-to-clipboard friendly (plan §3.2 pick 6).
use repowalk_core::metrics;
use repowalk_core::model::Index;
use repowalk_core::tour;

pub fn tour_markdown(idx: &Index) -> String {
    let m = metrics::compute(idx);
    let mut out = String::new();
    out.push_str("# repowalk tour\n\n");
    out.push_str(&format!(
        "- repo: `{}`\n- LOC: {}\n- languages: {}\n- entrypoints ({}): {}\n\n",
        idx.repo_root,
        m.total_loc,
        m.langs
            .iter()
            .map(|(l, n)| format!("{l} ({n})"))
            .collect::<Vec<_>>()
            .join(", "),
        m.entrypoints.len(),
        if m.entrypoints.is_empty() {
            "none".to_string()
        } else {
            m.entrypoints.join(", ")
        }
    ));
    out.push_str("## Reading order\n\n");
    for (i, item) in tour::tour(idx, 20).iter().enumerate() {
        out.push_str(&format!("{}. **{}** — {}\n", i + 1, item.path, item.why));
    }
    out
}

/// Health report → Markdown: cycles, god-files, unresolved %, entrypoints.
pub fn health_markdown(idx: &Index) -> String {
    let m = metrics::compute(idx);
    let mut out = String::new();
    out.push_str("# Health report\n\n");
    out.push_str(&format!(
        "● {} cycles · {} god-files · {:.0}% edges unresolved · {} Entrypoints\n\n",
        m.cycles.len(),
        m.god_files.len(),
        m.unresolved_pct * 100.0,
        m.entrypoints.len()
    ));
    out.push_str("## Cycles (spaghetti zones)\n\n");
    if m.cycles.is_empty() {
        out.push_str("none\n");
    }
    for cycle in &m.cycles {
        out.push_str(&format!("- {}\n", cycle.join(" ↔ ")));
    }
    out.push_str("\n## God files\n\n");
    if m.god_files.is_empty() {
        out.push_str("none\n");
    }
    for god in &m.god_files {
        out.push_str(&format!("- {god}\n"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use repowalk_core::pipeline;

    fn temp_root(tag: &str) -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!("rw-md-{}-{}", std::process::id(), tag));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    fn sample_index(root: &std::path::Path) -> Index {
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(
            root.join("src/main.rs"),
            "//! Entry point.\nfn main() { helper(); }\n\n/// A helper.\npub fn helper() {}\n",
        )
        .unwrap();
        pipeline::build_index(root, &|_| {}).unwrap()
    }

    #[test]
    fn tour_markdown_has_metrics_header_and_numbered_list() {
        let root = temp_root("tour");
        let idx = sample_index(&root);
        let md = tour_markdown(&idx);

        assert!(md.starts_with("# repowalk tour"), "{md}");
        assert!(md.contains("LOC:"), "metrics header: {md}");
        assert!(md.contains("rust"), "languages: {md}");
        assert!(md.contains("1. "), "numbered items: {md}");
        assert!(md.contains("src/main.rs"), "tour path: {md}");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn tour_markdown_is_deterministic() {
        let root = temp_root("det");
        let idx = sample_index(&root);
        assert_eq!(tour_markdown(&idx), tour_markdown(&idx));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn health_markdown_lists_health_sections() {
        let root = temp_root("health");
        let idx = sample_index(&root);
        let md = health_markdown(&idx);

        assert!(md.starts_with("# Health report"), "{md}");
        assert!(md.contains("Cycles"), "{md}");
        assert!(md.contains("God files"), "{md}");
        assert!(md.contains("unresolved"), "{md}");
        assert!(md.contains("Entrypoints"), "{md}");
        let _ = std::fs::remove_dir_all(&root);
    }
}
