//! Rust analysis: variables, control flow, types, references (INTEL-2).
//!
//! Fills the frozen [`crate::analysis`] contract from tree-sitter-rust. Uses the
//! shared helpers in [`super`] (`enclosing_symbol`, `snippet`).
//!
//! Counting convention (mirrors `fixtures/vars/rust_counts.rs`):
//!   * a declaration / binding is exactly one WRITE (except params/fields, which
//!     are introduced by the caller and are not writes);
//!   * a later appearance that only reads the value is one READ;
//!   * a plain reassignment (`x = v`) is one WRITE;
//!   * a compound assignment (`x += v`) is one READ and one WRITE.
use super::{enclosing_symbol, enclosing_symbol_at, snippet};
use crate::analysis::{
    Analysis, CfgEdge, CfgEdgeKind, CfgKind, CfgNode, Field, FunctionCfg, TypeEdge, TypeEdgeKind,
    TypeKind, TypeNode, UseKind, UseSite, VarKind, Variable,
};
use crate::model::{Symbol, SymbolKind};
use std::collections::{BTreeMap, BTreeSet};
use tree_sitter::{Node, Parser, Query, QueryCursor, StreamingIterator};

/// One query for every node kind the analyzer needs, dispatched by capture name.
const ITEMS: &str = r#"
(struct_item) @struct_item
(enum_item) @enum_item
(trait_item) @trait_item
(impl_item) @impl_item
(function_item) @function_item
(let_declaration) @let_declaration
(let_condition) @let_condition
(parameter) @parameter
(field_declaration) @field_declaration
(const_item) @const_item
(static_item) @static_item
(call_expression) @call_expression
(field_expression) @field_expression
(type_identifier) @type_identifier
(match_arm) @match_arm
(for_expression) @for_expression
(identifier) @identifier
"#;

/// Append Rust variables/CFG/types/use-sites for `file` into `out`.
pub fn analyze(file: &str, source: &str, symbols: &[Symbol], out: &mut Analysis) {
    let mut parser = Parser::new();
    if parser
        .set_language(&tree_sitter_rust::LANGUAGE.into())
        .is_err()
    {
        return;
    }
    let Some(tree) = parser.parse(source, None) else {
        return;
    };
    let root = tree.root_node();
    let Some(items) = collect_items(root, source) else {
        return;
    };

    let line_count = source.lines().count().max(1);
    let ctx = Ctx {
        file,
        source,
        symbols,
        line_count,
    };

    let (types, type_edges, known) = build_types(&ctx, &items, out);
    let (variables, mut uses) = collect_variables(&ctx, &items);
    collect_use_sites(&ctx, &items, &known, &mut uses);
    let cfgs: Vec<FunctionCfg> = items
        .functions
        .iter()
        .map(|f| build_cfg(&ctx, *f))
        .collect();

    out.types.extend(types);
    out.type_edges.extend(type_edges);
    out.variables.extend(variables);
    out.uses.extend(uses);
    out.cfgs.extend(cfgs);
    sort_analysis(out);
}

struct Ctx<'a> {
    file: &'a str,
    source: &'a str,
    symbols: &'a [Symbol],
    line_count: usize,
}

#[derive(Default)]
struct Items<'t> {
    structs: Vec<Node<'t>>,
    enums: Vec<Node<'t>>,
    traits: Vec<Node<'t>>,
    impls: Vec<Node<'t>>,
    functions: Vec<Node<'t>>,
    lets: Vec<Node<'t>>,
    let_conditions: Vec<Node<'t>>,
    params: Vec<Node<'t>>,
    fields: Vec<Node<'t>>,
    consts: Vec<Node<'t>>,
    statics: Vec<Node<'t>>,
    calls: Vec<Node<'t>>,
    field_accesses: Vec<Node<'t>>,
    type_ids: Vec<Node<'t>>,
    match_arms: Vec<Node<'t>>,
    for_exprs: Vec<Node<'t>>,
    identifiers: Vec<Node<'t>>,
}

fn collect_items<'t>(root: Node<'t>, source: &str) -> Option<Items<'t>> {
    let query = Query::new(&tree_sitter_rust::LANGUAGE.into(), ITEMS).ok()?;
    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(&query, root, source.as_bytes());
    let names = query.capture_names();
    let mut items = Items::default();
    while let Some(m) = matches.next() {
        for cap in m.captures() {
            let Some(name) = names.get(cap.index as usize) else {
                continue;
            };
            let n = cap.node;
            match *name {
                "struct_item" => items.structs.push(n),
                "enum_item" => items.enums.push(n),
                "trait_item" => items.traits.push(n),
                "impl_item" => items.impls.push(n),
                "function_item" => items.functions.push(n),
                "let_declaration" => items.lets.push(n),
                "let_condition" => items.let_conditions.push(n),
                "parameter" => items.params.push(n),
                "field_declaration" => items.fields.push(n),
                "const_item" => items.consts.push(n),
                "static_item" => items.statics.push(n),
                "call_expression" => items.calls.push(n),
                "field_expression" => items.field_accesses.push(n),
                "type_identifier" => items.type_ids.push(n),
                "match_arm" => items.match_arms.push(n),
                "for_expression" => items.for_exprs.push(n),
                "identifier" => items.identifiers.push(n),
                _ => {}
            }
        }
    }
    Some(items)
}

// ---------------------------------------------------------------------------
// types
// ---------------------------------------------------------------------------

fn build_types(
    ctx: &Ctx,
    items: &Items,
    existing: &Analysis,
) -> (Vec<TypeNode>, Vec<TypeEdge>, BTreeMap<String, String>) {
    let mut types: Vec<TypeNode> = Vec::new();
    for item in items
        .structs
        .iter()
        .chain(items.enums.iter())
        .chain(items.traits.iter())
    {
        if let Some(t) = type_node_from(ctx, *item) {
            types.push(t);
        }
    }

    // methods: impl blocks matching the type name, plus trait bodies.
    let mut methods: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for imp in &items.impls {
        if let Some(tn) = impl_type_name(*imp, ctx.source) {
            if let Some(body) = imp.child_by_field_name("body") {
                collect_method_names(body, ctx.source, &tn, &mut methods);
            }
        }
    }
    for tr in &items.traits {
        if let Some(tn) = tr
            .child_by_field_name("name")
            .and_then(|n| text(n, ctx.source))
        {
            if let Some(body) = tr.child_by_field_name("body") {
                collect_method_names(body, ctx.source, tn, &mut methods);
            }
        }
    }
    for t in &mut types {
        if let Some(set) = methods.get(&t.name) {
            t.methods = set.iter().cloned().collect();
        }
    }

    // name -> id, preferring declarations from this file.
    let mut known: BTreeMap<String, String> = existing
        .types
        .iter()
        .map(|t| (t.name.clone(), t.id.clone()))
        .collect();
    for t in &types {
        known.insert(t.name.clone(), t.id.clone());
    }

    let mut edges: Vec<TypeEdge> = Vec::new();
    // Field containment.
    for t in &types {
        for f in &t.fields {
            if let Some((_, to)) = resolve_type_ref(&f.type_name, &known) {
                edges.push(TypeEdge {
                    from: t.id.clone(),
                    to,
                    kind: TypeEdgeKind::Field,
                    line: f.line,
                });
            }
        }
    }
    // Impl + Inherit.
    for imp in &items.impls {
        let line = imp.start_position().row + 1;
        let Some(tn) = impl_type_name(*imp, ctx.source) else {
            continue;
        };
        let Some(type_id) = known.get(&tn).cloned() else {
            continue;
        };
        edges.push(TypeEdge {
            from: format!("{}:{}", ctx.file, line),
            to: type_id.clone(),
            kind: TypeEdgeKind::Impl,
            line,
        });
        if let Some(tr) = imp.child_by_field_name("trait") {
            if let Some(trn) = first_type_ident(tr).and_then(|n| text(n, ctx.source)) {
                if let Some(trait_id) = known.get(trn) {
                    edges.push(TypeEdge {
                        from: type_id,
                        to: trait_id.clone(),
                        kind: TypeEdgeKind::Inherit,
                        line,
                    });
                }
            }
        }
    }
    // Enum variant payloads.
    for en in &items.enums {
        let Some(name) = en
            .child_by_field_name("name")
            .and_then(|n| text(n, ctx.source))
        else {
            continue;
        };
        let Some(body) = en.child_by_field_name("body") else {
            continue;
        };
        for v in kids(body) {
            if v.kind() != "enum_variant" {
                continue;
            }
            let line = v.start_position().row + 1;
            let payload = variant_payload(ctx.source, v);
            if let Some((_, to)) = resolve_type_ref(&payload, &known) {
                edges.push(TypeEdge {
                    from: format!("{}:{}", ctx.file, name),
                    to,
                    kind: TypeEdgeKind::Variant,
                    line,
                });
            }
        }
    }

    (types, edges, known)
}

fn type_node_from(ctx: &Ctx, item: Node) -> Option<TypeNode> {
    let name = item
        .child_by_field_name("name")
        .and_then(|n| text(n, ctx.source))?
        .to_string();
    let kind = match item.kind() {
        "struct_item" => TypeKind::Struct,
        "enum_item" => TypeKind::Enum,
        "trait_item" => TypeKind::Trait,
        _ => return None,
    };
    let line = item.start_position().row + 1;
    let doc = ctx
        .symbols
        .iter()
        .find(|s| s.start_line == line && s.name == name)
        .map(|s| s.doc.clone())
        .unwrap_or_default();
    Some(TypeNode {
        id: format!("{}:{}", ctx.file, name),
        name,
        kind,
        file: ctx.file.to_string(),
        line,
        fields: collect_fields(ctx.source, item),
        methods: Vec::new(),
        doc,
        module_path: module_path(ctx.file),
    })
}

fn collect_fields(source: &str, item: Node) -> Vec<Field> {
    let Some(body) = item.child_by_field_name("body") else {
        return Vec::new();
    };
    match item.kind() {
        "struct_item" => struct_fields(source, body),
        "enum_item" => {
            let mut out = Vec::new();
            for v in kids(body) {
                if v.kind() != "enum_variant" {
                    continue;
                }
                let name = v
                    .child_by_field_name("name")
                    .and_then(|n| text(n, source))
                    .unwrap_or("")
                    .to_string();
                out.push(Field {
                    name,
                    type_name: variant_payload(source, v),
                    visibility: visibility(v, source),
                    line: v.start_position().row + 1,
                });
            }
            out
        }
        _ => Vec::new(),
    }
}

fn struct_fields(source: &str, body: Node) -> Vec<Field> {
    let mut out = Vec::new();
    match body.kind() {
        "field_declaration_list" => {
            for c in kids(body) {
                if c.kind() != "field_declaration" {
                    continue;
                }
                let Some(name) = c.child_by_field_name("name").and_then(|n| text(n, source)) else {
                    continue;
                };
                out.push(Field {
                    name: name.to_string(),
                    type_name: c
                        .child_by_field_name("type")
                        .and_then(|n| text(n, source))
                        .unwrap_or("")
                        .to_string(),
                    visibility: visibility(c, source),
                    line: c.start_position().row + 1,
                });
            }
        }
        "ordered_field_declaration_list" => {
            let mut idx = 0usize;
            for c in kids(body) {
                if c.kind() == "visibility_modifier" {
                    continue;
                }
                if let Some(ty) = text(c, source) {
                    out.push(Field {
                        name: idx.to_string(),
                        type_name: ty.to_string(),
                        visibility: visibility(c, source),
                        line: c.start_position().row + 1,
                    });
                    idx += 1;
                }
            }
        }
        _ => {}
    }
    out
}

fn variant_payload(source: &str, variant: Node) -> String {
    let Some(body) = variant.child_by_field_name("body") else {
        return String::new();
    };
    match body.kind() {
        "field_declaration_list" => kids(body)
            .iter()
            .filter_map(|f| f.child_by_field_name("type").and_then(|t| text(t, source)))
            .collect::<Vec<_>>()
            .join(", "),
        "ordered_field_declaration_list" => kids(body)
            .iter()
            .filter(|n| n.kind() != "visibility_modifier")
            .filter_map(|n| text(*n, source))
            .collect::<Vec<_>>()
            .join(", "),
        _ => String::new(),
    }
}

fn collect_method_names(
    body: Node,
    source: &str,
    type_name: &str,
    into: &mut BTreeMap<String, BTreeSet<String>>,
) {
    for f in kids(body) {
        if f.kind() != "function_item" && f.kind() != "function_signature_item" {
            continue;
        }
        if let Some(name) = f.child_by_field_name("name").and_then(|n| text(n, source)) {
            into.entry(type_name.to_string())
                .or_default()
                .insert(name.to_string());
        }
    }
}

fn impl_type_name(imp: Node, source: &str) -> Option<String> {
    let ty = imp.child_by_field_name("type")?;
    first_type_ident(ty)
        .and_then(|n| text(n, source))
        .map(str::to_string)
}

fn first_type_ident(node: Node) -> Option<Node> {
    if node.kind() == "type_identifier" {
        return Some(node);
    }
    for k in kids(node) {
        if let Some(n) = first_type_ident(k) {
            return Some(n);
        }
    }
    None
}

/// First identifier token in `text` that names a declared type.
fn resolve_type_ref(text: &str, known: &BTreeMap<String, String>) -> Option<(String, String)> {
    let mut cur = String::new();
    for ch in text.chars().chain(std::iter::once(' ')) {
        if ch.is_alphanumeric() || ch == '_' {
            cur.push(ch);
        } else if !cur.is_empty() {
            if let Some(id) = known.get(&cur) {
                return Some((cur, id.clone()));
            }
            cur.clear();
        }
    }
    None
}

fn module_path(file: &str) -> String {
    let p = file.replace('\\', "/");
    let stem = p.strip_suffix(".rs").unwrap_or(&p);
    let mut segs: Vec<&str> = stem.split('/').filter(|s| !s.is_empty()).collect();
    if let Some(pos) = segs.iter().position(|s| *s == "src") {
        segs.drain(0..=pos);
    }
    if segs
        .last()
        .is_some_and(|last| matches!(*last, "lib" | "main" | "mod"))
    {
        segs.pop();
    }
    if segs.is_empty() {
        "crate".to_string()
    } else {
        format!("crate::{}", segs.join("::"))
    }
}

// ---------------------------------------------------------------------------
// variables + variable/field use sites
// ---------------------------------------------------------------------------

struct Draft {
    var: Variable,
    decl_byte: usize,
    decl_end: usize,
    owner: Option<String>,
}

struct VarBuilder<'a> {
    ctx: &'a Ctx<'a>,
    drafts: Vec<Draft>,
    bindings: BTreeSet<(usize, usize)>,
    uses: Vec<UseSite>,
}

impl VarBuilder<'_> {
    fn push(
        &mut self,
        name_node: Node,
        kind: VarKind,
        decl_line: usize,
        scope: (usize, usize),
        type_name: Option<String>,
    ) -> Option<usize> {
        let name = text(name_node, self.ctx.source).unwrap_or("");
        if name.is_empty() {
            return None;
        }
        self.bindings
            .insert((name_node.start_byte(), name_node.end_byte()));
        let id = format!("{}:{}:{}", self.ctx.file, decl_line, name);
        self.drafts.push(Draft {
            var: Variable {
                id,
                name: name.to_string(),
                kind,
                file: self.ctx.file.to_string(),
                decl_line,
                scope_start: scope.0,
                scope_end: scope.1,
                type_name,
                reads: 0,
                writes: 0,
                first_use: None,
                last_use: None,
                enclosing: enclosing_symbol(self.ctx.symbols, decl_line),
            },
            decl_byte: name_node.start_byte(),
            decl_end: name_node.end_byte(),
            owner: None,
        });
        Some(self.drafts.len() - 1)
    }

    fn bump(
        &mut self,
        idx: usize,
        read: bool,
        write: bool,
        line: usize,
        col: usize,
        kind: UseKind,
    ) {
        let d = &mut self.drafts[idx];
        if read {
            d.var.reads += 1;
        }
        if write {
            d.var.writes += 1;
        }
        d.var.first_use = Some(d.var.first_use.map_or(line, |f| f.min(line)));
        d.var.last_use = Some(d.var.last_use.map_or(line, |l| l.max(line)));
        self.uses.push(UseSite {
            target: d.var.id.clone(),
            file: self.ctx.file.to_string(),
            line,
            col,
            kind,
            snippet: snippet(self.ctx.source, line),
        });
    }
}

fn collect_variables(ctx: &Ctx, items: &Items) -> (Vec<Variable>, Vec<UseSite>) {
    let mut b = VarBuilder {
        ctx,
        drafts: Vec::new(),
        bindings: BTreeSet::new(),
        uses: Vec::new(),
    };

    // parameters (function + closure)
    for p in &items.params {
        let pattern = p.child_by_field_name("pattern").unwrap_or(*p);
        let ty = p
            .child_by_field_name("type")
            .and_then(|n| text(n, ctx.source))
            .map(str::to_string);
        let decl_line = p.start_position().row + 1;
        let scope = owner_range(*p).unwrap_or((decl_line, ctx.line_count));
        let ids = pattern_bindings(pattern);
        let simple = ids.len() == 1;
        for id in ids {
            let ty = if simple { ty.clone() } else { None };
            b.push(id, VarKind::Param, decl_line, scope, ty);
        }
    }

    // `let` locals
    for l in &items.lets {
        let Some(pattern) = l.child_by_field_name("pattern") else {
            continue;
        };
        let ty = l
            .child_by_field_name("type")
            .and_then(|n| text(n, ctx.source))
            .map(str::to_string);
        let decl_line = l.start_position().row + 1;
        let scope = nearest_of(*l, &["block"])
            .map(range)
            .unwrap_or((decl_line, ctx.line_count));
        let ids = pattern_bindings(pattern);
        let simple = ids.len() == 1;
        for id in ids.iter().copied() {
            let ty = if simple { ty.clone() } else { None };
            if let Some(idx) = b.push(id, VarKind::Local, decl_line, scope, ty) {
                b.bump(
                    idx,
                    false,
                    true,
                    decl_line,
                    id.start_position().column + 1,
                    UseKind::VarWrite,
                );
            }
        }
    }

    // const/static globals
    for c in items.consts.iter().chain(items.statics.iter()) {
        let kind = if c.kind() == "static_item" {
            VarKind::Static
        } else {
            VarKind::Global
        };
        let Some(name_node) = c.child_by_field_name("name") else {
            continue;
        };
        let ty = c
            .child_by_field_name("type")
            .and_then(|n| text(n, ctx.source))
            .map(str::to_string);
        let decl_line = c.start_position().row + 1;
        let scope = nearest_of(*c, &["mod_item"])
            .map(range)
            .unwrap_or((1, ctx.line_count));
        if let Some(idx) = b.push(name_node, kind, decl_line, scope, ty) {
            b.bump(
                idx,
                false,
                true,
                decl_line,
                name_node.start_position().column + 1,
                UseKind::VarWrite,
            );
        }
    }

    // struct / enum fields
    for f in &items.fields {
        let Some(name_node) = f.child_by_field_name("name") else {
            continue;
        };
        let ty = f
            .child_by_field_name("type")
            .and_then(|n| text(n, ctx.source))
            .map(str::to_string);
        let decl_line = f.start_position().row + 1;
        let owner_node = nearest_of(*f, &["struct_item", "enum_item"]);
        let scope = owner_node.map(range).unwrap_or((decl_line, decl_line));
        let owner = owner_node
            .and_then(|o| o.child_by_field_name("name"))
            .and_then(|n| text(n, ctx.source))
            .map(str::to_string);
        if let Some(idx) = b.push(name_node, VarKind::Field, decl_line, scope, ty) {
            b.drafts[idx].owner = owner;
        }
    }

    // exclude pattern bindings we do not model as variables (for/match/if-let)
    for f in &items.for_exprs {
        if let Some(p) = f.child_by_field_name("pattern") {
            for id in pattern_bindings(p) {
                b.bindings.insert((id.start_byte(), id.end_byte()));
            }
        }
    }
    for m in &items.match_arms {
        if let Some(p) = m.child_by_field_name("pattern") {
            for id in pattern_bindings(p) {
                b.bindings.insert((id.start_byte(), id.end_byte()));
            }
        }
    }
    for lc in &items.let_conditions {
        if let Some(p) = lc.child_by_field_name("pattern") {
            for id in pattern_bindings(p) {
                b.bindings.insert((id.start_byte(), id.end_byte()));
            }
        }
    }
    for f in &items.functions {
        if let Some(n) = f.child_by_field_name("name") {
            b.bindings.insert((n.start_byte(), n.end_byte()));
        }
    }

    // identifier reads / writes
    for id in &items.identifiers {
        let r = (id.start_byte(), id.end_byte());
        if b.bindings.contains(&r) {
            continue;
        }
        let Some(name) = text(*id, ctx.source) else {
            continue;
        };
        let line = id.start_position().row + 1;
        let byte = id.start_byte();
        let Some(idx) = attribute(&b.drafts, name, line, byte) else {
            continue;
        };
        let (read, write) = ident_role(*id);
        let kind = if write {
            UseKind::VarWrite
        } else {
            UseKind::VarRead
        };
        b.bump(idx, read, write, line, id.start_position().column + 1, kind);
    }

    // field accesses: `value.field`
    for fe in &items.field_accesses {
        let Some(fnode) = fe.child_by_field_name("field") else {
            continue;
        };
        let Some(fname) = text(fnode, ctx.source) else {
            continue;
        };
        let line = fe.start_position().row + 1;
        let col = fnode.start_position().column + 1;
        let container = enclosing_method_container(ctx.symbols, line);
        let (read, write) = field_role(*fe);
        match pick_field(&b.drafts, fname, container.as_deref()) {
            Some(idx) => b.bump(idx, read, write, line, col, UseKind::FieldAccess),
            None => b.uses.push(UseSite {
                target: fname.to_string(),
                file: ctx.file.to_string(),
                line,
                col,
                kind: UseKind::FieldAccess,
                snippet: snippet(ctx.source, line),
            }),
        }
    }

    let variables = b.drafts.into_iter().map(|d| d.var).collect();
    (variables, b.uses)
}

fn attribute(drafts: &[Draft], name: &str, line: usize, byte: usize) -> Option<usize> {
    let mut best: Option<usize> = None;
    for (i, d) in drafts.iter().enumerate() {
        let v = &d.var;
        if v.name != name {
            continue;
        }
        if line < v.scope_start || line > v.scope_end {
            continue;
        }
        let visible = line > v.decl_line || (line == v.decl_line && byte > d.decl_end);
        if !visible {
            continue;
        }
        match best {
            None => best = Some(i),
            Some(j) => {
                if (v.decl_line, d.decl_byte) > (drafts[j].var.decl_line, drafts[j].decl_byte) {
                    best = Some(i);
                }
            }
        }
    }
    best
}

fn pick_field(drafts: &[Draft], name: &str, container: Option<&str>) -> Option<usize> {
    let mut fallback = None;
    for (i, d) in drafts.iter().enumerate() {
        if d.var.kind != VarKind::Field || d.var.name != name {
            continue;
        }
        if let (Some(c), Some(owner)) = (container, d.owner.as_deref()) {
            if c == owner {
                return Some(i);
            }
        }
        fallback = fallback.or(Some(i));
    }
    fallback
}

fn ident_role(id: Node) -> (bool, bool) {
    if let Some(p) = id.parent() {
        let pk = p.kind();
        if pk == "assignment_expression" || pk == "compound_assignment_expr" {
            if let Some(left) = p.child_by_field_name("left") {
                if left.start_byte() == id.start_byte() && left.end_byte() == id.end_byte() {
                    return (pk == "compound_assignment_expr", true);
                }
            }
        }
    }
    (true, false)
}

fn field_role(fe: Node) -> (bool, bool) {
    if let Some(p) = fe.parent() {
        let pk = p.kind();
        if pk == "assignment_expression" || pk == "compound_assignment_expr" {
            if let Some(left) = p.child_by_field_name("left") {
                if left.start_byte() == fe.start_byte() && left.end_byte() == fe.end_byte() {
                    return (pk == "compound_assignment_expr", true);
                }
            }
        }
    }
    (true, false)
}

fn enclosing_method_container(symbols: &[Symbol], line: usize) -> Option<String> {
    enclosing_symbol_at(symbols, line)
        .filter(|s| s.kind == SymbolKind::Method)
        .and_then(|s| s.container.clone())
}

// ---------------------------------------------------------------------------
// calls + type references
// ---------------------------------------------------------------------------

fn collect_use_sites(
    ctx: &Ctx,
    items: &Items,
    known: &BTreeMap<String, String>,
    uses: &mut Vec<UseSite>,
) {
    for call in &items.calls {
        let Some(callee) = call
            .child_by_field_name("function")
            .and_then(|f| callee_name(f, ctx.source))
        else {
            continue;
        };
        let line = call.start_position().row + 1;
        let col = call.start_position().column + 1;
        let target = resolve_callee(ctx, &callee, line).unwrap_or(callee);
        uses.push(UseSite {
            target,
            file: ctx.file.to_string(),
            line,
            col,
            kind: UseKind::Call,
            snippet: snippet(ctx.source, line),
        });
    }

    for t in &items.type_ids {
        if is_decl_name(*t) {
            continue;
        }
        let Some(name) = text(*t, ctx.source) else {
            continue;
        };
        let Some(id) = known.get(name) else {
            continue;
        };
        let line = t.start_position().row + 1;
        let col = t.start_position().column + 1;
        uses.push(UseSite {
            target: id.clone(),
            file: ctx.file.to_string(),
            line,
            col,
            kind: UseKind::TypeRef,
            snippet: snippet(ctx.source, line),
        });
    }
}

fn callee_name(func: Node, source: &str) -> Option<String> {
    match func.kind() {
        "identifier" => text(func, source).map(str::to_string),
        "scoped_identifier" => func
            .child_by_field_name("name")
            .and_then(|n| text(n, source))
            .map(str::to_string),
        "field_expression" => func
            .child_by_field_name("field")
            .and_then(|n| text(n, source))
            .map(str::to_string),
        "generic_function" => func
            .child_by_field_name("function")
            .and_then(|n| callee_name(n, source)),
        _ => None,
    }
}

fn resolve_callee(ctx: &Ctx, callee: &str, line: usize) -> Option<String> {
    if let Some(container) = enclosing_method_container(ctx.symbols, line) {
        if let Some(s) = ctx
            .symbols
            .iter()
            .filter(|s| s.name == callee && s.container.as_deref() == Some(container.as_str()))
            .min_by_key(|s| s.start_line)
        {
            return Some(s.id.clone());
        }
    }
    ctx.symbols
        .iter()
        .filter(|s| s.name == callee && s.file == ctx.file)
        .min_by_key(|s| s.start_line)
        .or_else(|| {
            ctx.symbols
                .iter()
                .filter(|s| s.name == callee)
                .min_by_key(|s| s.start_line)
        })
        .map(|s| s.id.clone())
}

fn is_decl_name(node: Node) -> bool {
    let Some(p) = node.parent() else {
        return false;
    };
    if !matches!(
        p.kind(),
        "struct_item" | "enum_item" | "trait_item" | "type_item"
    ) {
        return false;
    }
    p.child_by_field_name("name")
        .is_some_and(|n| n.start_byte() == node.start_byte() && n.end_byte() == node.end_byte())
}

// ---------------------------------------------------------------------------
// control-flow graph
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
struct Entry {
    node: u32,
    kind: CfgEdgeKind,
    line: usize,
}

struct CfgBuilder<'a> {
    source: &'a str,
    nodes: Vec<CfgNode>,
    edges: Vec<CfgEdge>,
    decisions: u32,
    max_nesting: u32,
    exit_id: u32,
}

impl CfgBuilder<'_> {
    fn add_node(&mut self, kind: CfgKind, start: usize, end: usize, label: String) -> u32 {
        let id = self.nodes.len() as u32;
        self.nodes.push(CfgNode {
            id,
            kind,
            start_line: start,
            end_line: end,
            label,
        });
        id
    }

    fn connect(&mut self, entries: &[Entry], to: u32) {
        for e in entries {
            self.edges.push(CfgEdge {
                from: e.node,
                to,
                kind: e.kind,
                line: e.line,
            });
        }
    }

    fn flush(&mut self, entries: &mut Vec<Entry>, pending: &mut Option<(usize, usize, String)>) {
        if let Some((start, end, label)) = pending.take() {
            let id = self.add_node(CfgKind::Block, start, end, label);
            self.connect(entries, id);
            *entries = vec![Entry {
                node: id,
                kind: CfgEdgeKind::Normal,
                line: start,
            }];
        }
    }

    fn emit_stmts(&mut self, stmts: &[Node], mut entries: Vec<Entry>, depth: u32) -> Vec<Entry> {
        let mut pending: Option<(usize, usize, String)> = None;
        for &st in stmts {
            let node = if st.kind() == "expression_statement" {
                single_named_child(st).unwrap_or(st)
            } else {
                st
            };
            match node.kind() {
                "if_expression" => {
                    self.flush(&mut entries, &mut pending);
                    entries = self.emit_if(node, entries, depth);
                }
                "loop_expression" | "while_expression" | "for_expression" => {
                    self.flush(&mut entries, &mut pending);
                    entries = self.emit_loop(node, entries, depth);
                }
                "match_expression" => {
                    self.flush(&mut entries, &mut pending);
                    entries = self.emit_match(node, entries, depth);
                }
                "return_expression" => {
                    self.flush(&mut entries, &mut pending);
                    entries = self.emit_return(node, entries);
                }
                "call_expression" => {
                    self.flush(&mut entries, &mut pending);
                    entries = self.emit_call(node, entries);
                }
                "block" => {
                    self.flush(&mut entries, &mut pending);
                    let cs = kids(node);
                    entries = self.emit_stmts(&cs, entries, depth);
                }
                _ => {
                    let s = st.start_position().row + 1;
                    let e = st.end_position().row + 1;
                    match &mut pending {
                        Some((_, pe, _)) => *pe = e,
                        None => pending = Some((s, e, st_label(st, self.source))),
                    }
                }
            }
        }
        self.flush(&mut entries, &mut pending);
        entries
    }

    fn emit_if(&mut self, node: Node, entries: Vec<Entry>, depth: u32) -> Vec<Entry> {
        let line = node.start_position().row + 1;
        let (cl, ce, label) = match node.child_by_field_name("condition") {
            Some(c) => (
                c.start_position().row + 1,
                c.end_position().row + 1,
                clip(text(c, self.source).unwrap_or("if")),
            ),
            None => (line, line, "if".to_string()),
        };
        let id = self.add_node(CfgKind::Branch, cl, ce, label);
        self.connect(&entries, id);
        self.decisions += 1;
        self.max_nesting = self.max_nesting.max(depth + 1);

        let true_entries = vec![Entry {
            node: id,
            kind: CfgEdgeKind::True,
            line,
        }];
        let false_entries = vec![Entry {
            node: id,
            kind: CfgEdgeKind::False,
            line,
        }];
        let cons_exits = match node.child_by_field_name("consequence") {
            Some(c) => {
                let cs = kids(c);
                self.emit_stmts(&cs, true_entries, depth + 1)
            }
            None => true_entries,
        };
        let alt_exits = match node.child_by_field_name("alternative") {
            Some(a) => self.emit_else(a, false_entries, depth),
            None => false_entries,
        };
        cons_exits.into_iter().chain(alt_exits).collect()
    }

    fn emit_else(&mut self, alt: Node, entries: Vec<Entry>, depth: u32) -> Vec<Entry> {
        match single_named_child(alt) {
            Some(c) if c.kind() == "if_expression" => self.emit_if(c, entries, depth),
            Some(c) => {
                let cs = if c.kind() == "block" {
                    kids(c)
                } else {
                    vec![c]
                };
                self.emit_stmts(&cs, entries, depth + 1)
            }
            None => entries,
        }
    }

    fn emit_loop(&mut self, node: Node, entries: Vec<Entry>, depth: u32) -> Vec<Entry> {
        let start = node.start_position().row + 1;
        let end = node.end_position().row + 1;
        let label = match node.kind() {
            "while_expression" => node
                .child_by_field_name("condition")
                .and_then(|c| text(c, self.source))
                .map(clip)
                .unwrap_or_else(|| "while".to_string()),
            "for_expression" => {
                let pat = node
                    .child_by_field_name("pattern")
                    .and_then(|p| text(p, self.source))
                    .unwrap_or("_");
                let val = node
                    .child_by_field_name("value")
                    .and_then(|v| text(v, self.source))
                    .unwrap_or("");
                clip(&format!("for {pat} in {val}"))
            }
            _ => "loop".to_string(),
        };
        let id = self.add_node(CfgKind::Loop, start, end, label);
        self.connect(&entries, id);
        self.decisions += 1;
        self.max_nesting = self.max_nesting.max(depth + 1);

        let is_cond = node.kind() != "loop_expression";
        let body_kind = if is_cond {
            CfgEdgeKind::True
        } else {
            CfgEdgeKind::Normal
        };
        let body_entries = vec![Entry {
            node: id,
            kind: body_kind,
            line: start,
        }];
        let body_exits = match node.child_by_field_name("body") {
            Some(b) => {
                let cs = kids(b);
                self.emit_stmts(&cs, body_entries, depth + 1)
            }
            None => body_entries,
        };
        for e in &body_exits {
            self.edges.push(CfgEdge {
                from: e.node,
                to: id,
                kind: CfgEdgeKind::Back,
                line: e.line,
            });
        }
        let exit_kind = if is_cond {
            CfgEdgeKind::False
        } else {
            CfgEdgeKind::Normal
        };
        vec![Entry {
            node: id,
            kind: exit_kind,
            line: start,
        }]
    }

    fn emit_match(&mut self, node: Node, entries: Vec<Entry>, depth: u32) -> Vec<Entry> {
        let start = node.start_position().row + 1;
        let end = node.end_position().row + 1;
        let label = node
            .child_by_field_name("value")
            .and_then(|v| text(v, self.source))
            .map(clip)
            .unwrap_or_else(|| "match".to_string());
        let id = self.add_node(CfgKind::Match, start, end, label);
        self.connect(&entries, id);
        self.max_nesting = self.max_nesting.max(depth + 1);

        let mut outs: Vec<Entry> = Vec::new();
        if let Some(body) = node.child_by_field_name("body") {
            for arm in kids(body) {
                if arm.kind() != "match_arm" {
                    continue;
                }
                self.decisions += 1;
                let arm_entries = vec![Entry {
                    node: id,
                    kind: CfgEdgeKind::Normal,
                    line: start,
                }];
                let exits = match arm.child_by_field_name("value") {
                    Some(v) if v.kind() == "block" => {
                        let cs = kids(v);
                        self.emit_stmts(&cs, arm_entries, depth + 1)
                    }
                    Some(v) => {
                        let nid = self.add_node(
                            CfgKind::Block,
                            v.start_position().row + 1,
                            v.end_position().row + 1,
                            clip(text(v, self.source).unwrap_or("arm")),
                        );
                        self.connect(&arm_entries, nid);
                        vec![Entry {
                            node: nid,
                            kind: CfgEdgeKind::Normal,
                            line: v.start_position().row + 1,
                        }]
                    }
                    None => arm_entries,
                };
                outs.extend(exits);
            }
        }
        if outs.is_empty() {
            outs.push(Entry {
                node: id,
                kind: CfgEdgeKind::Normal,
                line: start,
            });
        }
        outs
    }

    fn emit_call(&mut self, node: Node, entries: Vec<Entry>) -> Vec<Entry> {
        let label = node
            .child_by_field_name("function")
            .and_then(|f| callee_name(f, self.source))
            .unwrap_or_else(|| "call".to_string());
        let l = node.start_position().row + 1;
        let id = self.add_node(CfgKind::Call, l, node.end_position().row + 1, label);
        self.connect(&entries, id);
        vec![Entry {
            node: id,
            kind: CfgEdgeKind::Normal,
            line: l,
        }]
    }

    fn emit_return(&mut self, node: Node, entries: Vec<Entry>) -> Vec<Entry> {
        let l = node.start_position().row + 1;
        let id = self.add_node(
            CfgKind::Return,
            l,
            node.end_position().row + 1,
            "return".into(),
        );
        self.connect(&entries, id);
        self.edges.push(CfgEdge {
            from: id,
            to: self.exit_id,
            kind: CfgEdgeKind::Normal,
            line: l,
        });
        Vec::new()
    }
}

fn build_cfg(ctx: &Ctx, func: Node) -> FunctionCfg {
    let start = func.start_position().row + 1;
    let end = func.end_position().row + 1;
    let symbol = ctx
        .symbols
        .iter()
        .find(|s| {
            s.start_line == start && matches!(s.kind, SymbolKind::Function | SymbolKind::Method)
        })
        .map(|s| s.id.clone())
        .unwrap_or_else(|| format!("{}:{}", ctx.file, start));

    let mut b = CfgBuilder {
        source: ctx.source,
        nodes: Vec::new(),
        edges: Vec::new(),
        decisions: 0,
        max_nesting: 0,
        exit_id: 0,
    };
    let entry = b.add_node(CfgKind::Entry, start, start, "entry".into());
    let exit = b.add_node(CfgKind::Exit, end, end, "exit".into());
    b.exit_id = exit;

    let body_entries = vec![Entry {
        node: entry,
        kind: CfgEdgeKind::Normal,
        line: start,
    }];
    let final_entries = match func.child_by_field_name("body") {
        Some(body) => {
            let cs = kids(body);
            b.emit_stmts(&cs, body_entries, 0)
        }
        None => body_entries,
    };
    for e in &final_entries {
        b.edges.push(CfgEdge {
            from: e.node,
            to: exit,
            kind: CfgEdgeKind::Normal,
            line: e.line,
        });
    }

    let bools = count_bool_ops(func, ctx.source);
    FunctionCfg {
        symbol,
        nodes: b.nodes,
        edges: b.edges,
        cyclomatic: 1 + b.decisions + bools,
        max_nesting: b.max_nesting,
    }
}

fn count_bool_ops(node: Node, source: &str) -> u32 {
    let mut n = 0;
    if node.kind() == "binary_expression" {
        if let Some(op) = node.child_by_field_name("operator") {
            if matches!(text(op, source), Some("&&") | Some("||")) {
                n += 1;
            }
        }
    }
    for k in kids(node) {
        n += count_bool_ops(k, source);
    }
    n
}

// ---------------------------------------------------------------------------
// shared AST helpers
// ---------------------------------------------------------------------------

fn kids<'t>(node: Node<'t>) -> Vec<Node<'t>> {
    let mut cur = node.walk();
    node.named_children(&mut cur).collect()
}

fn single_named_child(node: Node) -> Option<Node> {
    let cs = kids(node);
    if cs.len() == 1 {
        Some(cs[0])
    } else {
        None
    }
}

fn text<'a>(node: Node, source: &'a str) -> Option<&'a str> {
    node.utf8_text(source.as_bytes()).ok()
}

fn range(node: Node) -> (usize, usize) {
    (node.start_position().row + 1, node.end_position().row + 1)
}

fn nearest_of<'t>(node: Node<'t>, kinds: &[&str]) -> Option<Node<'t>> {
    let mut cur = node.parent();
    while let Some(p) = cur {
        if kinds.contains(&p.kind()) {
            return Some(p);
        }
        cur = p.parent();
    }
    None
}

fn owner_range(node: Node) -> Option<(usize, usize)> {
    let mut cur = node.parent();
    while let Some(p) = cur {
        if matches!(p.kind(), "function_item" | "closure_expression") {
            return Some(range(p));
        }
        cur = p.parent();
    }
    None
}

fn pattern_bindings<'t>(node: Node<'t>) -> Vec<Node<'t>> {
    let mut out = Vec::new();
    collect_bindings(node, &mut out);
    out
}

fn collect_bindings<'t>(node: Node<'t>, out: &mut Vec<Node<'t>>) {
    match node.kind() {
        "identifier" => {
            out.push(node);
            return;
        }
        "type_identifier"
        | "field_identifier"
        | "shorthand_field_identifier"
        | "self"
        | "mutable_specifier"
        | "closure_expression"
        | "integer_literal"
        | "float_literal"
        | "string_literal"
        | "boolean_literal" => return,
        _ => {}
    }
    let guard = if node.kind() == "match_pattern" {
        node.child_by_field_name("condition")
    } else {
        None
    };
    for k in kids(node) {
        if let Some(g) = guard {
            if g.start_byte() == k.start_byte() && g.end_byte() == k.end_byte() {
                continue;
            }
        }
        collect_bindings(k, out);
    }
}

fn visibility(node: Node, source: &str) -> String {
    let mut cur = node.walk();
    for c in node.children(&mut cur) {
        if c.kind() == "visibility_modifier" {
            return text(c, source).unwrap_or("").trim().to_string();
        }
    }
    "private".to_string()
}

fn st_label(node: Node, source: &str) -> String {
    clip(text(node, source).unwrap_or(""))
}

fn clip(s: &str) -> String {
    let one: String = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if one.chars().count() > 60 {
        let mut t: String = one.chars().take(57).collect();
        t.push_str("...");
        t
    } else {
        one
    }
}

// ---------------------------------------------------------------------------
// deterministic ordering
// ---------------------------------------------------------------------------

fn edge_kind_rank(k: TypeEdgeKind) -> u8 {
    match k {
        TypeEdgeKind::Field => 0,
        TypeEdgeKind::Impl => 1,
        TypeEdgeKind::Inherit => 2,
        TypeEdgeKind::Variant => 3,
    }
}

fn use_kind_rank(k: UseKind) -> u8 {
    match k {
        UseKind::Call => 0,
        UseKind::TypeRef => 1,
        UseKind::FieldAccess => 2,
        UseKind::VarRead => 3,
        UseKind::VarWrite => 4,
        UseKind::Import => 5,
    }
}

/// Stable output order so goldens never churn.
fn sort_analysis(out: &mut Analysis) {
    out.variables.sort_by(|a, b| {
        (a.file.as_str(), a.decl_line, a.name.as_str(), a.id.as_str()).cmp(&(
            b.file.as_str(),
            b.decl_line,
            b.name.as_str(),
            b.id.as_str(),
        ))
    });
    out.cfgs.sort_by(|a, b| a.symbol.cmp(&b.symbol));
    out.types.sort_by(|a, b| {
        (a.file.as_str(), a.line, a.name.as_str()).cmp(&(b.file.as_str(), b.line, b.name.as_str()))
    });
    out.type_edges.sort_by(|a, b| {
        (
            a.line,
            a.from.as_str(),
            a.to.as_str(),
            edge_kind_rank(a.kind),
        )
            .cmp(&(
                b.line,
                b.from.as_str(),
                b.to.as_str(),
                edge_kind_rank(b.kind),
            ))
    });
    out.uses.sort_by(|a, b| {
        (
            a.file.as_str(),
            a.line,
            a.col,
            use_kind_rank(a.kind),
            a.target.as_str(),
        )
            .cmp(&(
                b.file.as_str(),
                b.line,
                b.col,
                use_kind_rank(b.kind),
                b.target.as_str(),
            ))
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::{CfgEdgeKind, CfgKind, TypeEdgeKind};
    use crate::parse_rust;

    fn run(src: &str) -> (Analysis, Vec<Symbol>) {
        let (symbols, _, _) = parse_rust::extract(src, "a.rs");
        let mut out = Analysis::new();
        analyze("a.rs", src, &symbols, &mut out);
        (out, symbols)
    }

    #[test]
    fn if_else_and_loop_cyclomatic_and_back_edge() {
        let src = r#"
fn f(n: i32) -> i32 {
    let mut s = 0;
    for i in 0..n {
        if i % 2 == 0 {
            s += i;
        } else {
            s -= i;
        }
    }
    s
}
"#;
        let (a, symbols) = run(src);
        let sym = symbols.iter().find(|s| s.name == "f").unwrap();
        let cfg = a.cfgs.iter().find(|c| c.symbol == sym.id).unwrap();
        // one `for` + one `if` = 2 decision points -> cyclomatic 3.
        assert_eq!(cfg.cyclomatic, 3, "cyclomatic");
        assert_eq!(cfg.max_nesting, 2, "if nested inside for");
        assert!(
            cfg.nodes.iter().any(|n| n.kind == CfgKind::Branch),
            "branch node"
        );
        assert!(
            cfg.edges.iter().any(|e| e.kind == CfgEdgeKind::Back),
            "loop back edge"
        );
    }

    #[test]
    fn unused_let_has_zero_reads_and_used_has_reads() {
        let src = r#"
fn g() {
    let unused = 1;
    let used = 2;
    println!("{}", used);
}
"#;
        let (a, _) = run(src);
        let unused = a.variables.iter().find(|v| v.name == "unused").unwrap();
        let used = a.variables.iter().find(|v| v.name == "used").unwrap();
        assert_eq!(unused.kind, VarKind::Local);
        assert_eq!(unused.reads, 0, "unused local reads");
        assert_eq!(unused.writes, 1, "declaration is one write");
        assert!(used.reads >= 1, "used local reads");
    }

    #[test]
    fn struct_field_type_produces_field_edge() {
        let src = r#"
struct Inner {
    v: i32,
}

struct Outer {
    inner: Inner,
}
"#;
        let (a, _) = run(src);
        let inner = a.types.iter().find(|t| t.name == "Inner").unwrap();
        let outer = a.types.iter().find(|t| t.name == "Outer").unwrap();
        assert!(
            a.type_edges
                .iter()
                .any(|e| e.kind == TypeEdgeKind::Field && e.from == outer.id && e.to == inner.id),
            "expected Field edge Outer -> Inner; edges: {:?}",
            a.type_edges
        );
    }
}
