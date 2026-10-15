//! v2 per-language analysis: variables, control flow, types, references.
//!
//! Fills the frozen [`crate::analysis`] contract. `Lang` mirrors the `lang`
//! strings produced by [`crate::walk::walk`] (`walk::lang_of`), so the learn
//! pipeline can dispatch without re-deriving extensions.
//!
//! OWNER: INTEL team. Each language lives in its own file (`rust.rs`,
//! `python.rs`, `ts.rs`) and appends into a shared [`Analysis`]; the shared
//! helpers below are the only code meant to change rarely.
//!
//! Rules for every language module:
//! - never panic on malformed/incomplete input — return what was found
//! - use [`enclosing_symbol`] so `Variable::enclosing` / `UseSite` targets use
//!   the canonical `file:start_line` symbol id (same ids as `model::Symbol::id`)
//! - labels carry real source text (conditions, call names), not placeholders
use crate::analysis::Analysis;
use crate::model::Symbol;

pub mod python;
pub mod rust;
pub mod ts;

/// Supported analysis languages (mirrors `walk::lang_of` values).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    Rust,
    Python,
    TypeScript,
    JavaScript,
}

impl Lang {
    /// Map a `FileEntry.lang` string to a language we can analyze.
    pub fn from_walk(lang: &str) -> Option<Lang> {
        match lang {
            "rust" => Some(Lang::Rust),
            "python" => Some(Lang::Python),
            "typescript" => Some(Lang::TypeScript),
            "javascript" => Some(Lang::JavaScript),
            _ => None,
        }
    }
}

/// Analyze one file, appending everything found into `out`.
pub fn analyze(file: &str, lang: Lang, source: &str, symbols: &[Symbol], out: &mut Analysis) {
    match lang {
        Lang::Rust => rust::analyze(file, source, symbols, out),
        Lang::Python => python::analyze(file, source, symbols, out),
        Lang::TypeScript | Lang::JavaScript => ts::analyze(file, source, symbols, out),
    }
}

/// The tightest symbol whose line range contains `line`.
pub fn enclosing_symbol_at(symbols: &[Symbol], line: usize) -> Option<&Symbol> {
    symbols
        .iter()
        .filter(|s| s.start_line <= line && line <= s.end_line)
        .min_by_key(|s| s.end_line.saturating_sub(s.start_line))
}

/// Canonical enclosing symbol id (`file:start_line`) for a line, else `""`.
pub fn enclosing_symbol(symbols: &[Symbol], line: usize) -> String {
    enclosing_symbol_at(symbols, line)
        .map(|s| s.id.clone())
        .unwrap_or_default()
}

/// Trimmed source line (1-based; line 0 is not a real line and yields `""`),
/// for `UseSite::snippet`.
pub fn snippet(source: &str, line: usize) -> String {
    if line == 0 {
        return String::new();
    }
    source
        .lines()
        .nth(line - 1)
        .unwrap_or("")
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::SymbolKind;

    fn sym(id: &str, name: &str, start: usize, end: usize) -> Symbol {
        Symbol {
            id: id.to_string(),
            name: name.to_string(),
            kind: SymbolKind::Function,
            signature: String::new(),
            doc: String::new(),
            start_line: start,
            end_line: end,
            container: None,
            file: "a.rs".to_string(),
        }
    }

    #[test]
    fn from_walk_matches_walk_lang_strings() {
        assert_eq!(Lang::from_walk("rust"), Some(Lang::Rust));
        assert_eq!(Lang::from_walk("python"), Some(Lang::Python));
        assert_eq!(Lang::from_walk("typescript"), Some(Lang::TypeScript));
        assert_eq!(Lang::from_walk("javascript"), Some(Lang::JavaScript));
        assert_eq!(Lang::from_walk("markdown"), None);
    }

    #[test]
    fn enclosing_symbol_picks_the_tightest_range() {
        let symbols = vec![sym("a.rs:1", "outer", 1, 20), sym("a.rs:5", "inner", 5, 8)];
        assert_eq!(enclosing_symbol(&symbols, 6), "a.rs:5");
        assert_eq!(enclosing_symbol(&symbols, 15), "a.rs:1");
        assert_eq!(enclosing_symbol(&symbols, 99), "");
        assert_eq!(enclosing_symbol_at(&symbols, 6).unwrap().name, "inner");
    }

    #[test]
    fn snippet_is_trimmed_and_tolerates_bad_lines() {
        let src = "fn main() {\n    go();\n}\n";
        assert_eq!(snippet(src, 2), "go();");
        assert_eq!(snippet(src, 99), "");
        assert_eq!(snippet(src, 0), "");
    }

    #[test]
    fn dispatch_on_unparsable_input_never_panics() {
        // Stubs may ignore their inputs, but the entry point must stay total.
        let mut out = Analysis::new();
        analyze("a.rs", Lang::Rust, "", &[], &mut out);
        analyze("b.py", Lang::Python, "def (:", &[], &mut out);
        analyze("c.ts", Lang::TypeScript, "class {", &[], &mut out);
    }
}
