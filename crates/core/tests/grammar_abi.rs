//! Proves the tree-sitter grammar crates load and are ABI-compatible with the
//! pinned `tree-sitter` runtime (BLD-2).
//!
//! For every grammar the v2 pipeline uses (Rust, Python, TypeScript, JavaScript)
//! this registers the grammar on a fresh parser, parses a tiny *valid* snippet,
//! and asserts the resulting tree is error-free. A grammar whose ABI does not
//! match `tree-sitter` fails at `set_language` (or as an error node here).

fn assert_parses(language: tree_sitter::Language, source: &str) {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&language)
        .expect("grammar ABI is incompatible with the pinned tree-sitter runtime");
    let tree = parser.parse(source, None).expect("parser returned no tree");
    let root = tree.root_node();
    assert!(!root.kind().is_empty(), "root node kind is empty");
    assert!(
        !root.has_error(),
        "valid snippet parsed with errors: {}",
        root.to_sexp()
    );
}

#[test]
fn rust_grammar_loads() {
    assert_parses(
        tree_sitter_rust::LANGUAGE.into(),
        "fn main() { let x = 1 + 2; }\n",
    );
}

#[test]
fn python_grammar_loads() {
    assert_parses(
        tree_sitter_python::LANGUAGE.into(),
        "def main():\n    return 1 + 2\n",
    );
}

#[test]
fn typescript_grammar_loads() {
    assert_parses(
        tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        "function main(): number { const x: number = 1 + 2; return x; }\n",
    );
}

#[test]
fn javascript_grammar_loads() {
    assert_parses(
        tree_sitter_javascript::LANGUAGE.into(),
        "function main() { const x = 1 + 2; return x; }\n",
    );
}
