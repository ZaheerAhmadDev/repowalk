//! Fuzzy subsequence search over symbol names + file paths (plan §7.5).
//! OWNER: RSK-8. Deterministic fzf-style scoring:
/// +4 exact substring in name, +2 subsequence, +1 consecutive/word-boundary, +0.5 doc/path hit.
use crate::model::{Index, Symbol};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hit {
    pub target: String, // symbol id or file path
    pub path: String,
    pub line: usize,
    pub score: f64,
}

/// Score `query` against one name: returns None if query is not a subsequence
/// of name (case-insensitive). Scoring per plan §7.5.
fn score_name(name: &str, query: &str) -> Option<f64> {
    let mut score = 0.0;
    if name.to_lowercase().contains(&query.to_lowercase()) {
        score += 4.0;
    }
    let name_chars: Vec<char> = name.chars().collect();
    let query_chars: Vec<char> = query.chars().collect();
    let mut positions = Vec::with_capacity(query_chars.len());
    let mut qi = 0;
    for (i, c) in name_chars.iter().enumerate() {
        if qi < query_chars.len() && c.eq_ignore_ascii_case(&query_chars[qi]) {
            positions.push(i);
            qi += 1;
        }
    }
    if qi < query_chars.len() {
        return None;
    }
    score += 2.0;
    for w in positions.windows(2) {
        if w[1] == w[0] + 1 {
            score += 1.0;
        }
    }
    for &p in &positions {
        if p == 0 {
            score += 1.0;
        } else {
            let prev = name_chars[p - 1];
            let curr = name_chars[p];
            let sep = matches!(prev, '_' | '-' | '/' | '.' | ' ');
            let camel = prev.is_lowercase() && curr.is_uppercase();
            if sep || camel {
                score += 1.0;
            }
        }
    }
    Some(score)
}

fn symbol_hit(sym: &Symbol, query_lower: &str) -> Option<Hit> {
    let mut score = score_name(&sym.name, query_lower)?;
    if !sym.doc.is_empty() && sym.doc.to_lowercase().contains(query_lower) {
        score += 0.5;
    }
    if sym.file.to_lowercase().contains(query_lower) {
        score += 0.5;
    }
    Some(Hit {
        target: sym.id.clone(),
        path: sym.file.clone(),
        line: sym.start_line,
        score,
    })
}

/// Top `limit` hits for `query`, sorted score desc (ties broken by target asc).
/// Empty query returns no hits. Covers symbols AND files, case-insensitive.
pub fn search(idx: &Index, query: &str, limit: usize) -> Vec<Hit> {
    if query.is_empty() || limit == 0 {
        return Vec::new();
    }
    let query_lower = query.to_lowercase();
    let mut hits: Vec<Hit> = Vec::new();
    for sym in &idx.symbols {
        if let Some(hit) = symbol_hit(sym, &query_lower) {
            hits.push(hit);
        }
    }
    for file in &idx.files {
        if let Some(score) = score_name(&file.path, &query_lower) {
            hits.push(Hit {
                target: file.path.clone(),
                path: file.path.clone(),
                line: 0,
                score,
            });
        }
    }
    hits.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap()
            .then_with(|| a.target.cmp(&b.target))
    });
    hits.truncate(limit);
    hits
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{FileEntry, Symbol, SymbolKind};

    fn fixture() -> Index {
        let mut idx = Index::new("/repo");
        idx.files.push(FileEntry {
            path: "src/foo_helper.rs".into(),
            lang: "rust".into(),
            lines: 10,
            mtime: 0,
            is_entrypoint: false,
        });
        idx.symbols.push(Symbol {
            id: "src/foo_helper.rs:1".into(),
            name: "foobar".into(),
            kind: SymbolKind::Function,
            signature: "fn foobar()".into(),
            doc: String::new(),
            start_line: 1,
            end_line: 3,
            container: None,
            file: "src/foo_helper.rs".into(),
        });
        idx.symbols.push(Symbol {
            id: "src/other.rs:5".into(),
            name: "baz_f_o_o_qux".into(),
            kind: SymbolKind::Function,
            signature: "fn baz_f_o_o_qux()".into(),
            doc: String::new(),
            start_line: 5,
            end_line: 7,
            container: None,
            file: "src/other.rs".into(),
        });
        idx
    }

    #[test]
    fn exact_substring_beats_fuzzy_subsequence() {
        let idx = fixture();
        let hits = search(&idx, "foo", 10);
        assert!(hits.len() >= 2);
        assert_eq!(hits[0].target, "src/foo_helper.rs:1");
        assert!(hits[0].score > hits[1].score);
    }

    #[test]
    fn empty_query_returns_nothing() {
        let idx = fixture();
        assert!(search(&idx, "", 10).is_empty());
    }

    #[test]
    fn case_insensitive() {
        let idx = fixture();
        let lower = search(&idx, "foo", 10);
        let upper = search(&idx, "FOO", 10);
        let mixed = search(&idx, "FoO", 10);
        assert_eq!(lower[0].target, upper[0].target);
        assert_eq!(lower[0].score, upper[0].score);
        assert_eq!(lower[0].score, mixed[0].score);
    }

    #[test]
    fn covers_files_and_symbols_sorted_deterministically() {
        let idx = fixture();
        let hits = search(&idx, "foo", 10);
        let targets: Vec<&str> = hits.iter().map(|h| h.target.as_str()).collect();
        assert!(targets.contains(&"src/foo_helper.rs:1"));
        assert!(targets.contains(&"src/foo_helper.rs"));
        for w in hits.windows(2) {
            assert!(w[0].score > w[1].score || (w[0].score == w[1].score && w[0].target < w[1].target));
        }
    }

    #[test]
    fn limit_truncates() {
        let idx = fixture();
        let hits = search(&idx, "o", 1);
        assert_eq!(hits.len(), 1);
    }
}
