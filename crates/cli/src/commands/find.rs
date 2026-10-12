//! `repowalk find <query> <path>` — ranked fuzzy hits: path:line  name  score.
//! OWNER: RSK-13.
use repowalk_core::model::Index;
use repowalk_core::pipeline;
use repowalk_core::search;
use std::path::Path;

pub fn run(query: &str, path: &Path) -> anyhow::Result<()> {
    let idx = pipeline::build_index(path, &|_done| {})?;
    print!("{}", render_find(&idx, query));
    Ok(())
}

/// Ranked hits, one per line: `path:line  name  score`. Empty query → hint.
pub fn render_find(idx: &Index, query: &str) -> String {
    if query.trim().is_empty() {
        return "usage: repowalk find <query> <path>\n".to_string();
    }
    let hits = search::search(idx, query, 20);
    if hits.is_empty() {
        return format!("No matches for {query:?}\n");
    }
    let mut out = String::new();
    for h in hits {
        let name = idx
            .symbols
            .iter()
            .find(|s| s.id == h.target)
            .map(|s| s.name.clone())
            .unwrap_or_else(|| h.target.clone());
        out.push_str(&format!("{}:{}  {}  {:.1}\n", h.path, h.line, name, h.score));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn temp_root(tag: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("rw-cli-find-{}-{}", std::process::id(), tag));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    fn sample_index(root: &Path) -> Index {
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(
            root.join("src/store.rs"),
            "pub fn save_index() {}\npub fn load_index() {}\n",
        )
        .unwrap();
        pipeline::build_index(root, &|_| {}).unwrap()
    }

    #[test]
    fn find_lists_ranked_hits_path_line_name_score() {
        let root = temp_root("content");
        let idx = sample_index(&root);
        let out = render_find(&idx, "index");

        assert!(out.contains("src/store.rs:"), "path:line: {out}");
        assert!(
            out.contains("save_index") && out.contains("load_index"),
            "{out}"
        );
        let line = out.lines().find(|l| l.contains("save_index")).unwrap();
        let score: f64 = line.split_whitespace().last().unwrap().parse().unwrap();
        assert!(score > 0.0, "positive score: {line}");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn find_ranking_is_score_descending() {
        let root = temp_root("rank");
        let idx = sample_index(&root);
        let out = render_find(&idx, "index");
        let scores: Vec<f64> = out
            .lines()
            .filter(|l| l.contains("src/store.rs:"))
            .map(|l| l.split_whitespace().last().unwrap().parse().unwrap())
            .collect();
        assert!(scores.windows(2).all(|w| w[0] >= w[1]), "desc: {scores:?}");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn find_no_hits_is_honest() {
        let root = temp_root("none");
        let idx = sample_index(&root);
        let out = render_find(&idx, "zzz_no_match");
        assert!(out.contains("No matches"), "{out}");
        let _ = std::fs::remove_dir_all(&root);
    }
}
