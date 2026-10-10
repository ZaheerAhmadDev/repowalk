//! tree-sitter-rust extraction: defs, doc comments, call sites, use-decls (plan §8.1).
//! OWNER: RSK-3. Produces the same Symbol shapes as parse_py via shared conventions:
//! id = "{path}:{start_line}", start_line = node.start_position.row + 1.
use crate::model::{CallSite, Import, Symbol, SymbolKind};
use tree_sitter::{Node, Parser, Query, QueryCursor, StreamingIterator};

const DEFS: &str = r#"
(function_item name: (identifier) @name) @def
(struct_item name: (type_identifier) @name) @def
(enum_item name: (type_identifier) @name) @def
(trait_item name: (type_identifier) @name) @def
(impl_item type: (type_identifier) @name) @def
(impl_item type: (generic_type (type_identifier) @name)) @def
(mod_item name: (identifier) @name) @def
"#;

const CALLS: &str = r#"
(call_expression function: (identifier) @callee) @call
(call_expression function: (scoped_identifier name: (identifier) @callee)) @call
(call_expression function: (field_expression field: (identifier) @callee)) @call
"#;

const USES: &str = r#"
(use_declaration argument: (_) @target) @u
"#;

/// Extract symbols, call sites and imports from one Rust source file.
pub fn extract(source: &str, path: &str) -> (Vec<Symbol>, Vec<CallSite>, Vec<Import>) {
    let mut parser = Parser::new();
    if parser.set_language(&tree_sitter_rust::LANGUAGE.into()).is_err() {
        return (Vec::new(), Vec::new(), Vec::new());
    }
    let Some(tree) = parser.parse(source, None) else {
        return (Vec::new(), Vec::new(), Vec::new());
    };
    let root = tree.root_node();

    let symbols = extract_defs(root, source, path);
    let calls = extract_calls(root, source, path);
    let imports = extract_imports(root, source, path);
    (symbols, calls, imports)
}

fn extract_defs(root: Node, source: &str, path: &str) -> Vec<Symbol> {
    let Ok(query) = Query::new(&tree_sitter_rust::LANGUAGE.into(), DEFS) else {
        return Vec::new();
    };
    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(&query, root, source.as_bytes());
    let names = query.capture_names();
    let mut out = Vec::new();
    while let Some(m) = matches.next() {
        let (Some(def), Some(name)) = (find_capture(&m, names, "def"), find_capture(&m, names, "name"))
        else {
            continue;
        };
        let Some(name) = text(name, source) else { continue };
        let start_line = def.start_position.row + 1;
        let (kind, container) = classify(def, source);
        out.push(Symbol {
            id: format!("{path}:{start_line}"),
            name: name.to_string(),
            kind,
            signature: signature(def, source),
            doc: leading_doc(def, source),
            start_line,
            end_line: def.end_position.row + 1,
            container,
            file: path.to_string(),
        });
    }
    out
}

fn extract_calls(root: Node, source: &str, path: &str) -> Vec<CallSite> {
    let Ok(query) = Query::new(&tree_sitter_rust::LANGUAGE.into(), CALLS) else {
        return Vec::new();
    };
    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(&query, root, source.as_bytes());
    let names = query.capture_names();
    let mut out = Vec::new();
    while let Some(m) = matches.next() {
        let (Some(call), Some(callee)) =
            (find_capture(&m, names, "call"), find_capture(&m, names, "callee"))
        else {
            continue;
        };
        let Some(callee) = text(callee, source) else { continue };
        out.push(CallSite {
            file: path.to_string(),
            callee: callee.to_string(),
            line: call.start_position.row + 1,
            enclosing: enclosing_fn(call, source),
        });
    }
    out
}

fn extract_imports(root: Node, source: &str, path: &str) -> Vec<Import> {
    let Ok(query) = Query::new(&tree_sitter_rust::LANGUAGE.into(), USES) else {
        return Vec::new();
    };
    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(&query, root, source.as_bytes());
    let names = query.capture_names();
    let mut out = Vec::new();
    while let Some(m) = matches.next() {
        let Some(target) = find_capture(&m, names, "target").and_then(|n| text(n, source)) else {
            continue;
        };
        out.push(Import {
            file: path.to_string(),
            target: target.to_string(),
        });
    }
    out
}

type QueryMatch<'a, 'tree> = tree_sitter::QueryMatch<'a, 'tree, &'tree [u8]>;

fn find_capture<'a, 'tree>(
    m: &QueryMatch<'a, 'tree>,
    names: &[&str],
    want: &str,
) -> Option<Node<'tree>> {
    m.captures
        .iter()
        .find(|c| names.get(c.index as usize) == Some(&want))
        .map(|c| c.node)
}

fn text<'a>(node: Node, source: &'a str) -> Option<&'a str> {
    node.utf8_text(source.as_bytes()).ok()
}

fn signature(def: Node, source: &str) -> String {
    let Some(raw) = text(def, source) else {
        return String::new();
    };
    let end = raw.find('{').or_else(|| raw.find(';')).unwrap_or(raw.len());
    raw[..end].trim().to_string()
}

fn leading_doc(def: Node, source: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();
    let mut prev = def.prev_sibling();
    while let Some(sib) = prev {
        if !sib.kind().contains("comment") {
            break;
        }
        let Some(raw) = text(sib, source) else { break };
        parts.push(raw);
        prev = sib.prev_sibling();
    }
    parts.reverse();
    parts
        .iter()
        .map(|c| clean_comment(c))
        .collect::<Vec<_>>()
        .join("\n")
}

fn clean_comment(raw: &str) -> String {
    let t = raw.trim();
    if let Some(rest) = t.strip_prefix("///") {
        return rest.strip_prefix(' ').unwrap_or(rest).to_string();
    }
    if let Some(rest) = t.strip_prefix("//") {
        return rest.trim_start().to_string();
    }
    let inner = t
        .strip_prefix("/*")
        .and_then(|s| s.strip_suffix("*/"))
        .unwrap_or(t);
    inner
        .lines()
        .map(|l| l.trim().trim_start_matches('*').trim())
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

fn classify(def: Node, source: &str) -> (SymbolKind, Option<String>) {
    match def.kind() {
        "function_item" => {
            let mut cur = def.parent();
            while let Some(p) = cur {
                match p.kind() {
                    "impl_item" => return (SymbolKind::Method, impl_type_name(p, source)),
                    "trait_item" => {
                        let t = p
                            .child_by_field_name("name")
                            .and_then(|n| text(n, source))
                            .map(str::to_string);
                        return (SymbolKind::Method, t);
                    }
                    "mod_item" => {
                        let t = p
                            .child_by_field_name("name")
                            .and_then(|n| text(n, source))
                            .map(str::to_string);
                        return (SymbolKind::Function, t);
                    }
                    _ => cur = p.parent(),
                }
            }
            (SymbolKind::Function, None)
        }
        "struct_item" => (SymbolKind::Struct, None),
        "enum_item" => (SymbolKind::Enum, None),
        "trait_item" => (SymbolKind::Trait, None),
        "mod_item" => (SymbolKind::Mod, None),
        _ => (SymbolKind::Struct, None),
    }
}

fn impl_type_name(impl_node: Node, source: &str) -> Option<String> {
    let ty = impl_node.child_by_field_name("type")?;
    first_type_ident(ty)
        .and_then(|n| text(n, source))
        .map(str::to_string)
}

fn first_type_ident(n: Node) -> Option<Node> {
    if n.kind() == "type_identifier" {
        return Some(n);
    }
    n.child(0).and_then(first_type_ident)
}

fn enclosing_fn(mut node: Node, source: &str) -> Option<String> {
    while let Some(parent) = node.parent() {
        if parent.kind() == "function_item" {
            return parent
                .child_by_field_name("name")
                .and_then(|n| text(n, source))
                .map(str::to_string);
        }
        node = parent;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"use crate::model::Thing;
use std::collections::HashMap;

/// Adds numbers.
pub fn add(a: i32, b: i32) -> i32 { a + b }

struct Point { x: i32 }

/// A shape.
enum Shape { Circle, Square }

trait Draw {
    fn draw(&self);
}

impl Point {
    pub fn new() -> Self {
        helper();
        Self { x: 0 }
    }
}

impl<T: Clone> Wrapper<T> {
    fn get(&self) -> T { self.0.clone() }
}

mod inner {
    pub fn nested() {
        super::add(1, 2);
        let _v = vec![1, 2];
    }
}
"#;

    #[test]
    fn empty_source_yields_nothing() {
        let (s, c, i) = extract("", "src/empty.rs");
        assert!(s.is_empty() && c.is_empty() && i.is_empty());
    }

    #[test]
    fn defs_kinds_lines_and_ids() {
        let (symbols, _, _) = extract(SAMPLE, "src/lib.rs");
        let get = |name: &str| symbols.iter().find(|s| s.name == name).unwrap();

        let add = get("add");
        assert_eq!(add.kind, SymbolKind::Function);
        assert_eq!(add.id, "src/lib.rs:5");
        assert_eq!(add.start_line, 5);
        assert_eq!(add.signature, "pub fn add(a: i32, b: i32) -> i32");
        assert_eq!(add.container, None);

        assert_eq!(get("Point").kind, SymbolKind::Struct);
        assert_eq!(get("Shape").kind, SymbolKind::Enum);
        assert_eq!(get("Draw").kind, SymbolKind::Trait);
        assert_eq!(get("inner").kind, SymbolKind::Mod);

        let new = get("new");
        assert_eq!(new.kind, SymbolKind::Method);
        assert_eq!(new.container.as_deref(), Some("Point"));
        assert_eq!(new.start_line, 17);

        let draw = get("draw");
        assert_eq!(draw.kind, SymbolKind::Method);
        assert_eq!(draw.container.as_deref(), Some("Draw"));

        let nested = get("nested");
        assert_eq!(nested.kind, SymbolKind::Function);
        assert_eq!(nested.container.as_deref(), Some("inner"));
    }

    #[test]
    fn impl_symbols_take_type_name_and_generic_impls_match() {
        let (symbols, _, _) = extract(SAMPLE, "src/lib.rs");
        let impls: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.signature.starts_with("impl"))
            .collect();
        assert_eq!(impls.len(), 2);
        let point_impl = impls.iter().find(|s| s.name == "Point").unwrap();
        assert_eq!(point_impl.start_line, 16);
        let wrap_impl = impls.iter().find(|s| s.name == "Wrapper").unwrap();
        assert_eq!(wrap_impl.start_line, 23);
        assert_eq!(wrap_impl.signature, "impl<T: Clone> Wrapper<T>");

        let get = symbols.iter().find(|s| s.name == "get").unwrap();
        assert_eq!(get.kind, SymbolKind::Method);
        assert_eq!(get.container.as_deref(), Some("Wrapper"));
    }

    #[test]
    fn leading_docs_are_cleaned() {
        let (symbols, _, _) = extract(SAMPLE, "src/lib.rs");
        let add = symbols.iter().find(|s| s.name == "add").unwrap();
        assert_eq!(add.doc, "Adds numbers.");
        let shape = symbols.iter().find(|s| s.name == "Shape").unwrap();
        assert_eq!(shape.doc, "A shape.");
        let point = symbols.iter().find(|s| s.name == "Point").unwrap();
        assert!(point.doc.is_empty());
    }

    #[test]
    fn call_sites_plain_scoped_field_with_enclosing() {
        let (_, calls, _) = extract(SAMPLE, "src/lib.rs");
        let find = |callee: &str| calls.iter().find(|c| c.callee == callee).unwrap();

        let helper = find("helper");
        assert_eq!(helper.line, 18);
        assert_eq!(helper.enclosing.as_deref(), Some("new"));
        assert_eq!(helper.file, "src/lib.rs");

        let add = find("add");
        assert_eq!(add.line, 29);
        assert_eq!(add.enclosing.as_deref(), Some("nested"));

        let clone = find("clone");
        assert_eq!(clone.enclosing.as_deref(), Some("get"));

        assert!(!calls.iter().any(|c| c.callee == "vec"));
    }

    #[test]
    fn use_declarations_captured() {
        let (_, _, imports) = extract(SAMPLE, "src/lib.rs");
        let targets: Vec<&str> = imports.iter().map(|i| i.target.as_str()).collect();
        assert!(targets.contains(&"crate::model::Thing"));
        assert!(targets.contains(&"std::collections::HashMap"));
        assert!(imports.iter().all(|i| i.file == "src/lib.rs"));
    }

    #[test]
    fn block_docs_and_nested_calls() {
        let src = "/** Multi\n * line\n */\npub fn f() { g(h()); }";
        let (symbols, calls, _) = extract(src, "a.rs");
        assert_eq!(symbols[0].doc, "Multi\nline");
        let names: Vec<&str> = calls.iter().map(|c| c.callee.as_str()).collect();
        assert!(names.contains(&"g") && names.contains(&"h"));
        assert!(calls.iter().all(|c| c.enclosing.as_deref() == Some("f")));
    }
}
