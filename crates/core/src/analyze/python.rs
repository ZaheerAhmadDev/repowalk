//! Python analysis: variables, control flow, types, references (INTEL-3).
//!
//! Fills the frozen [`crate::analysis`] contract from tree-sitter-python. Uses
//! the shared helpers in [`super`] (`enclosing_symbol`, `snippet`) so ids match
//! `model::Symbol::id` (`"{path}:{start_line}"`).
//!
//! Design notes / choices:
//! - Every assignment target, parameter, `self.x` attribute and module-level
//!   name becomes a [`Variable`] (one logical variable per `(scope, name)`,
//!   `decl_line` = first declaration). `type_name` comes *only* from an explicit
//!   `AnnAssign`/parameter annotation, never inferred.
//! - Reads/writes are counted from name events recorded while walking the tree;
//!   an event counts toward the variable declared in the same scope key.
//! - One [`FunctionCfg`] per `function_definition` (incl. `async def`), with a
//!   `Back` edge for every loop body and `cyclomatic = decisions + 1`.
//! - `TypeEdge`: `Inherit` for base classes, `Field` for annotated fields whose
//!   base name matches a declared class. `Impl` is intentionally **skipped** for
//!   Python (class ownership is implicit in the class body) — recorded on the card.
//! - Everything is total: tree-sitter recovers from malformed input and no field
//!   access or `utf8_text` may panic.
use std::collections::{BTreeMap, HashMap};

use tree_sitter::{Node, Parser};

use crate::analysis::{
    Analysis, CfgEdge, CfgEdgeKind, CfgKind, CfgNode, Field, FunctionCfg, TypeEdge, TypeEdgeKind,
    TypeKind, TypeNode, UseKind, UseSite, VarKind, Variable,
};
use crate::model::{Symbol, SymbolKind};

use super::{enclosing_symbol, snippet};

/// Append Python variables/CFG/types/use-sites for `file` into `out`.
pub fn analyze(file: &str, source: &str, symbols: &[Symbol], out: &mut Analysis) {
    let language: tree_sitter::Language = tree_sitter_python::LANGUAGE.into();
    let mut parser = Parser::new();
    if parser.set_language(&language).is_err() {
        return;
    }
    let Some(tree) = parser.parse(source, None) else {
        return;
    };
    let root = tree.root_node();
    let file_lines = source.lines().count().max(1);

    let mut visitor = Visitor {
        file,
        source,
        symbols,
        file_lines,
        decls: Vec::new(),
        events: Vec::new(),
        uses: Vec::new(),
        types: Vec::new(),
        type_edges: Vec::new(),
    };
    visitor.visit(root);
    visitor.collect_types(root);
    let mut types = std::mem::take(&mut visitor.types);
    let mut type_edges = std::mem::take(&mut visitor.type_edges);
    let (variables, uses) = visitor.finish();

    let cfgs = collect_cfgs(file, source, root);

    types.sort_by(|a, b| (a.line, a.name.as_str()).cmp(&(b.line, b.name.as_str())));
    type_edges.sort_by(|a, b| {
        (a.from.as_str(), a.to.as_str(), a.line).cmp(&(b.from.as_str(), b.to.as_str(), b.line))
    });

    out.variables.extend(variables);
    out.cfgs.extend(cfgs);
    out.types.extend(types);
    out.type_edges.extend(type_edges);
    out.uses.extend(uses);
}

// ---------------------------------------------------------------------------
// Small pure helpers
// ---------------------------------------------------------------------------

fn node_text(node: Node, source: &str) -> String {
    node.utf8_text(source.as_bytes()).unwrap_or("").to_string()
}

fn first_line(text: &str) -> String {
    text.lines().next().unwrap_or("").trim().to_string()
}

/// Short single-line label from real source text (no placeholders).
fn label_of(node: Node, source: &str) -> String {
    let text = first_line(&node_text(node, source));
    let mut out: String = text.chars().take(60).collect();
    if text.chars().count() > 60 {
        out.push('…');
    }
    out
}

/// Base type name of an annotation: strips generics/quoting and module paths.
/// `Optional[typing.List[int]]` -> `Optional`, `pkg.Foo` -> `Foo`, `"Bar"` -> `Bar`.
fn base_type_name(raw: &str) -> String {
    let trimmed = raw.trim();
    let unquoted = trimmed.trim_matches(|c| c == '"' || c == '\'');
    let head = unquoted
        .split(['[', ',', '|'])
        .next()
        .unwrap_or(unquoted)
        .trim();
    head.rsplit('.').next().unwrap_or(head).trim().to_string()
}

/// Best-effort owning module path for a file (`pkg/mod.py` -> `pkg.mod`).
fn module_path_of(file: &str) -> String {
    let stripped = file
        .strip_suffix(".pyi")
        .or_else(|| file.strip_suffix(".py"))
        .unwrap_or(file);
    let mut parts: Vec<&str> = stripped
        .split(['/', '\\'])
        .filter(|s| !s.is_empty())
        .collect();
    if parts.last() == Some(&"__init__") {
        parts.pop();
    }
    parts.join(".")
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

fn find_child_kind<'t>(node: Node<'t>, kind: &str) -> Option<Node<'t>> {
    let mut cursor = node.walk();
    let mut it = node.named_children(&mut cursor);
    it.find(|c| c.kind() == kind)
}

fn all_of_kind<'t>(root: Node<'t>, kind: &str, out: &mut Vec<Node<'t>>) {
    if root.kind() == kind {
        out.push(root);
    }
    let mut cursor = root.walk();
    for k in root.named_children(&mut cursor) {
        all_of_kind(k, kind, out);
    }
}

/// Strip string prefix + quotes (single/triple, ' or ").
fn unquote(raw: &str) -> String {
    let s = raw.trim();
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() && bytes[i].is_ascii_alphabetic() {
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

fn container_matches(container: Option<&str>, name: &str) -> bool {
    match container {
        Some(c) => c == name || c.ends_with(&format!(".{name}")),
        None => false,
    }
}

fn visibility_of(name: &str) -> String {
    if name.starts_with('_') && !name.starts_with("__") {
        "private".to_string()
    } else {
        "public".to_string()
    }
}

/// Callee text of a call: identifier, attribute (`a.b` -> `b`), else first line.
fn callee_text(function: Node, source: &str) -> String {
    match function.kind() {
        "identifier" => node_text(function, source),
        "attribute" => function
            .child_by_field_name("attribute")
            .map(|a| node_text(a, source))
            .unwrap_or_else(|| first_line(&node_text(function, source))),
        _ => first_line(&node_text(function, source)),
    }
}

// ---------------------------------------------------------------------------
// Variable / use-site collection
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct Decl {
    name: String,
    kind: VarKind,
    line: usize,
    scope_key: String,
    span: (usize, usize),
    enclosing: String,
    type_name: Option<String>,
}

#[derive(Debug, Clone)]
struct NameEvent {
    name: String,
    line: usize,
    col: usize,
    read: bool,
    write: bool,
    scope_key: String,
    is_field: bool,
}

struct Visitor<'a> {
    file: &'a str,
    source: &'a str,
    symbols: &'a [Symbol],
    file_lines: usize,
    decls: Vec<Decl>,
    events: Vec<NameEvent>,
    uses: Vec<UseSite>,
    types: Vec<TypeNode>,
    type_edges: Vec<TypeEdge>,
}

impl<'a> Visitor<'a> {
    fn nearest_ancestor<'t>(&self, node: Node<'t>, kind: &str) -> Option<Node<'t>> {
        let mut cur = node.parent();
        while let Some(n) = cur {
            if n.kind() == kind {
                return Some(n);
            }
            cur = n.parent();
        }
        None
    }

    /// Canonical `file:line` id for a declaration node, preferring the shared
    /// helper and falling back to the id convention for missing symbols.
    fn id_of(&self, node: Node) -> String {
        let line = node.start_position().row + 1;
        let id = enclosing_symbol(self.symbols, line);
        if id.is_empty() {
            format!("{}:{}", self.file, line)
        } else {
            id
        }
    }

    /// (kind, scope key, scope span, enclosing) for a declaration node.
    fn auto_scope(&self, node: Node) -> (VarKind, String, (usize, usize), String) {
        if let Some(f) = self.nearest_ancestor(node, "function_definition") {
            let id = self.id_of(f);
            let enc = self.fallback_enclosing(node, &id);
            (
                VarKind::Local,
                id,
                (f.start_position().row + 1, f.end_position().row + 1),
                enc,
            )
        } else if let Some(c) = self.nearest_ancestor(node, "class_definition") {
            let id = self.id_of(c);
            let enc = self.fallback_enclosing(node, &id);
            (
                VarKind::Field,
                id,
                (c.start_position().row + 1, c.end_position().row + 1),
                enc,
            )
        } else {
            let line = node.start_position().row + 1;
            (
                VarKind::Global,
                String::new(),
                (1, self.file_lines),
                enclosing_symbol(self.symbols, line),
            )
        }
    }

    fn fallback_enclosing(&self, node: Node, computed: &str) -> String {
        let line = node.start_position().row + 1;
        let id = enclosing_symbol(self.symbols, line);
        if id.is_empty() {
            computed.to_string()
        } else {
            id
        }
    }

    fn resolve_symbol(&self, name: &str) -> Option<String> {
        self.symbols
            .iter()
            .find(|s| s.name == name && s.file == self.file)
            .or_else(|| self.symbols.iter().find(|s| s.name == name))
            .map(|s| s.id.clone())
    }

    fn push_use(&mut self, target: String, line: usize, col: usize, kind: UseKind) {
        self.uses.push(UseSite {
            target,
            file: self.file.to_string(),
            line,
            col,
            kind,
            snippet: snippet(self.source, line),
        });
    }

    fn record_call(&mut self, call: Node) {
        let Some(function) = call.child_by_field_name("function") else {
            return;
        };
        let callee = callee_text(function, self.source);
        if callee.is_empty() {
            return;
        }
        let target = self.resolve_symbol(&callee).unwrap_or(callee);
        let line = call.start_position().row + 1;
        let col = call.start_position().column + 1;
        self.push_use(target, line, col, UseKind::Call);
    }

    fn record_type_refs(&mut self, type_node: Node) {
        let mut stack = vec![type_node];
        while let Some(n) = stack.pop() {
            if n.kind() == "identifier" {
                let name = node_text(n, self.source);
                if !name.is_empty() {
                    let target = self.resolve_symbol(&name).unwrap_or(name);
                    let line = n.start_position().row + 1;
                    let col = n.start_position().column + 1;
                    self.push_use(target, line, col, UseKind::TypeRef);
                }
            }
            let mut cursor = n.walk();
            for k in n.named_children(&mut cursor) {
                stack.push(k);
            }
        }
    }

    fn handle_ident_target(&mut self, node: Node, line: usize, plain: bool, ann: Option<&str>) {
        let (kind, scope_key, span, enc) = self.auto_scope(node);
        let name = node_text(node, self.source);
        if name.is_empty() {
            return;
        }
        self.decls.push(Decl {
            name: name.clone(),
            kind,
            line,
            scope_key: scope_key.clone(),
            span,
            enclosing: enc,
            type_name: ann.map(str::to_string),
        });
        self.events.push(NameEvent {
            name,
            line,
            col: node.start_position().column + 1,
            read: !plain,
            write: true,
            scope_key,
            is_field: false,
        });
    }

    fn handle_ident_read(&mut self, node: Node) {
        let name = node_text(node, self.source);
        if name.is_empty() {
            return;
        }
        let (_, scope_key, _, _) = self.auto_scope(node);
        self.events.push(NameEvent {
            name,
            line: node.start_position().row + 1,
            col: node.start_position().column + 1,
            read: true,
            write: false,
            scope_key,
            is_field: false,
        });
    }

    /// `self.x` field handling: declaration/event for reads and writes; the
    /// generated use-site comes from the event as a `FieldAccess`.
    fn handle_self_attribute(
        &mut self,
        node: Node,
        attr: Node,
        target: bool,
        rw: bool,
        ann: Option<&str>,
    ) {
        let Some(class) = self.nearest_ancestor(node, "class_definition") else {
            return;
        };
        let class_id = self.id_of(class);
        let span = (class.start_position().row + 1, class.end_position().row + 1);
        let name = node_text(attr, self.source);
        if name.is_empty() {
            return;
        }
        let line = node.start_position().row + 1;
        let col = node.start_position().column + 1;
        if target {
            self.decls.push(Decl {
                name: name.clone(),
                kind: VarKind::Field,
                line,
                scope_key: class_id.clone(),
                span,
                enclosing: self.fallback_enclosing(node, &class_id),
                type_name: ann.map(str::to_string),
            });
            self.events.push(NameEvent {
                name,
                line,
                col,
                read: rw,
                write: true,
                scope_key: class_id,
                is_field: true,
            });
        } else {
            self.events.push(NameEvent {
                name,
                line,
                col,
                read: true,
                write: false,
                scope_key: class_id,
                is_field: true,
            });
        }
    }

    fn handle_attribute(&mut self, node: Node, target: bool, rw: bool, ann: Option<&str>) {
        let (Some(obj), Some(attr)) = (
            node.child_by_field_name("object"),
            node.child_by_field_name("attribute"),
        ) else {
            return;
        };
        if node_text(obj, self.source) == "self" {
            self.handle_self_attribute(node, attr, target, rw, ann);
            return;
        }
        // `obj.attr`: a field access; the object expression is still read.
        let name = node_text(attr, self.source);
        if !name.is_empty() {
            let target_id = self.resolve_symbol(&name).unwrap_or(name);
            let line = node.start_position().row + 1;
            let col = node.start_position().column + 1;
            self.push_use(target_id, line, col, UseKind::FieldAccess);
        }
        self.visit(obj);
    }

    fn handle_target(&mut self, node: Node, line: usize, plain: bool, ann: Option<&str>) {
        match node.kind() {
            "identifier" => self.handle_ident_target(node, line, plain, ann),
            "attribute" => self.handle_attribute(node, true, !plain, ann),
            "as_pattern_target" => {
                if let Some(c) = node.named_child(0) {
                    self.handle_target(c, line, plain, ann);
                }
            }
            "pattern_list"
            | "tuple_pattern"
            | "list_pattern"
            | "tuple"
            | "list"
            | "parenthesized_expression"
            | "expression_list" => {
                let mut cursor = node.walk();
                for c in node.named_children(&mut cursor) {
                    self.handle_target(c, line, plain, None);
                }
            }
            "list_splat_pattern"
            | "dictionary_splat_pattern"
            | "list_splat"
            | "dictionary_splat" => {
                if let Some(c) = node.named_child(0) {
                    self.handle_target(c, line, plain, None);
                }
            }
            _ => self.visit(node),
        }
    }

    fn collect_params(&mut self, params: Node) {
        let mut cursor = params.walk();
        for p in params.named_children(&mut cursor) {
            match p.kind() {
                "identifier" => self.declare_param(p, None),
                "typed_parameter" => {
                    let ty = p
                        .child_by_field_name("type")
                        .map(|t| first_line(&node_text(t, self.source)));
                    if let Some(t) = p.child_by_field_name("type") {
                        self.record_type_refs(t);
                    }
                    if let Some(id) = param_name_node(p) {
                        self.declare_param(id, ty.as_deref());
                    }
                }
                "default_parameter" => {
                    if let Some(name) = p
                        .child_by_field_name("name")
                        .filter(|n| n.kind() == "identifier")
                    {
                        self.declare_param(name, None);
                    }
                    if let Some(v) = p.child_by_field_name("value") {
                        self.visit(v);
                    }
                }
                "typed_default_parameter" => {
                    let ty = p
                        .child_by_field_name("type")
                        .map(|t| first_line(&node_text(t, self.source)));
                    if let Some(t) = p.child_by_field_name("type") {
                        self.record_type_refs(t);
                    }
                    if let Some(name) = p
                        .child_by_field_name("name")
                        .filter(|n| n.kind() == "identifier")
                    {
                        self.declare_param(name, ty.as_deref());
                    }
                    if let Some(v) = p.child_by_field_name("value") {
                        self.visit(v);
                    }
                }
                "list_splat_pattern" | "dictionary_splat_pattern" => {
                    if let Some(id) = p.named_child(0).filter(|n| n.kind() == "identifier") {
                        self.declare_param(id, None);
                    }
                }
                _ => {}
            }
        }
    }

    fn declare_param(&mut self, id_node: Node, ann: Option<&str>) {
        let name = node_text(id_node, self.source);
        if name.is_empty() {
            return;
        }
        let (scope_key, span) = match self.nearest_ancestor(id_node, "function_definition") {
            Some(f) => (
                self.id_of(f),
                (f.start_position().row + 1, f.end_position().row + 1),
            ),
            None => (String::new(), (1, self.file_lines)),
        };
        let enc = self.fallback_enclosing(id_node, &scope_key);
        self.decls.push(Decl {
            name,
            kind: VarKind::Param,
            line: id_node.start_position().row + 1,
            scope_key,
            span,
            enclosing: enc,
            type_name: ann.map(str::to_string),
        });
    }

    fn visit(&mut self, node: Node) {
        match node.kind() {
            "function_definition" => {
                if let Some(params) = node.child_by_field_name("parameters") {
                    self.collect_params(params);
                }
                if let Some(rt) = node.child_by_field_name("return_type") {
                    self.record_type_refs(rt);
                }
                if let Some(body) = node.child_by_field_name("body") {
                    self.visit(body);
                }
            }
            "class_definition" => {
                if let Some(body) = node.child_by_field_name("body") {
                    self.visit(body);
                }
            }
            "lambda" => {
                if let Some(body) = node.child_by_field_name("body") {
                    self.visit(body);
                }
            }
            "assignment" => {
                let line = node.start_position().row + 1;
                let ann = node
                    .child_by_field_name("type")
                    .map(|t| first_line(&node_text(t, self.source)));
                if let Some(left) = node.child_by_field_name("left") {
                    self.handle_target(left, line, true, ann.as_deref());
                }
                if let Some(ty) = node.child_by_field_name("type") {
                    self.record_type_refs(ty);
                }
                if let Some(right) = node.child_by_field_name("right") {
                    self.visit(right);
                }
            }
            "augmented_assignment" => {
                let line = node.start_position().row + 1;
                if let Some(left) = node.child_by_field_name("left") {
                    self.handle_target(left, line, false, None);
                }
                if let Some(right) = node.child_by_field_name("right") {
                    self.visit(right);
                }
            }
            "for_statement" => {
                let line = node.start_position().row + 1;
                if let Some(left) = node.child_by_field_name("left") {
                    self.handle_target(left, line, true, None);
                }
                if let Some(right) = node.child_by_field_name("right") {
                    self.visit(right);
                }
                if let Some(body) = node.child_by_field_name("body") {
                    self.visit(body);
                }
                if let Some(alt) = node.child_by_field_name("alternative") {
                    self.visit(alt);
                }
            }
            "as_pattern" => {
                let line = node.start_position().row + 1;
                let alias = node.child_by_field_name("alias");
                let alias_id = alias.map(|a| a.id());
                let mut cursor = node.walk();
                for c in node.named_children(&mut cursor) {
                    if Some(c.id()) != alias_id {
                        self.visit(c);
                    }
                }
                if let Some(a) = alias {
                    self.handle_target(a, line, true, None);
                }
            }
            "except_clause" => {
                let line = node.start_position().row + 1;
                if let Some(v) = node.child_by_field_name("value") {
                    self.visit(v);
                }
                if let Some(a) = node.child_by_field_name("alias") {
                    self.handle_target(a, line, true, None);
                }
                if let Some(b) = find_child_kind(node, "block") {
                    self.visit(b);
                }
            }
            "call" => {
                if has_ancestor_kind(node, "decorator") {
                    return;
                }
                self.record_call(node);
                if let Some(function) = node
                    .child_by_field_name("function")
                    .filter(|f| f.kind() != "identifier")
                {
                    self.visit(function);
                }
                if let Some(args) = node.child_by_field_name("arguments") {
                    self.visit(args);
                }
            }
            "attribute" => self.handle_attribute(node, false, false, None),
            "identifier" => self.handle_ident_read(node),
            "keyword_argument" => {
                if let Some(v) = node.child_by_field_name("value") {
                    self.visit(v);
                }
            }
            "type" => self.record_type_refs(node),
            "parameters"
            | "import_statement"
            | "import_from_statement"
            | "future_import_statement"
            | "global_statement"
            | "nonlocal_statement"
            | "comment" => {}
            _ => {
                let mut cursor = node.walk();
                for c in node.named_children(&mut cursor) {
                    self.visit(c);
                }
            }
        }
    }

    // -- type nodes ---------------------------------------------------------

    fn collect_types(&mut self, root: Node) {
        let mut classes: Vec<Node> = Vec::new();
        all_of_kind(root, "class_definition", &mut classes);
        if classes.is_empty() {
            return;
        }
        let mut declared: BTreeMap<String, String> = BTreeMap::new();
        for c in &classes {
            if let Some(name_node) = c.child_by_field_name("name") {
                let name = node_text(name_node, self.source);
                if !name.is_empty() {
                    declared.insert(
                        name,
                        format!("{}:{}", self.file, c.start_position().row + 1),
                    );
                }
            }
        }
        for c in &classes {
            let Some(name_node) = c.child_by_field_name("name") else {
                continue;
            };
            let name = node_text(name_node, self.source);
            if name.is_empty() {
                continue;
            }
            let line = c.start_position().row + 1;
            let id = format!("{}:{}", self.file, line);
            let doc = self.class_doc(*c, line);
            let methods = self.class_methods(*c, &name);
            let fields = self.class_fields(*c);
            let mut edges: Vec<TypeEdge> = Vec::new();

            if let Some(sup) = c.child_by_field_name("superclasses") {
                let mut cursor = sup.walk();
                for a in sup.named_children(&mut cursor) {
                    if a.kind() == "keyword_argument" {
                        continue;
                    }
                    let base = base_type_name(&node_text(a, self.source));
                    if base.is_empty() {
                        continue;
                    }
                    let to = declared.get(&base).cloned().unwrap_or_else(|| base.clone());
                    edges.push(TypeEdge {
                        from: id.clone(),
                        to: to.clone(),
                        kind: TypeEdgeKind::Inherit,
                        line,
                    });
                    let col = a.start_position().column + 1;
                    self.push_use(to, line, col, UseKind::TypeRef);
                }
            }
            for f in &fields {
                if f.type_name.is_empty() {
                    continue;
                }
                let base = base_type_name(&f.type_name);
                if let Some(to) = declared.get(&base) {
                    edges.push(TypeEdge {
                        from: id.clone(),
                        to: to.clone(),
                        kind: TypeEdgeKind::Field,
                        line: f.line,
                    });
                }
            }
            self.types.push(TypeNode {
                id,
                name,
                kind: TypeKind::Class,
                file: self.file.to_string(),
                line,
                fields,
                methods,
                doc,
                module_path: module_path_of(self.file),
            });
            self.type_edges.extend(edges);
        }
    }

    fn class_doc(&self, class_node: Node, line: usize) -> String {
        if let Some(s) = self.symbols.iter().find(|s| {
            s.kind == SymbolKind::Class
                && s.file == self.file
                && s.start_line == line
                && !s.doc.is_empty()
        }) {
            return s.doc.clone();
        }
        docstring_of(class_node, self.source)
    }

    fn class_methods(&self, class_node: Node, name: &str) -> Vec<String> {
        let mut methods: Vec<String> = self
            .symbols
            .iter()
            .filter(|s| {
                s.kind == SymbolKind::Method && container_matches(s.container.as_deref(), name)
            })
            .map(|s| s.name.clone())
            .collect();
        // Fallback: direct defs in the class body (when symbols are absent).
        if methods.is_empty() {
            if let Some(body) = class_node.child_by_field_name("body") {
                let mut cursor = body.walk();
                for child in body.named_children(&mut cursor) {
                    let def = if child.kind() == "decorated_definition" {
                        child.child_by_field_name("definition")
                    } else {
                        Some(child)
                    };
                    if let Some(d) = def {
                        if d.kind() == "function_definition" {
                            if let Some(n) = d.child_by_field_name("name") {
                                methods.push(node_text(n, self.source));
                            }
                        }
                    }
                }
            }
        }
        methods.sort();
        methods.dedup();
        methods
    }

    fn class_fields(&self, class_node: Node) -> Vec<Field> {
        let mut found: BTreeMap<String, Field> = BTreeMap::new();
        let mut stack = vec![class_node];
        while let Some(n) = stack.pop() {
            if n.id() != class_node.id() && n.kind() == "class_definition" {
                continue; // nested class owns its own fields
            }
            if n.kind() == "assignment" {
                if let Some((fname, ftype, fline)) = self.class_assign_field(n, class_node) {
                    let visibility = visibility_of(&fname);
                    found.entry(fname.clone()).or_insert(Field {
                        name: fname,
                        type_name: ftype,
                        visibility,
                        line: fline,
                    });
                }
            }
            let mut cursor = n.walk();
            for k in n.named_children(&mut cursor) {
                stack.push(k);
            }
        }
        found.into_values().collect()
    }

    fn class_assign_field(&self, node: Node, class_node: Node) -> Option<(String, String, usize)> {
        let left = node.child_by_field_name("left")?;
        let ty = node
            .child_by_field_name("type")
            .map(|t| first_line(&node_text(t, self.source)))
            .unwrap_or_default();
        let line = node.start_position().row + 1;
        if left.kind() == "attribute" {
            let obj = left.child_by_field_name("object")?;
            let attr = left.child_by_field_name("attribute")?;
            if node_text(obj, self.source) != "self" {
                return None;
            }
            let owner = self.nearest_ancestor(left, "class_definition")?;
            if owner.id() != class_node.id() {
                return None;
            }
            let fname = node_text(attr, self.source);
            if fname.is_empty() {
                return None;
            }
            return Some((fname, ty, line));
        }
        if left.kind() == "identifier" {
            if self.nearest_ancestor(left, "function_definition").is_some() {
                return None; // a method-local, not a class attribute
            }
            let owner = self.nearest_ancestor(left, "class_definition")?;
            if owner.id() != class_node.id() {
                return None;
            }
            let fname = node_text(left, self.source);
            if fname.is_empty() {
                return None;
            }
            return Some((fname, ty, line));
        }
        None
    }

    // -- finish: variables + variable use sites -----------------------------

    fn finish(mut self) -> (Vec<Variable>, Vec<UseSite>) {
        let mut grouped: BTreeMap<(String, String), Decl> = BTreeMap::new();
        for d in &self.decls {
            let key = (d.scope_key.clone(), d.name.clone());
            grouped
                .entry(key)
                .and_modify(|e| {
                    if d.line < e.line {
                        e.line = d.line;
                        e.span = d.span;
                        e.enclosing = d.enclosing.clone();
                    }
                    if e.type_name.is_none() {
                        e.type_name = d.type_name.clone();
                    }
                })
                .or_insert_with(|| d.clone());
        }

        let mut variables: Vec<Variable> = Vec::new();
        let mut targets: HashMap<(String, String), String> = HashMap::new();
        for ((scope_key, name), d) in &grouped {
            let decl_line = d.line;
            let mut reads = 0usize;
            let mut writes = 0usize;
            let mut first_use: Option<usize> = None;
            let mut last_use: Option<usize> = None;
            for e in &self.events {
                if &e.scope_key != scope_key || &e.name != name {
                    continue;
                }
                if e.read {
                    reads += 1;
                }
                if e.write {
                    writes += 1;
                }
                let decl_store = e.line == decl_line && e.write && !e.read;
                if !decl_store {
                    first_use = Some(first_use.map_or(e.line, |f| f.min(e.line)));
                    last_use = Some(last_use.map_or(e.line, |l| l.max(e.line)));
                }
            }
            let id = format!("{}:{}:{}", self.file, decl_line, name);
            targets.insert((scope_key.clone(), name.clone()), id.clone());
            variables.push(Variable {
                id,
                name: name.clone(),
                kind: d.kind,
                file: self.file.to_string(),
                decl_line,
                scope_start: d.span.0,
                scope_end: d.span.1,
                type_name: d.type_name.clone(),
                reads,
                writes,
                first_use,
                last_use,
                enclosing: d.enclosing.clone(),
            });
        }
        variables
            .sort_by(|a, b| (a.decl_line, a.name.as_str()).cmp(&(b.decl_line, b.name.as_str())));

        let mut uses = std::mem::take(&mut self.uses);
        for e in &self.events {
            let key = (e.scope_key.clone(), e.name.clone());
            let target = targets
                .get(&key)
                .cloned()
                .or_else(|| self.resolve_symbol(&e.name))
                .unwrap_or_else(|| e.name.clone());
            let kind = if e.is_field {
                UseKind::FieldAccess
            } else if e.write {
                UseKind::VarWrite
            } else {
                UseKind::VarRead
            };
            uses.push(UseSite {
                target,
                file: self.file.to_string(),
                line: e.line,
                col: e.col,
                kind,
                snippet: snippet(self.source, e.line),
            });
        }
        uses.sort_by(|a, b| {
            (a.line, a.col, a.target.as_str()).cmp(&(b.line, b.col, b.target.as_str()))
        });

        (variables, uses)
    }
}

fn param_name_node<'t>(p: Node<'t>) -> Option<Node<'t>> {
    let mut cursor = p.walk();
    let mut it = p.named_children(&mut cursor);
    it.find(|c| {
        matches!(
            c.kind(),
            "identifier" | "list_splat_pattern" | "dictionary_splat_pattern"
        )
    })
}

// ---------------------------------------------------------------------------
// Control-flow graphs
// ---------------------------------------------------------------------------

struct CfgBuilder<'a> {
    source: &'a str,
    nodes: Vec<CfgNode>,
    edges: Vec<CfgEdge>,
    next: u32,
}

impl<'a> CfgBuilder<'a> {
    fn new(source: &'a str) -> Self {
        Self {
            source,
            nodes: Vec::new(),
            edges: Vec::new(),
            next: 0,
        }
    }

    fn node(&mut self, kind: CfgKind, start: usize, end: usize, label: String) -> u32 {
        let id = self.next;
        self.next += 1;
        self.nodes.push(CfgNode {
            id,
            kind,
            start_line: start,
            end_line: end,
            label,
        });
        id
    }

    fn edge(&mut self, from: u32, to: u32, kind: CfgEdgeKind, line: usize) {
        self.edges.push(CfgEdge {
            from,
            to,
            kind,
            line,
        });
    }

    fn build_block(&mut self, block: Node) -> (u32, Vec<u32>) {
        let mut cursor = block.walk();
        let mut stmts = block.named_children(&mut cursor).peekable();
        if stmts.peek().is_none() {
            let line = block.start_position().row + 1;
            let end = block.end_position().row + 1;
            let n = self.node(CfgKind::Block, line, end, "pass".to_string());
            return (n, vec![n]);
        }
        let mut header: Option<u32> = None;
        let mut tails: Vec<u32> = Vec::new();
        for stmt in stmts {
            let (entry, t) = self.build_stmt(stmt);
            let sline = stmt.start_position().row + 1;
            if header.is_none() {
                header = Some(entry);
            } else {
                for p in &tails {
                    self.edge(*p, entry, CfgEdgeKind::Normal, sline);
                }
            }
            tails = t;
            if tails.is_empty() {
                break;
            }
        }
        (header.unwrap_or(0), tails)
    }

    fn build_stmt(&mut self, stmt: Node) -> (u32, Vec<u32>) {
        let line = stmt.start_position().row + 1;
        let end = stmt.end_position().row + 1;
        match stmt.kind() {
            "if_statement" => self.build_if(stmt, line, end),
            "for_statement" => self.build_loop(stmt, line, end, true),
            "while_statement" => self.build_loop(stmt, line, end, false),
            "try_statement" => self.build_try(stmt, line, end),
            "with_statement" => self.build_with(stmt, line, end),
            "match_statement" => self.build_match(stmt, line, end),
            "return_statement" => {
                let label = label_of(stmt, self.source);
                let n = self.node(CfgKind::Return, line, end, label);
                (n, vec![n])
            }
            "expression_statement" => {
                if let Some(inner) = stmt.named_child(0) {
                    if inner.kind() == "call" {
                        let callee = inner
                            .child_by_field_name("function")
                            .map(|f| callee_text(f, self.source))
                            .unwrap_or_default();
                        let n = self.node(CfgKind::Call, line, end, callee);
                        return (n, vec![n]);
                    }
                }
                let label = label_of(stmt, self.source);
                let n = self.node(CfgKind::Block, line, end, label);
                (n, vec![n])
            }
            _ => {
                let label = label_of(stmt, self.source);
                let n = self.node(CfgKind::Block, line, end, label);
                (n, vec![n])
            }
        }
    }

    fn build_if(&mut self, stmt: Node, line: usize, end: usize) -> (u32, Vec<u32>) {
        let cond = stmt
            .child_by_field_name("condition")
            .map(|c| label_of(c, self.source))
            .unwrap_or_default();
        let branch = self.node(CfgKind::Branch, line, line, cond);
        let mut tails: Vec<u32> = Vec::new();
        if let Some(cons) = stmt.child_by_field_name("consequence") {
            let (e, t) = self.build_block(cons);
            self.edge(branch, e, CfgEdgeKind::True, line);
            tails.extend(t);
        }
        let mut last_cond = Some(branch);
        let mut cursor = stmt.walk();
        for alt in stmt.children_by_field_name("alternative", &mut cursor) {
            let aline = alt.start_position().row + 1;
            match alt.kind() {
                "elif_clause" => {
                    let c2 = alt
                        .child_by_field_name("condition")
                        .map(|c| label_of(c, self.source))
                        .unwrap_or_default();
                    let b2 = self.node(CfgKind::Branch, aline, aline, c2);
                    if let Some(lc) = last_cond {
                        self.edge(lc, b2, CfgEdgeKind::False, aline);
                    }
                    if let Some(cons) = alt.child_by_field_name("consequence") {
                        let (e, t) = self.build_block(cons);
                        self.edge(b2, e, CfgEdgeKind::True, aline);
                        tails.extend(t);
                    }
                    last_cond = Some(b2);
                }
                "else_clause" => {
                    if let Some(body) = alt.child_by_field_name("body") {
                        let (e, t) = self.build_block(body);
                        if let Some(lc) = last_cond {
                            self.edge(lc, e, CfgEdgeKind::False, aline);
                        }
                        tails.extend(t);
                    }
                    last_cond = None;
                }
                _ => {}
            }
        }
        let merge = self.node(CfgKind::Block, line, end, "merge".to_string());
        for t in tails {
            self.edge(t, merge, CfgEdgeKind::Normal, end);
        }
        if let Some(lc) = last_cond {
            self.edge(lc, merge, CfgEdgeKind::False, line);
        }
        (branch, vec![merge])
    }

    fn build_loop(
        &mut self,
        stmt: Node,
        line: usize,
        _end: usize,
        is_for: bool,
    ) -> (u32, Vec<u32>) {
        let head = if is_for {
            let left = stmt
                .child_by_field_name("left")
                .map(|n| first_line(&node_text(n, self.source)))
                .unwrap_or_default();
            let right = stmt
                .child_by_field_name("right")
                .map(|n| first_line(&node_text(n, self.source)))
                .unwrap_or_default();
            format!("for {left} in {right}")
        } else {
            let c = stmt
                .child_by_field_name("condition")
                .map(|n| label_of(n, self.source))
                .unwrap_or_default();
            format!("while {c}")
        };
        let head: String = head.chars().take(60).collect();
        let loopn = self.node(CfgKind::Loop, line, line, head);
        let mut exit = loopn;
        if let Some(body) = stmt.child_by_field_name("body") {
            let (e, t) = self.build_block(body);
            self.edge(loopn, e, CfgEdgeKind::True, line);
            for x in t {
                self.edge(x, loopn, CfgEdgeKind::Back, line);
            }
        }
        if let Some(alt) = stmt.child_by_field_name("alternative") {
            let aline = alt.start_position().row + 1;
            if let Some(body) = alt.child_by_field_name("body") {
                let (e, t) = self.build_block(body);
                self.edge(loopn, e, CfgEdgeKind::Normal, aline);
                let merge = self.node(
                    CfgKind::Block,
                    aline,
                    alt.end_position().row + 1,
                    "merge".to_string(),
                );
                for x in t {
                    self.edge(x, merge, CfgEdgeKind::Normal, aline);
                }
                exit = merge;
            }
        }
        (loopn, vec![exit])
    }

    fn build_try(&mut self, stmt: Node, line: usize, end: usize) -> (u32, Vec<u32>) {
        let tryn = self.node(CfgKind::Block, line, end, "try".to_string());
        let mut tails: Vec<u32> = Vec::new();
        if let Some(body) = stmt.child_by_field_name("body") {
            let (e, t) = self.build_block(body);
            self.edge(tryn, e, CfgEdgeKind::Normal, line);
            tails.extend(t);
        }
        let mut cursor = stmt.walk();
        for child in stmt.named_children(&mut cursor) {
            let cline = child.start_position().row + 1;
            match child.kind() {
                "except_clause" => {
                    let label = child
                        .child_by_field_name("value")
                        .map(|v| first_line(&node_text(v, self.source)))
                        .unwrap_or_else(|| "except".to_string());
                    let br = self.node(CfgKind::Branch, cline, cline, label);
                    self.edge(tryn, br, CfgEdgeKind::Normal, cline);
                    if let Some(b) = find_child_kind(child, "block") {
                        let (e, t) = self.build_block(b);
                        self.edge(br, e, CfgEdgeKind::Normal, cline);
                        tails.extend(t);
                    }
                }
                "else_clause" | "finally_clause" => {
                    if let Some(b) = find_child_kind(child, "block") {
                        let (e, t) = self.build_block(b);
                        self.edge(tryn, e, CfgEdgeKind::Normal, cline);
                        tails.extend(t);
                    }
                }
                _ => {}
            }
        }
        let merge = self.node(CfgKind::Block, line, end, "merge".to_string());
        if tails.is_empty() {
            self.edge(tryn, merge, CfgEdgeKind::Normal, line);
        }
        for t in tails {
            self.edge(t, merge, CfgEdgeKind::Normal, end);
        }
        (tryn, vec![merge])
    }

    fn build_with(&mut self, stmt: Node, line: usize, end: usize) -> (u32, Vec<u32>) {
        let n = self.node(CfgKind::Block, line, end, label_of(stmt, self.source));
        if let Some(body) = stmt.child_by_field_name("body") {
            let (e, t) = self.build_block(body);
            self.edge(n, e, CfgEdgeKind::Normal, line);
            (n, t)
        } else {
            (n, vec![n])
        }
    }

    fn build_match(&mut self, stmt: Node, line: usize, end: usize) -> (u32, Vec<u32>) {
        let subject = stmt
            .child_by_field_name("subject")
            .map(|s| label_of(s, self.source))
            .unwrap_or_default();
        let m = self.node(CfgKind::Match, line, line, format!("match {subject}"));
        let mut tails: Vec<u32> = Vec::new();
        if let Some(body) = stmt.child_by_field_name("body") {
            let mut cursor = body.walk();
            for case in body.named_children(&mut cursor) {
                if case.kind() != "case_clause" {
                    continue;
                }
                let cline = case.start_position().row + 1;
                let br = self.node(CfgKind::Branch, cline, cline, label_of(case, self.source));
                self.edge(m, br, CfgEdgeKind::Normal, cline);
                if let Some(cons) = case.child_by_field_name("consequence") {
                    let (e, t) = self.build_block(cons);
                    self.edge(br, e, CfgEdgeKind::Normal, cline);
                    tails.extend(t);
                }
            }
        }
        let merge = self.node(CfgKind::Block, line, end, "merge".to_string());
        if tails.is_empty() {
            self.edge(m, merge, CfgEdgeKind::Normal, line);
        }
        for t in tails {
            self.edge(t, merge, CfgEdgeKind::Normal, end);
        }
        (m, vec![merge])
    }
}

fn collect_cfgs(file: &str, source: &str, root: Node) -> Vec<FunctionCfg> {
    let mut funcs: Vec<Node> = Vec::new();
    all_of_kind(root, "function_definition", &mut funcs);
    let mut out: Vec<FunctionCfg> = Vec::new();
    for f in funcs {
        let start = f.start_position().row + 1;
        let end = f.end_position().row + 1;
        let symbol = format!("{file}:{start}");
        let mut b = CfgBuilder::new(source);
        let entry = b.node(CfgKind::Entry, start, start, "entry".to_string());
        let tails = if let Some(body) = f.child_by_field_name("body") {
            let (e, t) = b.build_block(body);
            b.edge(entry, e, CfgEdgeKind::Normal, start);
            t
        } else {
            vec![entry]
        };
        let exit = b.node(CfgKind::Exit, end, end, "exit".to_string());
        for t in &tails {
            b.edge(*t, exit, CfgEdgeKind::Normal, end);
        }
        let (cyclomatic, max_nesting) = complexity(f);
        out.push(FunctionCfg {
            symbol,
            nodes: b.nodes,
            edges: b.edges,
            cyclomatic,
            max_nesting,
        });
    }
    out.sort_by(|a, b| a.symbol.cmp(&b.symbol));
    out
}

/// `(cyclomatic, max_nesting)` for one function. Decisions: if/elif, for/while,
/// except handlers, match cases and boolean operators; nested defs don't leak in.
fn complexity(func: Node) -> (u32, u32) {
    fn rec(n: Node, depth: u32, decisions: &mut u32, maxn: &mut u32) {
        let kind = n.kind();
        if matches!(
            kind,
            "if_statement"
                | "elif_clause"
                | "for_statement"
                | "while_statement"
                | "except_clause"
                | "case_clause"
                | "boolean_operator"
        ) {
            *decisions += 1;
        }
        let mut d = depth;
        if matches!(
            kind,
            "if_statement"
                | "for_statement"
                | "while_statement"
                | "try_statement"
                | "match_statement"
                | "with_statement"
        ) {
            d = depth + 1;
            if d > *maxn {
                *maxn = d;
            }
        }
        if matches!(kind, "function_definition" | "class_definition" | "lambda") {
            return;
        }
        let mut cursor = n.walk();
        for c in n.named_children(&mut cursor) {
            rec(c, d, decisions, maxn);
        }
    }
    let mut decisions = 0u32;
    let mut maxn = 0u32;
    if let Some(body) = func.child_by_field_name("body") {
        rec(body, 0, &mut decisions, &mut maxn);
    }
    (decisions + 1, maxn)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse_py;

    fn analyze_src(src: &str) -> Analysis {
        let (syms, _, _) = parse_py::extract(src, "t.py");
        let mut out = Analysis::new();
        analyze("t.py", src, &syms, &mut out);
        out
    }

    #[test]
    fn for_and_if_have_expected_cyclomatic_and_back_edge() {
        let src = concat!(
            "def f(xs):\n",
            "    total = 0\n",
            "    for x in xs:\n",
            "        if x > 0:\n",
            "            total = total + x\n",
            "    return total\n",
        );
        let out = analyze_src(src);
        let cfg = out
            .cfgs
            .iter()
            .find(|c| c.symbol == "t.py:1")
            .expect("cfg for f");
        assert_eq!(cfg.cyclomatic, 3, "for(+1) if(+1) + 1");
        assert!(
            cfg.edges.iter().any(|e| e.kind == CfgEdgeKind::Back),
            "loop body must have a Back edge: {:?}",
            cfg.edges
        );
        assert!(cfg.max_nesting >= 2, "for/if nesting: {}", cfg.max_nesting);
    }

    #[test]
    fn unused_assignment_has_zero_reads_used_has_some() {
        let src = concat!(
            "def g():\n",
            "    unused = 1\n",
            "    used = 2\n",
            "    return used\n",
        );
        let out = analyze_src(src);
        let unused = out
            .variables
            .iter()
            .find(|v| v.name == "unused")
            .expect("unused variable");
        assert_eq!(unused.reads, 0, "declaration-only variable has no reads");
        assert_eq!(unused.writes, 1);
        assert_eq!(unused.first_use, None);
        assert_eq!(unused.kind, VarKind::Local);
        assert_eq!(unused.id, "t.py:2:unused");

        let used = out
            .variables
            .iter()
            .find(|v| v.name == "used")
            .expect("used variable");
        assert!(used.reads >= 1, "used variable is read");
        assert_eq!(used.writes, 1);
        assert_eq!(used.first_use, Some(4));
    }

    #[test]
    fn class_yields_type_node_methods_and_inherit_edge() {
        let src = concat!(
            "class Base:\n",
            "    pass\n",
            "\n",
            "class Derived(Base):\n",
            "    \"\"\"A derived thing.\"\"\"\n",
            "\n",
            "    def __init__(self):\n",
            "        self.value = 1\n",
            "\n",
            "    def method(self):\n",
            "        return self.value\n",
        );
        let out = analyze_src(src);
        let derived = out
            .types
            .iter()
            .find(|t| t.name == "Derived")
            .expect("Derived TypeNode");
        assert_eq!(derived.kind, TypeKind::Class);
        assert_eq!(derived.doc, "A derived thing.");
        assert!(
            derived.methods.contains(&"method".to_string()),
            "methods: {:?}",
            derived.methods
        );
        assert!(derived.methods.contains(&"__init__".to_string()));
        assert!(
            derived.fields.iter().any(|f| f.name == "value"),
            "fields: {:?}",
            derived.fields
        );

        let base = out
            .types
            .iter()
            .find(|t| t.name == "Base")
            .expect("Base TypeNode");
        assert!(
            out.type_edges
                .iter()
                .any(|e| e.kind == TypeEdgeKind::Inherit
                    && e.from == derived.id
                    && e.to == base.id),
            "edges: {:?}",
            out.type_edges
        );
    }

    #[test]
    fn parameters_and_annotation_types_are_recorded() {
        let src = concat!(
            "class Foo:\n",
            "    pass\n",
            "\n",
            "def f(a: int, b, *args, c: Foo = None) -> str:\n",
            "    d: Foo = Foo()\n",
            "    return str(a)\n",
        );
        let out = analyze_src(src);
        let a = out.variables.iter().find(|v| v.name == "a").unwrap();
        assert_eq!(a.kind, VarKind::Param);
        assert_eq!(a.type_name.as_deref(), Some("int"));
        let c = out.variables.iter().find(|v| v.name == "c").unwrap();
        assert_eq!(c.type_name.as_deref(), Some("Foo"));
        let d = out.variables.iter().find(|v| v.name == "d").unwrap();
        assert_eq!(d.type_name.as_deref(), Some("Foo"));

        let foo = out.types.iter().find(|t| t.name == "Foo").unwrap();
        // `d: Foo` is a local, not a field -> no Field edge; base-class path only.
        assert!(!out
            .type_edges
            .iter()
            .any(|e| e.kind == TypeEdgeKind::Field && e.to == foo.id));
        // The annotation produces a TypeRef use for Foo.
        assert!(out
            .uses
            .iter()
            .any(|u| u.kind == UseKind::TypeRef && u.target == foo.id));
    }

    #[test]
    fn calls_fields_and_reads_are_use_sites() {
        let src = concat!(
            "class Greeter:\n",
            "    def hello(self, name):\n",
            "        msg = greet(name)\n",
            "        return msg.upper()\n",
        );
        let out = analyze_src(src);
        assert!(
            out.uses
                .iter()
                .any(|u| u.kind == UseKind::Call && u.target == "greet"),
            "call use: {:?}",
            out.uses
        );
        assert!(
            out.uses
                .iter()
                .any(|u| u.kind == UseKind::FieldAccess && u.target == "upper"),
            "field access use: {:?}",
            out.uses
        );
        let msg = out.variables.iter().find(|v| v.name == "msg").unwrap();
        assert!(msg.reads >= 1, "msg is read in .upper()");
    }

    #[test]
    fn malformed_input_never_panics() {
        let mut out = Analysis::new();
        analyze("bad.py", "def (:\n    x = = 1\n class", &[], &mut out);
        // No assertions beyond "did not panic"; empty symbol list is tolerated.
    }
}
