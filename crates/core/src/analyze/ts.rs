//! TypeScript/JavaScript analysis: variables, control flow, types, references (INTEL-4).
//!
//! Fills the frozen [`crate::analysis`] contract from tree-sitter-typescript /
//! tree-sitter-javascript. Uses the shared helpers in [`super`]
//! (`enclosing_symbol`, `snippet`).
//!
//! Grammar selection is by file extension: `.tsx` uses the TSX grammar (the plain
//! TS grammar cannot parse JSX), `.js`/`.jsx`/`.mjs`/`.cjs` use the JavaScript
//! grammar (which supports JSX), everything else uses the TypeScript grammar.
//! A single parser is built per file; any parse/grammar failure returns early
//! without panicking.
use super::{enclosing_symbol, snippet};
use crate::analysis::{
    Analysis, CfgEdge, CfgEdgeKind, CfgKind, CfgNode, Field, FunctionCfg, TypeEdge, TypeEdgeKind,
    TypeKind, TypeNode, UseKind, UseSite, VarKind, Variable,
};
use crate::model::Symbol;
use std::collections::HashSet;
use tree_sitter::{Node, Parser};

/// Append TS/JS variables/CFG/types/use-sites for `file` into `out`.
pub fn analyze(file: &str, source: &str, symbols: &[Symbol], out: &mut Analysis) {
    let language: tree_sitter::Language = pick_language(file);
    let mut parser = Parser::new();
    if parser.set_language(&language).is_err() {
        return;
    }
    let Some(tree) = parser.parse(source, None) else {
        return;
    };
    let root = tree.root_node();
    let mut analyzer = Analyzer::new(file, source, symbols);
    analyzer.walk(root);
    analyzer.finish(out);
}

/// Map a path to the grammar that can parse it.
fn pick_language(file: &str) -> tree_sitter::Language {
    let lower = file.to_ascii_lowercase();
    if lower.ends_with(".tsx") {
        tree_sitter_typescript::LANGUAGE_TSX.into()
    } else if lower.ends_with(".js")
        || lower.ends_with(".jsx")
        || lower.ends_with(".mjs")
        || lower.ends_with(".cjs")
    {
        tree_sitter_javascript::LANGUAGE.into()
    } else {
        tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()
    }
}

/// A declared variable plus the byte offset of its name (to exclude the decl
/// itself from its own read/write counts).
struct VarRec {
    var: Variable,
    name_byte: usize,
}

/// One identifier occurrence with its read/write role.
struct IdentUse {
    name: String,
    byte: usize,
    line: usize,
    col: usize,
    read: bool,
    write: bool,
}

/// Per-file accumulator. A single parse; everything else is a recursive walk.
struct Analyzer<'a> {
    file: &'a str,
    source: &'a str,
    symbols: &'a [Symbol],
    vars: Vec<VarRec>,
    cfgs: Vec<FunctionCfg>,
    types: Vec<TypeNode>,
    type_edges: Vec<TypeEdge>,
    /// `(owner id, base type name, line)` resolved against declared types at the end.
    pending_field_edges: Vec<(String, String, usize)>,
    uses: Vec<UseSite>,
    idents: Vec<IdentUse>,
}

impl<'a> Analyzer<'a> {
    fn new(file: &'a str, source: &'a str, symbols: &'a [Symbol]) -> Self {
        Self {
            file,
            source,
            symbols,
            vars: Vec::new(),
            cfgs: Vec::new(),
            types: Vec::new(),
            type_edges: Vec::new(),
            pending_field_edges: Vec::new(),
            uses: Vec::new(),
            idents: Vec::new(),
        }
    }

    fn text(&self, node: Node) -> &'a str {
        node.utf8_text(self.source.as_bytes()).unwrap_or("")
    }

    fn walk(&mut self, node: Node) {
        match node.kind() {
            "lexical_declaration" | "variable_declaration" => self.collect_declaration(node),
            "formal_parameters" => self.collect_params(node),
            "arrow_function" => {
                if let Some(p) = node.child_by_field_name("parameter") {
                    self.collect_param(p);
                }
            }
            "class_declaration" | "abstract_class_declaration" => self.collect_class(node),
            "interface_declaration" => self.collect_interface(node),
            "enum_declaration" => self.collect_enum(node),
            "type_alias_declaration" => self.collect_type_alias(node),
            _ => {}
        }

        // One CFG per named function / method / named arrow / named function expr.
        let is_fn = matches!(
            node.kind(),
            "function_declaration" | "generator_function_declaration" | "method_definition"
        ) || (matches!(
            node.kind(),
            "arrow_function" | "function_expression" | "generator_function"
        ) && is_named_function(node));
        if is_fn {
            self.collect_cfg(node);
        }

        match node.kind() {
            "call_expression" => self.collect_call(node),
            "new_expression" => self.collect_new(node),
            "member_expression" => self.collect_field_access(node),
            "type_identifier" => self.collect_type_ref(node),
            "identifier" | "shorthand_property_identifier_pattern" => self.collect_ident(node),
            _ => {}
        }

        for child in named_children(node) {
            self.walk(child);
        }
    }

    // ----- variables -------------------------------------------------------

    fn collect_declaration(&mut self, node: Node) {
        for declarator in named_children(node) {
            if declarator.kind() != "variable_declarator" {
                continue;
            }
            let type_name = declarator
                .child_by_field_name("type")
                .map(|t| strip_annotation(self.text(t)));
            let Some(name_node) = declarator.child_by_field_name("name") else {
                continue;
            };
            for name in bindings(name_node) {
                self.push_var(name, VarKind::Local, type_name.clone());
            }
        }
    }

    fn collect_params(&mut self, params: Node) {
        for child in named_children(params) {
            self.collect_param(child);
        }
    }

    fn collect_param(&mut self, param: Node) {
        let type_name = param
            .child_by_field_name("type")
            .map(|t| strip_annotation(self.text(t)));
        let names = param_bindings(param);
        for name in names {
            self.push_var(name, VarKind::Param, type_name.clone());
        }
    }

    fn push_var(&mut self, name_node: Node, kind: VarKind, type_name: Option<String>) {
        let name = self.text(name_node).to_string();
        if name.is_empty() {
            return;
        }
        let byte = name_node.start_byte();
        if self.vars.iter().any(|v| v.name_byte == byte) {
            return;
        }
        let line = name_node.start_position().row + 1;
        let (scope_start, scope_end, top_level) = self.scope_of(name_node);
        let kind = if top_level && kind == VarKind::Local {
            VarKind::Global
        } else {
            kind
        };
        let enclosing = enclosing_symbol(self.symbols, line);
        self.vars.push(VarRec {
            var: Variable {
                id: format!("{}:{}:{}", self.file, line, name),
                name,
                kind,
                file: self.file.to_string(),
                decl_line: line,
                scope_start,
                scope_end,
                type_name: type_name.filter(|t| !t.is_empty()),
                reads: 0,
                writes: 0,
                first_use: None,
                last_use: None,
                enclosing,
            },
            name_byte: byte,
        });
    }

    /// `(scope_start, scope_end, is_top_level)` for a declaration name node.
    fn scope_of(&self, node: Node) -> (usize, usize, bool) {
        let mut cur = node.parent();
        while let Some(p) = cur {
            match p.kind() {
                "program" => {
                    return (p.start_position().row + 1, p.end_position().row + 1, true);
                }
                "statement_block" | "class_body" | "interface_body" | "enum_body"
                | "switch_body" => {
                    return (p.start_position().row + 1, p.end_position().row + 1, false);
                }
                "arrow_function"
                | "function_declaration"
                | "generator_function_declaration"
                | "method_definition"
                | "function_expression"
                | "generator_function" => {
                    return (p.start_position().row + 1, p.end_position().row + 1, false);
                }
                _ => cur = p.parent(),
            }
        }
        (1, self.source.lines().count().max(1), true)
    }

    fn collect_ident(&mut self, node: Node) {
        if is_binding_identifier(node) {
            return;
        }
        let (read, write) = ident_role(node);
        self.idents.push(IdentUse {
            name: self.text(node).to_string(),
            byte: node.start_byte(),
            line: node.start_position().row + 1,
            col: node.start_position().column + 1,
            read,
            write,
        });
    }

    // ----- types -----------------------------------------------------------

    fn collect_class(&mut self, node: Node) {
        let Some(name_node) = node.child_by_field_name("name") else {
            return;
        };
        let name = self.text(name_node).to_string();
        let line = name_node.start_position().row + 1;
        let id = format!("{}:{}", self.file, name);
        let mut fields = Vec::new();
        let mut methods = Vec::new();
        if let Some(body) = node.child_by_field_name("body") {
            for member in named_children(body) {
                match member.kind() {
                    "public_field_definition" | "field_definition" => {
                        let name_field = member
                            .child_by_field_name("name")
                            .or_else(|| member.child_by_field_name("property"));
                        let Some(nf) = name_field else { continue };
                        let ftype = member
                            .child_by_field_name("type")
                            .map(|t| strip_annotation(self.text(t)))
                            .unwrap_or_default();
                        let fline = nf.start_position().row + 1;
                        fields.push(Field {
                            name: self.text(nf).to_string(),
                            type_name: ftype.clone(),
                            visibility: visibility_of(member, self.source),
                            line: fline,
                        });
                        if let Some(base) = simple_type_name_from_text(&ftype) {
                            self.pending_field_edges.push((id.clone(), base, fline));
                        }
                        let tn = if ftype.is_empty() { None } else { Some(ftype) };
                        self.push_var(nf, VarKind::Field, tn);
                    }
                    "method_definition" | "method_signature" | "abstract_method_signature" => {
                        if let Some(n) = member.child_by_field_name("name") {
                            methods.push(self.text(n).to_string());
                        }
                    }
                    _ => {}
                }
            }
        }
        let doc = leading_doc(node, self.source);
        self.types.push(TypeNode {
            id: id.clone(),
            name,
            kind: TypeKind::Class,
            file: self.file.to_string(),
            line,
            fields,
            methods,
            doc,
            module_path: module_path(self.file),
        });
        for heritage in named_children(node) {
            if heritage.kind() != "class_heritage" {
                continue;
            }
            for clause in named_children(heritage) {
                match clause.kind() {
                    "extends_clause" => {
                        if let Some(v) = clause.child_by_field_name("value") {
                            if let Some(base) = simple_type_name_from_text(self.text(v)) {
                                self.type_edges.push(TypeEdge {
                                    from: id.clone(),
                                    to: format!("{}:{}", self.file, base),
                                    kind: TypeEdgeKind::Inherit,
                                    line: v.start_position().row + 1,
                                });
                            }
                        }
                    }
                    "implements_clause" => {
                        for t in named_children(clause) {
                            if let Some(base) = simple_type_name_from_text(self.text(t)) {
                                self.type_edges.push(TypeEdge {
                                    from: id.clone(),
                                    to: format!("{}:{}", self.file, base),
                                    kind: TypeEdgeKind::Impl,
                                    line: t.start_position().row + 1,
                                });
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    fn collect_interface(&mut self, node: Node) {
        let Some(name_node) = node.child_by_field_name("name") else {
            return;
        };
        let name = self.text(name_node).to_string();
        let line = name_node.start_position().row + 1;
        let id = format!("{}:{}", self.file, name);
        let mut fields = Vec::new();
        let mut methods = Vec::new();
        if let Some(body) = node.child_by_field_name("body") {
            for member in named_children(body) {
                match member.kind() {
                    "property_signature" => {
                        let Some(nf) = member.child_by_field_name("name") else {
                            continue;
                        };
                        let ftype = member
                            .child_by_field_name("type")
                            .map(|t| strip_annotation(self.text(t)))
                            .unwrap_or_default();
                        let fline = nf.start_position().row + 1;
                        fields.push(Field {
                            name: self.text(nf).to_string(),
                            type_name: ftype.clone(),
                            visibility: visibility_of(member, self.source),
                            line: fline,
                        });
                        if let Some(base) = simple_type_name_from_text(&ftype) {
                            self.pending_field_edges.push((id.clone(), base, fline));
                        }
                    }
                    "method_signature" => {
                        if let Some(n) = member.child_by_field_name("name") {
                            methods.push(self.text(n).to_string());
                        }
                    }
                    _ => {}
                }
            }
        }
        let doc = leading_doc(node, self.source);
        self.types.push(TypeNode {
            id: id.clone(),
            name,
            kind: TypeKind::Interface,
            file: self.file.to_string(),
            line,
            fields,
            methods,
            doc,
            module_path: module_path(self.file),
        });
        for clause in named_children(node) {
            if clause.kind() != "extends_type_clause" {
                continue;
            }
            let Some(t) = clause.child_by_field_name("type") else {
                continue;
            };
            if let Some(base) = simple_type_name_from_text(self.text(t)) {
                self.type_edges.push(TypeEdge {
                    from: id.clone(),
                    to: format!("{}:{}", self.file, base),
                    kind: TypeEdgeKind::Inherit,
                    line: t.start_position().row + 1,
                });
            }
        }
    }

    fn collect_enum(&mut self, node: Node) {
        let Some(name_node) = node.child_by_field_name("name") else {
            return;
        };
        let name = self.text(name_node).to_string();
        let line = name_node.start_position().row + 1;
        let mut fields = Vec::new();
        if let Some(body) = node.child_by_field_name("body") {
            for member in named_children(body) {
                let nf = match member.kind() {
                    "enum_assignment" => member
                        .child_by_field_name("name")
                        .or_else(|| member.named_child(0)),
                    _ => Some(member),
                };
                if let Some(nf) = nf {
                    let mname = self.text(nf).trim().trim_matches('"').to_string();
                    if !mname.is_empty() {
                        fields.push(Field {
                            name: mname,
                            type_name: String::new(),
                            visibility: String::new(),
                            line: nf.start_position().row + 1,
                        });
                    }
                }
            }
        }
        let doc = leading_doc(node, self.source);
        // NOTE: enum members are emitted as `Field`s; no `Variant` edges are
        // modelled because members carry no payload type in this pass.
        self.types.push(TypeNode {
            id: format!("{}:{}", self.file, name),
            name,
            kind: TypeKind::Enum,
            file: self.file.to_string(),
            line,
            fields,
            methods: Vec::new(),
            doc,
            module_path: module_path(self.file),
        });
    }

    fn collect_type_alias(&mut self, node: Node) {
        let Some(name_node) = node.child_by_field_name("name") else {
            return;
        };
        let name = self.text(name_node).to_string();
        let line = name_node.start_position().row + 1;
        let id = format!("{}:{}", self.file, name);
        // `TypeKind` has no alias variant; a named alias is recorded as a Struct
        // (structural type) and, when it aliases a named type, linked via Inherit.
        let doc = leading_doc(node, self.source);
        self.types.push(TypeNode {
            id: id.clone(),
            name,
            kind: TypeKind::Struct,
            file: self.file.to_string(),
            line,
            fields: Vec::new(),
            methods: Vec::new(),
            doc,
            module_path: module_path(self.file),
        });
        if let Some(value) = node.child_by_field_name("value") {
            if let Some(base) = simple_type_name_from_text(self.text(value)) {
                self.type_edges.push(TypeEdge {
                    from: id,
                    to: format!("{}:{}", self.file, base),
                    kind: TypeEdgeKind::Inherit,
                    line: value.start_position().row + 1,
                });
            }
        }
    }

    // ----- use sites -------------------------------------------------------

    fn collect_call(&mut self, node: Node) {
        let func = node.child_by_field_name("function");
        let (name, line, col) = match func {
            Some(f) => (
                callee_name(f, self.source),
                f.start_position().row + 1,
                f.start_position().column + 1,
            ),
            None => (
                String::new(),
                node.start_position().row + 1,
                node.start_position().column + 1,
            ),
        };
        self.uses.push(UseSite {
            target: name,
            file: self.file.to_string(),
            line,
            col,
            kind: UseKind::Call,
            snippet: snippet(self.source, line),
        });
    }

    fn collect_new(&mut self, node: Node) {
        let constructor = node
            .child_by_field_name("constructor")
            .or_else(|| node.named_child(0));
        let (name, line, col) = match constructor {
            Some(c) => (
                callee_name(c, self.source),
                c.start_position().row + 1,
                c.start_position().column + 1,
            ),
            None => (
                String::new(),
                node.start_position().row + 1,
                node.start_position().column + 1,
            ),
        };
        self.uses.push(UseSite {
            target: name,
            file: self.file.to_string(),
            line,
            col,
            kind: UseKind::Call,
            snippet: snippet(self.source, line),
        });
    }

    fn collect_field_access(&mut self, node: Node) {
        let Some(prop) = node.child_by_field_name("property") else {
            return;
        };
        let line = prop.start_position().row + 1;
        self.uses.push(UseSite {
            target: self.text(prop).to_string(),
            file: self.file.to_string(),
            line,
            col: prop.start_position().column + 1,
            kind: UseKind::FieldAccess,
            snippet: snippet(self.source, line),
        });
    }

    fn collect_type_ref(&mut self, node: Node) {
        let text = self.text(node).trim();
        if text.is_empty() {
            return;
        }
        let line = node.start_position().row + 1;
        self.uses.push(UseSite {
            target: text.to_string(),
            file: self.file.to_string(),
            line,
            col: node.start_position().column + 1,
            kind: UseKind::TypeRef,
            snippet: snippet(self.source, line),
        });
    }

    // ----- control flow ----------------------------------------------------

    fn collect_cfg(&mut self, node: Node) {
        let line = node.start_position().row + 1;
        let sym = enclosing_symbol(self.symbols, line);
        let symbol = if sym.is_empty() {
            format!("{}:{}", self.file, line)
        } else {
            sym
        };
        let (decisions, max_nesting) = metrics(node, self.source);
        let (nodes, edges) = build_cfg(self.source, node);
        self.cfgs.push(FunctionCfg {
            symbol,
            nodes,
            edges,
            cyclomatic: decisions + 1,
            max_nesting,
        });
    }

    // ----- finalize --------------------------------------------------------

    fn finish(mut self, out: &mut Analysis) {
        let declared: HashSet<&str> = self.types.iter().map(|t| t.name.as_str()).collect();
        for (owner, base, line) in &self.pending_field_edges {
            if declared.contains(base.as_str()) {
                self.type_edges.push(TypeEdge {
                    from: owner.clone(),
                    to: format!("{}:{}", self.file, base),
                    kind: TypeEdgeKind::Field,
                    line: *line,
                });
            }
        }

        for rec in self.vars.iter_mut() {
            let mut reads = 0usize;
            let mut writes = 0usize;
            let mut first: Option<usize> = None;
            let mut last: Option<usize> = None;
            for u in &self.idents {
                if u.name != rec.var.name || u.byte == rec.name_byte {
                    continue;
                }
                if u.line < rec.var.scope_start || u.line > rec.var.scope_end {
                    continue;
                }
                if u.read {
                    reads += 1;
                }
                if u.write {
                    writes += 1;
                }
                first = Some(first.map_or(u.line, |f| f.min(u.line)));
                last = Some(last.map_or(u.line, |l| l.max(u.line)));
            }
            rec.var.reads = reads;
            rec.var.writes = writes;
            rec.var.first_use = first;
            rec.var.last_use = last;
        }

        let mut var_uses: Vec<UseSite> = Vec::new();
        for u in &self.idents {
            let target = self
                .find_var_for(u)
                .map(|v| v.var.id.clone())
                .unwrap_or_else(|| u.name.clone());
            let kind = if u.write {
                UseKind::VarWrite
            } else {
                UseKind::VarRead
            };
            var_uses.push(UseSite {
                target,
                file: self.file.to_string(),
                line: u.line,
                col: u.col,
                kind,
                snippet: snippet(self.source, u.line),
            });
        }

        out.variables.extend(self.vars.into_iter().map(|r| r.var));
        out.cfgs.extend(self.cfgs);
        out.types.extend(self.types);
        out.type_edges.extend(self.type_edges);
        out.uses.extend(self.uses);
        out.uses.extend(var_uses);
    }

    fn find_var_for(&self, use_site: &IdentUse) -> Option<&VarRec> {
        self.vars
            .iter()
            .filter(|v| {
                v.var.name == use_site.name
                    && v.name_byte != use_site.byte
                    && use_site.line >= v.var.scope_start
                    && use_site.line <= v.var.scope_end
            })
            .min_by_key(|v| v.var.scope_end.saturating_sub(v.var.scope_start))
    }
}

// ----- pattern / binding helpers ------------------------------------------

/// Every simple binding name introduced by a pattern node.
fn bindings<'t>(node: Node<'t>) -> Vec<Node<'t>> {
    let mut out = Vec::new();
    collect_bindings(node, &mut out);
    out
}

fn collect_bindings<'t>(node: Node<'t>, out: &mut Vec<Node<'t>>) {
    match node.kind() {
        "identifier" | "shorthand_property_identifier_pattern" => out.push(node),
        "rest_pattern" | "array_pattern" | "object_pattern" => {
            for child in named_children(node) {
                collect_bindings(child, out);
            }
        }
        "pair_pattern" | "object_assignment_pattern" => {
            if let Some(v) = node.child_by_field_name("value") {
                collect_bindings(v, out);
            } else if let Some(l) = node.child_by_field_name("left") {
                collect_bindings(l, out);
            }
        }
        "assignment_pattern" => {
            if let Some(l) = node.child_by_field_name("left") {
                collect_bindings(l, out);
            }
        }
        "required_parameter" | "optional_parameter" => {
            if let Some(p) = node.child_by_field_name("pattern") {
                collect_bindings(p, out);
            } else if let Some(n) = node.child_by_field_name("name") {
                collect_bindings(n, out);
            }
        }
        _ => {}
    }
}

/// Bindings named by one parameter (TS `required_parameter`/`optional_parameter`
/// or a bare JS pattern / identifier).
fn param_bindings<'t>(param: Node<'t>) -> Vec<Node<'t>> {
    match param.kind() {
        "required_parameter" | "optional_parameter" => {
            let mut out = Vec::new();
            if let Some(p) = param.child_by_field_name("pattern") {
                collect_bindings(p, &mut out);
            }
            if let Some(n) = param.child_by_field_name("name") {
                collect_bindings(n, &mut out);
            }
            if out.is_empty() {
                collect_bindings(param, &mut out);
            }
            out
        }
        _ => bindings(param),
    }
}

/// True when `node` is a declaration/binding name rather than a variable use.
fn is_binding_identifier(node: Node) -> bool {
    let Some(parent) = node.parent() else {
        return false;
    };
    match parent.kind() {
        "formal_parameters"
        | "variable_declarator"
        | "required_parameter"
        | "optional_parameter"
        | "rest_pattern"
        | "pair_pattern"
        | "array_pattern"
        | "object_pattern"
        | "shorthand_property_identifier_pattern"
        | "object_assignment_pattern" => true,
        "assignment_pattern" => parent
            .child_by_field_name("left")
            .map(|l| l.id() == node.id())
            .unwrap_or(false),
        "public_field_definition" | "field_definition" => parent
            .child_by_field_name("name")
            .or_else(|| parent.child_by_field_name("property"))
            .map(|n| n.id() == node.id())
            .unwrap_or(false),
        "function_declaration"
        | "generator_function_declaration"
        | "function_expression"
        | "generator_function"
        | "class_declaration"
        | "abstract_class_declaration"
        | "method_definition"
        | "interface_declaration"
        | "enum_declaration"
        | "type_alias_declaration" => parent
            .child_by_field_name("name")
            .map(|n| n.id() == node.id())
            .unwrap_or(false),
        _ => false,
    }
}

/// `(read, write)` role of an identifier occurrence.
fn ident_role(node: Node) -> (bool, bool) {
    let Some(parent) = node.parent() else {
        return (true, false);
    };
    match parent.kind() {
        "assignment_expression" => {
            let is_left = parent
                .child_by_field_name("left")
                .map(|l| l.id() == node.id())
                .unwrap_or(false);
            if is_left {
                (false, true)
            } else {
                (true, false)
            }
        }
        "augmented_assignment_expression" | "update_expression" => (true, true),
        _ => (true, false),
    }
}

// ----- type helpers --------------------------------------------------------

fn visibility_of(member: Node, source: &str) -> String {
    for child in named_children(member) {
        if child.kind() == "accessibility_modifier" {
            return child.utf8_text(source.as_bytes()).unwrap_or("").to_string();
        }
    }
    String::new()
}

/// Leading `/** */` / `//` comment(s) directly above a node, cleaned.
fn leading_doc(node: Node, source: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();
    let mut prev = node.prev_named_sibling();
    while let Some(sib) = prev {
        if sib.kind() != "comment" {
            break;
        }
        if let Ok(raw) = sib.utf8_text(source.as_bytes()) {
            parts.push(raw);
        }
        prev = sib.prev_named_sibling();
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
    if let Some(rest) = t.strip_prefix("//") {
        return rest.trim().to_string();
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

fn strip_annotation(raw: &str) -> String {
    raw.trim().trim_start_matches(':').trim().to_string()
}

/// Best-effort base type name from a raw annotation / heritage text.
fn simple_type_name_from_text(raw: &str) -> Option<String> {
    let base = raw.split('<').next().unwrap_or(raw).trim();
    let base = base.rsplit('.').next().unwrap_or(base).trim();
    let base = base.trim_end_matches("[]").trim();
    if base.is_empty() {
        return None;
    }
    let mut chars = base.chars();
    let first = chars.next()?;
    if !(first.is_ascii_alphabetic() || first == '_' || first == '$') {
        return None;
    }
    if !chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$') {
        return None;
    }
    Some(base.to_string())
}

fn module_path(file: &str) -> String {
    let trimmed = file.trim_start_matches("./");
    let without_ext = trimmed.rsplit_once('.').map(|(a, _)| a).unwrap_or(trimmed);
    without_ext.replace('/', "::")
}

fn callee_name(node: Node, source: &str) -> String {
    if node.kind() == "member_expression" {
        if let Some(p) = node.child_by_field_name("property") {
            return p.utf8_text(source.as_bytes()).unwrap_or("").to_string();
        }
    }
    node.utf8_text(source.as_bytes()).unwrap_or("").to_string()
}

fn named_children<'t>(node: Node<'t>) -> Vec<Node<'t>> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor).collect()
}

/// A function-like node that owns its own CFG (named function, method, or an
/// arrow / function expression bound to a name). Anonymous callbacks are
/// folded into the enclosing function instead.
fn is_named_function(node: Node) -> bool {
    match node.kind() {
        "function_declaration" | "generator_function_declaration" | "method_definition" => true,
        "arrow_function" | "function_expression" | "generator_function" => {
            let Some(parent) = node.parent() else {
                return false;
            };
            matches!(
                parent.kind(),
                "variable_declarator" | "public_field_definition" | "field_definition"
            ) || (parent.kind() == "assignment_expression"
                && parent
                    .child_by_field_name("right")
                    .map(|r| r.id() == node.id())
                    .unwrap_or(false))
        }
        _ => false,
    }
}

// ----- complexity metrics --------------------------------------------------

fn metrics(node: Node, source: &str) -> (u32, u32) {
    let mut decisions = 0u32;
    let mut max_nesting = 0u32;
    metrics_rec(node, source, 0, &mut decisions, &mut max_nesting);
    (decisions, max_nesting)
}

fn metrics_rec(node: Node, source: &str, depth: u32, decisions: &mut u32, max_nesting: &mut u32) {
    for child in named_children(node) {
        if is_named_function(child) {
            continue;
        }
        let control = is_control(child, source);
        let adds = match child.kind() {
            "if_statement" | "for_statement" | "for_in_statement" | "while_statement"
            | "do_statement" | "switch_case" | "ternary_expression" | "catch_clause" => 1,
            "binary_expression" => u32::from(is_logical(child, source)),
            _ => 0,
        };
        *decisions += adds;
        let next = if control { depth + 1 } else { depth };
        if next > *max_nesting {
            *max_nesting = next;
        }
        metrics_rec(child, source, next, decisions, max_nesting);
    }
}

fn is_control(node: Node, source: &str) -> bool {
    matches!(
        node.kind(),
        "if_statement"
            | "for_statement"
            | "for_in_statement"
            | "while_statement"
            | "do_statement"
            | "switch_statement"
            | "switch_case"
            | "switch_default"
            | "try_statement"
            | "catch_clause"
            | "ternary_expression"
    ) || (node.kind() == "binary_expression" && is_logical(node, source))
}

fn is_logical(node: Node, source: &str) -> bool {
    node.child_by_field_name("operator")
        .and_then(|op| op.utf8_text(source.as_bytes()).ok())
        .is_some_and(|op| matches!(op, "&&" | "||" | "??"))
}

// ----- CFG construction ----------------------------------------------------

struct CfgBuilder<'a> {
    source: &'a str,
    nodes: Vec<CfgNode>,
    edges: Vec<CfgEdge>,
    next: u32,
    exit: u32,
}

impl<'a> CfgBuilder<'a> {
    fn new(source: &'a str) -> Self {
        Self {
            source,
            nodes: Vec::new(),
            edges: Vec::new(),
            next: 0,
            exit: 0,
        }
    }

    fn add(&mut self, kind: CfgKind, node: Node, label: &str) -> u32 {
        let id = self.next;
        self.next += 1;
        self.nodes.push(CfgNode {
            id,
            kind,
            start_line: node.start_position().row + 1,
            end_line: node.end_position().row + 1,
            label: truncate(label, 60),
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

    fn build(&mut self, node: Node, up: Vec<u32>, kind: CfgEdgeKind) -> Vec<u32> {
        if is_named_function(node) {
            return up;
        }
        match node.kind() {
            "statement_block" => self.build_block(node, up, kind),
            "if_statement" => self.build_if(node, up, kind),
            "for_statement" | "for_in_statement" | "while_statement" | "do_statement" => {
                self.build_loop(node, up, kind)
            }
            "switch_statement" => self.build_match(node, up, kind),
            "switch_case" | "switch_default" => self.build_case(node, up, kind),
            "return_statement" | "throw_statement" => self.build_return(node, up, kind),
            "try_statement" => self.build_try(node, up, kind),
            "else_clause" => match named_children(node).first().copied() {
                Some(child) => self.build(child, up, kind),
                None => up,
            },
            "empty_statement" => up,
            _ => self.build_simple(node, up, kind),
        }
    }

    fn build_block(&mut self, node: Node, up: Vec<u32>, kind: CfgEdgeKind) -> Vec<u32> {
        let line = node.start_position().row + 1;
        let block = self.add(CfgKind::Block, node, "block");
        for u in &up {
            self.edge(*u, block, kind, line);
        }
        let mut frontier = vec![block];
        for child in named_children(node) {
            frontier = self.build(child, frontier, CfgEdgeKind::Normal);
        }
        frontier
    }

    fn build_if(&mut self, node: Node, up: Vec<u32>, kind: CfgEdgeKind) -> Vec<u32> {
        let line = node.start_position().row + 1;
        let cond = node.child_by_field_name("condition");
        let label = cond.map(|c| self.text(c)).unwrap_or_default();
        let branch = self.add(CfgKind::Branch, cond.unwrap_or(node), label);
        for u in &up {
            self.edge(*u, branch, kind, line);
        }
        let mut tails = match node.child_by_field_name("consequence") {
            Some(c) => self.build(c, vec![branch], CfgEdgeKind::True),
            None => vec![branch],
        };
        match node.child_by_field_name("alternative") {
            Some(alt) => {
                let false_tails = self.build(alt, vec![branch], CfgEdgeKind::False);
                tails.extend(false_tails);
            }
            None => tails.push(branch),
        }
        tails
    }

    fn build_loop(&mut self, node: Node, up: Vec<u32>, kind: CfgEdgeKind) -> Vec<u32> {
        let line = node.start_position().row + 1;
        let label = self.text(node);
        let lp = self.add(CfgKind::Loop, node, label);
        for u in &up {
            self.edge(*u, lp, kind, line);
        }
        if let Some(body) = node.child_by_field_name("body") {
            let body_line = body.end_position().row + 1;
            let body_tails = self.build(body, vec![lp], CfgEdgeKind::Normal);
            for t in body_tails {
                self.edge(t, lp, CfgEdgeKind::Back, body_line);
            }
        }
        vec![lp]
    }

    fn build_match(&mut self, node: Node, up: Vec<u32>, kind: CfgEdgeKind) -> Vec<u32> {
        let line = node.start_position().row + 1;
        let label = self.text(node);
        let match_id = self.add(CfgKind::Match, node, label);
        for u in &up {
            self.edge(*u, match_id, kind, line);
        }
        let mut tails = Vec::new();
        if let Some(body) = node.child_by_field_name("body") {
            for case in named_children(body) {
                let case_tails = self.build(case, vec![match_id], CfgEdgeKind::Normal);
                tails.extend(case_tails);
            }
        }
        if tails.is_empty() {
            tails.push(match_id);
        }
        tails
    }

    fn build_case(&mut self, node: Node, up: Vec<u32>, kind: CfgEdgeKind) -> Vec<u32> {
        let line = node.start_position().row + 1;
        let value = node.child_by_field_name("value");
        let label = match value {
            Some(v) => format!("case {}", self.text(v)),
            None => "default".to_string(),
        };
        let block = self.add(CfgKind::Block, node, &label);
        for u in &up {
            self.edge(*u, block, kind, line);
        }
        let value_id = value.map(|v| v.id());
        let mut frontier = vec![block];
        for child in named_children(node) {
            if Some(child.id()) == value_id {
                continue;
            }
            frontier = self.build(child, frontier, CfgEdgeKind::Normal);
        }
        frontier
    }

    fn build_return(&mut self, node: Node, up: Vec<u32>, kind: CfgEdgeKind) -> Vec<u32> {
        let line = node.start_position().row + 1;
        let label = self.text(node);
        let ret = self.add(CfgKind::Return, node, label);
        for u in &up {
            self.edge(*u, ret, kind, line);
        }
        self.edge(ret, self.exit, CfgEdgeKind::Normal, line);
        Vec::new()
    }

    fn build_try(&mut self, node: Node, up: Vec<u32>, kind: CfgEdgeKind) -> Vec<u32> {
        let line = node.start_position().row + 1;
        let entry = self.add(CfgKind::Block, node, "try");
        for u in &up {
            self.edge(*u, entry, kind, line);
        }
        let mut tails = Vec::new();
        if let Some(body) = node.child_by_field_name("body") {
            tails.extend(self.build(body, vec![entry], CfgEdgeKind::Normal));
        }
        if let Some(handler) = node.child_by_field_name("handler") {
            if let Some(body) = handler.child_by_field_name("body") {
                tails.extend(self.build(body, vec![entry], CfgEdgeKind::Normal));
            }
        }
        if let Some(finalizer) = node.child_by_field_name("finalizer") {
            let from = tails.clone();
            tails = self.build(finalizer, from, CfgEdgeKind::Normal);
        }
        if tails.is_empty() {
            tails.push(entry);
        }
        tails
    }

    fn build_simple(&mut self, node: Node, up: Vec<u32>, kind: CfgEdgeKind) -> Vec<u32> {
        let calls = collect_calls(node);
        let mut frontier = up;
        let mut first = true;
        for call in calls {
            let line = call.start_position().row + 1;
            let label = callee_name(
                call.child_by_field_name("function")
                    .or_else(|| call.child_by_field_name("constructor"))
                    .unwrap_or(call),
                self.source,
            );
            let id = self.add(CfgKind::Call, call, &label);
            for u in &frontier {
                let edge_kind = if first { kind } else { CfgEdgeKind::Normal };
                self.edge(*u, id, edge_kind, line);
            }
            frontier = vec![id];
            first = false;
        }
        frontier
    }

    fn text(&self, node: Node) -> &'a str {
        node.utf8_text(self.source.as_bytes()).unwrap_or("")
    }
}

fn build_cfg(source: &str, fn_node: Node) -> (Vec<CfgNode>, Vec<CfgEdge>) {
    let mut b = CfgBuilder::new(source);
    let line = fn_node.start_position().row + 1;
    let entry = b.add(CfgKind::Entry, fn_node, "entry");
    let exit = b.add(CfgKind::Exit, fn_node, "exit");
    b.exit = exit;
    let body = fn_node.child_by_field_name("body").unwrap_or(fn_node);
    let tails = b.build(body, vec![entry], CfgEdgeKind::Normal);
    let end_line = fn_node.end_position().row + 1;
    for t in tails {
        b.edge(t, exit, CfgEdgeKind::Normal, end_line.max(line));
    }
    (b.nodes, b.edges)
}

/// Call expressions reachable in a statement without entering a nested named
/// function (those own their own CFG).
fn collect_calls<'t>(node: Node<'t>) -> Vec<Node<'t>> {
    let mut out = Vec::new();
    collect_calls_rec(node, &mut out);
    out
}

fn collect_calls_rec<'t>(node: Node<'t>, out: &mut Vec<Node<'t>>) {
    if is_named_function(node) {
        return;
    }
    if matches!(node.kind(), "call_expression" | "new_expression") {
        out.push(node);
    }
    for child in named_children(node) {
        collect_calls_rec(child, out);
    }
}

fn truncate(s: &str, max: usize) -> String {
    let collapsed = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.chars().count() > max {
        collapsed.chars().take(max).collect()
    } else {
        collapsed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(file: &str, src: &str) -> Analysis {
        let mut out = Analysis::new();
        analyze(file, src, &[], &mut out);
        out
    }

    #[test]
    fn if_else_for_has_cyclomatic_and_back_edge() {
        let src = "function f(x: number): number {\n\
                   \x20 if (x > 0) {\n\
                   \x20   x = x - 1;\n\
                   \x20 } else {\n\
                   \x20   x = 0;\n\
                   \x20 }\n\
                   \x20 for (let i = 0; i < 3; i++) {\n\
                   \x20   x += i;\n\
                   \x20 }\n\
                   \x20 return x;\n\
                   }\n";
        let out = run("m.ts", src);
        let cfg = out.cfgs.iter().find(|c| c.symbol == "m.ts:1").unwrap();
        assert_eq!(cfg.cyclomatic, 3, "if + for => 2 decisions + 1");
        assert!(
            cfg.edges.iter().any(|e| e.kind == CfgEdgeKind::Back),
            "loop body must have a Back edge: {:?}",
            cfg.edges
        );
        assert!(cfg.nodes.iter().any(|n| n.kind == CfgKind::Branch));
        assert!(cfg.nodes.iter().any(|n| n.kind == CfgKind::Loop));
        assert!(cfg.nodes.iter().any(|n| n.kind == CfgKind::Return));
        assert!(cfg.nodes.iter().any(|n| n.kind == CfgKind::Entry));
        assert!(cfg.nodes.iter().any(|n| n.kind == CfgKind::Exit));
    }

    #[test]
    fn used_and_unused_variables() {
        let src = "const used: number = 1;\n\
                   const unused: number = 2;\n\
                   function g(): number { return used + used; }\n";
        let out = run("v.ts", src);
        let find = |name: &str| {
            out.variables
                .iter()
                .find(|v| v.name == name)
                .unwrap_or_else(|| panic!("missing variable {name}"))
        };
        let used = find("used");
        assert!(used.reads >= 2, "used reads: {}", used.reads);
        assert_eq!(used.type_name.as_deref(), Some("number"));
        assert_eq!(used.kind, VarKind::Global);
        assert_eq!(used.first_use, Some(3));

        let unused = find("unused");
        assert_eq!(unused.reads, 0);
        assert_eq!(unused.first_use, None);
    }

    #[test]
    fn class_fields_and_methods() {
        let src = "class Point {\n\
                   \x20 x: number;\n\
                   \x20 y: number;\n\
                   \x20 constructor(x: number, y: number) { this.x = x; this.y = y; }\n\
                   \x20 move(dx: number): void { this.x += dx; }\n\
                   }\n";
        let out = run("p.ts", src);
        let ty = out.types.iter().find(|t| t.name == "Point").unwrap();
        assert_eq!(ty.kind, TypeKind::Class);
        let field_names: Vec<&str> = ty.fields.iter().map(|f| f.name.as_str()).collect();
        assert!(field_names.contains(&"x") && field_names.contains(&"y"));
        let method_names: Vec<&str> = ty.methods.iter().map(String::as_str).collect();
        assert!(method_names.contains(&"constructor"), "{method_names:?}");
        assert!(method_names.contains(&"move"), "{method_names:?}");

        // class fields become Field variables
        let field = out.variables.iter().find(|v| v.name == "x").unwrap();
        assert_eq!(field.kind, VarKind::Field);
        // method parameters become Param variables
        let param = out
            .variables
            .iter()
            .find(|v| v.name == "dx")
            .expect("dx param");
        assert_eq!(param.kind, VarKind::Param);
    }

    #[test]
    fn interface_doc_and_edges() {
        let src = "/** A shape. */\n\
                   interface Shape {\n\
                   \x20 area: number;\n\
                   \x20 scale(f: number): Shape;\n\
                   }\n\
                   class Circle extends Shape {\n\
                   \x20 r: number;\n\
                   \x20 anchor: Shape;\n\
                   }\n";
        let out = run("s.ts", src);
        let shape = out.types.iter().find(|t| t.name == "Shape").unwrap();
        assert_eq!(shape.kind, TypeKind::Interface);
        assert_eq!(shape.doc, "A shape.");
        assert!(shape.fields.iter().any(|f| f.name == "area"));
        assert!(shape.methods.iter().any(|m| m == "scale"));
        assert!(
            out.type_edges
                .iter()
                .any(|e| e.kind == TypeEdgeKind::Inherit && e.to == "s.ts:Shape"),
            "extends => Inherit edge"
        );
        assert!(
            out.type_edges.iter().any(|e| e.kind == TypeEdgeKind::Field
                && e.from == "s.ts:Circle"
                && e.to == "s.ts:Shape"),
            "field typed Shape => Field edge"
        );
    }

    #[test]
    fn use_sites_include_calls_fields_and_type_refs() {
        let src = "import { helper } from './h';\n\
                   function go(p: Point): number {\n\
                   \x20 const q = new Point(1, 2);\n\
                   \x20 return helper(q.value);\n\
                   }\n";
        let out = run("u.ts", src);
        assert!(
            out.uses
                .iter()
                .any(|u| u.kind == UseKind::Call && u.target == "helper"),
            "call use: {:?}",
            out.uses
        );
        assert!(
            out.uses
                .iter()
                .any(|u| u.kind == UseKind::Call && u.target == "Point"),
            "new use"
        );
        assert!(
            out.uses
                .iter()
                .any(|u| u.kind == UseKind::FieldAccess && u.target == "value"),
            "field access use"
        );
        assert!(
            out.uses
                .iter()
                .any(|u| u.kind == UseKind::TypeRef && u.target == "Point"),
            "type ref use: {:?}",
            out.uses
        );
        assert!(
            out.uses
                .iter()
                .any(|u| u.kind == UseKind::VarRead && u.target.contains(":3:q")),
            "var read target resolves to variable id: {:?}",
            out.uses
        );
    }

    #[test]
    fn tsx_parses_without_panic() {
        let src = "const App = () => <div className=\"x\">{1 + 1}</div>;\n\
                   export default App;\n";
        let out = run("App.tsx", src);
        assert!(
            out.variables.iter().any(|v| v.name == "App"),
            "App variable"
        );
    }

    #[test]
    fn empty_and_malformed_sources_do_not_panic() {
        let _ = run("a.ts", "");
        let _ = run("b.js", "function ( { const = ;");
        let _ = run("c.tsx", "<div>");
    }
}
