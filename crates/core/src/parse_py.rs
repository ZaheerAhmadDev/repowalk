//! tree-sitter-python extraction: defs (incl. async/class), docstrings,
//! call sites, imports (plan §8.1).
//! OWNER: RSK-4. Same Symbol conventions as parse_rust (id = "{path}:{start_line}").
use crate::model::{CallSite, Import, Symbol, SymbolKind};
use tree_sitter::{Node, Parser, Query, QueryCursor, StreamingIterator};

const DEFS_QUERY: &str = r#"
(function_definition name: (identifier) @name) @def
(class_definition name: (identifier) @name) @def
"#;

const CALLS_QUERY: &str = r#"
(call function: (identifier) @callee) @call
(call function: (attribute attribute: (identifier) @callee)) @call
"#;

const IMPORTS_QUERY: &str = r#"
(import_statement) @import
(import_from_statement) @import
"#;

/// Extract symbols, call sites and imports from one Python source file.
pub fn extract(source: &str, path: &str) -> (Vec<Symbol>, Vec<CallSite>, Vec<Import>) {
    let language: tree_sitter::Language = tree_sitter_python::LANGUAGE.into();
    let mut parser = Parser::new();
    parser
        .set_language(&language)
        .expect("tree-sitter-python version mismatch");
    let tree = parser.parse(source, None).expect("python parse failed");
    let root = tree.root_node();

    let defs_q = Query::new(&language, DEFS_QUERY).expect("valid defs query");
    let calls_q = Query::new(&language, CALLS_QUERY).expect("valid calls query");
    let imports_q = Query::new(&language, IMPORTS_QUERY).expect("valid imports query");

    let mut symbols = extract_defs(&defs_q, root, source, path);
    symbols.sort_by_key(|s| (s.start_line, s.name.clone()));

    let mut call_sites = extract_calls(&calls_q, root, source, path);
    call_sites.sort_by_key(|c| (c.line, c.callee.clone()));

    let imports = extract_imports(&imports_q, root, source, path);

    (symbols, call_sites, imports)
}

fn extract_defs(query: &Query, root: Node, source: &str, path: &str) -> Vec<Symbol> {
    let names = query.capture_names();
    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(query, root, source.as_bytes());
    let mut out = Vec::new();
    while let Some(m) = matches.next() {
        let (Some(def), Some(name)) = (
            capture(m.captures(), names, "def"),
            capture(m.captures(), names, "name"),
        ) else {
            continue;
        };
        let name_text = node_text(name, source);
        let enclosing = enclosing_chain(def, source);
        let is_class = def.kind() == "class_definition";
        let in_class = !is_class && has_class_ancestor(def);
        let kind = if is_class {
            SymbolKind::Class
        } else if in_class {
            SymbolKind::Method
        } else {
            SymbolKind::Function
        };
        let start_line = def.start_position().row + 1;
        out.push(Symbol {
            id: format!("{path}:{start_line}"),
            name: name_text,
            kind,
            signature: signature_of(def, source),
            doc: docstring_of(def, source),
            start_line,
            end_line: def.end_position().row + 1,
            container: if enclosing.is_empty() {
                None
            } else {
                Some(enclosing.join("."))
            },
            file: path.to_string(),
        });
    }
    out
}

fn extract_calls(query: &Query, root: Node, source: &str, path: &str) -> Vec<CallSite> {
    let names = query.capture_names();
    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(query, root, source.as_bytes());
    let mut out = Vec::new();
    while let Some(m) = matches.next() {
        let (Some(call), Some(callee)) = (
            capture(m.captures(), names, "call"),
            capture(m.captures(), names, "callee"),
        ) else {
            continue;
        };
        if has_ancestor_kind(call, "decorator") {
            continue;
        }
        let chain = enclosing_chain(call, source);
        out.push(CallSite {
            file: path.to_string(),
            callee: node_text(callee, source),
            line: call.start_position().row + 1,
            enclosing: chain.last().cloned(),
        });
    }
    out
}

fn extract_imports(query: &Query, root: Node, source: &str, path: &str) -> Vec<Import> {
    let names = query.capture_names();
    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(query, root, source.as_bytes());
    let mut out = Vec::new();
    while let Some(m) = matches.next() {
        let Some(stmt) = capture(m.captures(), names, "import") else {
            continue;
        };
        for target in import_targets(stmt, source) {
            out.push(Import {
                file: path.to_string(),
                target,
            });
        }
    }
    out
}

fn capture<'tree>(
    caps: &[tree_sitter::QueryCapture<'tree>],
    names: &[&str],
    want: &str,
) -> Option<Node<'tree>> {
    caps.iter()
        .find(|c| names.get(c.index as usize).copied() == Some(want))
        .map(|c| c.node)
}

fn node_text(node: Node, source: &str) -> String {
    node.utf8_text(source.as_bytes()).unwrap_or("").to_string()
}

/// Class/function names surrounding `node`, outermost first.
fn enclosing_chain(node: Node, source: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut cur = node.parent();
    while let Some(n) = cur {
        if matches!(n.kind(), "class_definition" | "function_definition") {
            if let Some(id) = n.child_by_field_name("name") {
                names.push(node_text(id, source));
            }
        }
        cur = n.parent();
    }
    names.reverse();
    names
}

fn has_class_ancestor(node: Node) -> bool {
    let mut cur = node.parent();
    while let Some(n) = cur {
        if n.kind() == "class_definition" {
            return true;
        }
        cur = n.parent();
    }
    false
}

fn has_ancestor_kind(node: Node, kind: &str) -> bool {
    let mut cur = node.parent();
    while let Some(n) = cur {
        if n.kind() == kind {
            return true;
        }
        cur = n.parent();
    }
    false
}

/// Header text (up to the body) with newlines collapsed — e.g. `def foo(x: int) -> str`.
fn signature_of(node: Node, source: &str) -> String {
    let text = node_text(node, source);
    let end = node
        .child_by_field_name("body")
        .map(|b| b.start_byte() - node.start_byte())
        .unwrap_or(text.len());
    let header: String = text[..end]
        .lines()
        .map(str::trim)
        .collect::<Vec<_>>()
        .join(" ");
    header.trim().trim_end_matches(':').to_string()
}

/// First string literal at the top of the def body (skipping leading comments).
fn docstring_of(node: Node, source: &str) -> String {
    let Some(body) = node.child_by_field_name("body") else {
        return String::new();
    };
    let mut cursor = body.walk();
    for child in body.named_children(&mut cursor) {
        if child.kind() == "comment" {
            continue;
        }
        if child.kind() == "expression_statement" {
            if let Some(first) = child.named_child(0) {
                if first.kind() == "string" {
                    return unquote(&node_text(first, source));
                }
            }
        }
        break;
    }
    String::new()
}

/// Strip string prefix + quotes (single/triple, ' or ").
fn unquote(raw: &str) -> String {
    let s = raw.trim();
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() && b[i].is_ascii_alphabetic() {
        i += 1;
    }
    let s = &s[i..];
    for q in ["'''", "\"\"\"", "'", "\""] {
        let n = q.len();
        if s.len() >= 2 * n && s.starts_with(q) && s.ends_with(q) {
            return s[n..s.len() - n].to_string();
        }
    }
    s.to_string()
}

/// Import targets: `import a.b` → ["a.b"]; `from .m import x, y` → [".m.x", ".m.y"].
fn import_targets(stmt: Node, source: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cursor = stmt.walk();
    match stmt.kind() {
        "import_statement" => {
            for child in stmt.named_children(&mut cursor) {
                match child.kind() {
                    "dotted_name" => out.push(node_text(child, source)),
                    "aliased_import" => {
                        if let Some(n) = child.child_by_field_name("name") {
                            out.push(node_text(n, source));
                        }
                    }
                    _ => {}
                }
            }
        }
        "import_from_statement" => {
            let module_node = stmt.child_by_field_name("module_name");
            let module = module_node
                .map(|m| node_text(m, source))
                .unwrap_or_default();
            for child in stmt.named_children(&mut cursor) {
                if Some(child.id()) == module_node.map(|m| m.id()) {
                    continue; // module path itself, not an imported name
                }
                let name = match child.kind() {
                    "dotted_name" => node_text(child, source),
                    "aliased_import" => child
                        .child_by_field_name("name")
                        .map(|n| node_text(n, source))
                        .unwrap_or_default(),
                    "wildcard_import" => node_text(child, source),
                    _ => continue,
                };
                if module.is_empty() {
                    out.push(name);
                } else if module.ends_with('.') {
                    out.push(format!("{module}{name}"));
                } else {
                    out.push(format!("{module}.{name}"));
                }
            }
        }
        _ => {}
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::SymbolKind;

    const SRC: &str = r#""""Module docstring - not a symbol."""
import os
from pathlib import Path
from . import sibling
from .utils import helper as h

class Greeter(Base):
    """Greets people."""

    def hello(self, name: str) -> str:
        """Say hello."""
        msg = format_name(name)
        return msg.upper()

    async def run(self):
        await helper()

@myapp.route("/x")
def decorated():
    pass

def main():
    g = Greeter()
    g.hello("x")
    os.getcwd()

async def top():
    await main()
"#;

    fn find_symbol<'a>(syms: &'a [Symbol], name: &str) -> &'a Symbol {
        syms.iter()
            .find(|s| s.name == name)
            .unwrap_or_else(|| panic!("missing symbol {name}"))
    }

    #[test]
    fn extracts_defs_including_async_and_class() {
        let (syms, _, _) = extract(SRC, "m.py");
        assert_eq!(
            syms.len(),
            6,
            "symbols: {:?}",
            syms.iter().map(|s| &s.name).collect::<Vec<_>>()
        );

        let greeter = find_symbol(&syms, "Greeter");
        assert_eq!(greeter.kind, SymbolKind::Class);
        assert_eq!(greeter.id, "m.py:7");
        assert_eq!(greeter.doc, "Greets people.");
        assert_eq!(greeter.container, None);
        assert_eq!(greeter.signature, "class Greeter(Base)");

        let hello = find_symbol(&syms, "hello");
        assert_eq!(hello.kind, SymbolKind::Method);
        assert_eq!(hello.id, "m.py:10");
        assert_eq!(hello.doc, "Say hello.");
        assert_eq!(hello.container.as_deref(), Some("Greeter"));
        assert_eq!(hello.signature, "def hello(self, name: str) -> str");

        assert_eq!(find_symbol(&syms, "run").kind, SymbolKind::Method);
        assert_eq!(
            find_symbol(&syms, "run").container.as_deref(),
            Some("Greeter")
        );

        let deco = find_symbol(&syms, "decorated");
        assert_eq!(deco.kind, SymbolKind::Function);
        assert_eq!(deco.container, None);

        assert_eq!(find_symbol(&syms, "main").kind, SymbolKind::Function);
        assert_eq!(find_symbol(&syms, "top").kind, SymbolKind::Function);
        assert!(find_symbol(&syms, "top").id.ends_with(":27"));
        assert_eq!(find_symbol(&syms, "top").signature, "async def top()");
    }

    #[test]
    fn docstring_skips_leading_comments_and_module_doc() {
        let src = "def f():\n    # note\n    \"\"\"Real doc.\"\"\"\n    return 1\n";
        let (syms, _, _) = extract(src, "c.py");
        assert_eq!(syms[0].doc, "Real doc.");

        let src2 = "\"\"\"Mod.\"\"\"\ndef g():\n    pass\n";
        let (syms2, _, _) = extract(src2, "c2.py");
        assert_eq!(syms2.len(), 1);
        assert_eq!(syms2[0].doc, "");
    }

    #[test]
    fn calls_are_name_only_and_skip_decorators() {
        let (_, calls, _) = extract(SRC, "m.py");
        let get = |callee: &str| {
            calls
                .iter()
                .find(|c| c.callee == callee)
                .unwrap_or_else(|| panic!("missing call {callee}"))
        };

        // attribute call → name only
        assert_eq!(get("upper").enclosing.as_deref(), Some("hello"));
        assert_eq!(get("getcwd").enclosing.as_deref(), Some("main"));
        assert_eq!(get("hello").enclosing.as_deref(), Some("main"));
        assert_eq!(get("Greeter").enclosing.as_deref(), Some("main"));
        assert_eq!(get("format_name").enclosing.as_deref(), Some("hello"));
        assert_eq!(get("helper").enclosing.as_deref(), Some("run"));
        assert_eq!(get("main").enclosing.as_deref(), Some("top"));

        // decorator `@myapp.route("/x")` call is excluded
        assert!(
            calls.iter().all(|c| c.callee != "route"),
            "decorator call leaked: {calls:?}"
        );
    }

    #[test]
    fn imports_cover_relative_and_aliased() {
        let (_, _, imports) = extract(SRC, "m.py");
        let targets: Vec<&str> = imports.iter().map(|i| i.target.as_str()).collect();
        assert_eq!(
            targets,
            vec!["os", "pathlib.Path", ".sibling", ".utils.helper"]
        );
    }

    #[test]
    fn symbol_ids_use_path_and_start_line() {
        let (syms, _, _) = extract(SRC, "pkg/mod.py");
        for s in &syms {
            assert_eq!(s.id, format!("pkg/mod.py:{}", s.start_line));
            assert_eq!(s.file, "pkg/mod.py");
        }
    }
}
