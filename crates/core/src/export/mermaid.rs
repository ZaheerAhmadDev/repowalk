//! Mermaid emitter. OWNER: PROD-2.
//!
//! Pure and deterministic: `(index, analysis, diagram)` in, diagram body out.
//! Call graph / CFG / modules render from the shared [`super::util::Graph`];
//! the type graph uses Mermaid's `classDiagram` syntax directly.
use super::util::{self, Arrow, GNode, Graph, Shape, Style};
use super::Diagram;
use crate::analysis::{Analysis, TypeEdgeKind};
use crate::model::Index;

/// Render `diagram` as a Mermaid diagram body (no fence).
pub fn render(index: &Index, analysis: &Analysis, diagram: &Diagram) -> String {
    match diagram {
        Diagram::CallGraph => {
            let g = util::build_call_graph(index, util::FLOW_NODE_CAP);
            flow(&g, "LR", "call graph")
        }
        Diagram::Cfg { symbol } => {
            let g = util::build_cfg(analysis, symbol);
            flow(&g, "TD", "CFG")
        }
        Diagram::Types => class_diagram(analysis),
        Diagram::Modules => {
            let g = util::build_modules(analysis);
            flow(&g, "LR", "module graph")
        }
        Diagram::Directory { imports } => {
            let options = super::directory::DirectoryOptions {
                import_edges: *imports,
                loc_labels: true,
            };
            let g = super::directory::build(index, analysis, &options);
            flow(&g, "TD", "directory tree")
        }
    }
}

/// Render a generic graph as a Mermaid `flowchart`.
fn flow(g: &Graph, direction: &str, kind: &str) -> String {
    let mut out = format!("flowchart {direction}\n");
    out.push_str(&format!("%% repowalk {kind}"));
    if let Some(caption) = &g.caption {
        out.push_str(": ");
        out.push_str(caption);
    }
    out.push('\n');

    for n in &g.nodes {
        out.push_str(&mermaid_node(n));
        out.push('\n');
    }
    for e in &g.edges {
        // `Arrow::Open` (plain containment lines) loses its head in Mermaid:
        // `---` solid, `-.` dotted. Labelled dotted lines use `A-. text .-B`.
        let open = e.arrow == Arrow::Open;
        let dotted = e.style == Style::Dashed;
        let mut line = e.from.clone();
        line.push(' ');
        match (open, dotted) {
            (true, true) => {
                if e.label.is_empty() {
                    line.push_str("-.-");
                    line.push(' ');
                    line.push_str(&e.to);
                    out.push_str(&line);
                    out.push('\n');
                    continue;
                }
                line.push_str("-. ");
                line.push_str(&util::mermaid_escape(&e.label));
                line.push_str(" .- ");
                line.push_str(&e.to);
                out.push_str(&line);
                out.push('\n');
                continue;
            }
            (true, false) => line.push_str("---"),
            (false, true) => line.push_str("-.->"),
            (false, false) => line.push_str("-->"),
        }
        if !e.label.is_empty() {
            line.push('|');
            line.push_str(&util::mermaid_escape(&e.label));
            line.push('|');
        }
        line.push(' ');
        line.push_str(&e.to);
        out.push_str(&line);
        out.push('\n');
    }
    out
}

fn mermaid_node(n: &GNode) -> String {
    let label = util::mermaid_escape(&n.label);
    let id = &n.id;
    match n.shape {
        Shape::Diamond => format!("{id}{{\"{label}\"}}"),
        Shape::Hexagon => format!("{id}{{{{\"{label}\"}}}}"),
        Shape::Stadium | Shape::Oval => format!("{id}([\"{label}\"])"),
        Shape::Rounded => format!("{id}(\"{label}\")"),
        Shape::Box => format!("{id}[\"{label}\"]"),
    }
}

fn class_visibility(v: &str) -> &'static str {
    let v = v.to_ascii_lowercase();
    if v.contains("private") || v == "priv" {
        "-"
    } else if v.contains("protected") || v.contains("prot") {
        "#"
    } else if v.contains("pub") || v.contains("public") {
        "+"
    } else if v.contains("crate") || v.contains("internal") || v.contains("package") {
        "~"
    } else {
        "+"
    }
}

fn type_edge_rank(kind: &TypeEdgeKind) -> u8 {
    match kind {
        TypeEdgeKind::Field => 0,
        TypeEdgeKind::Impl => 1,
        TypeEdgeKind::Inherit => 2,
        TypeEdgeKind::Variant => 3,
    }
}

/// Mermaid `classDiagram` for the type graph.
fn class_diagram(analysis: &Analysis) -> String {
    if analysis.types.is_empty() {
        let id = util::sanitize("t", "placeholder");
        return format!(
            "classDiagram\n%% repowalk types: no types declared\nclass {id}[\"no types declared\"]\n"
        );
    }

    let declared: std::collections::BTreeSet<&str> =
        analysis.types.iter().map(|t| t.id.as_str()).collect();

    let mut out = String::from("classDiagram\n");
    for t in &analysis.types {
        let id = util::sanitize("t", &t.id);
        out.push_str(&format!(
            "class {id}[\"{}\"] {{\n",
            util::mermaid_escape(&t.name)
        ));

        let mut fields = t.fields.clone();
        fields.sort_by(|a, b| a.line.cmp(&b.line).then_with(|| a.name.cmp(&b.name)));
        for f in &fields {
            out.push_str(&format!(
                "  {}{}: {}\n",
                class_visibility(&f.visibility),
                util::class_member(&f.name),
                util::class_member(&f.type_name)
            ));
        }

        let mut methods = t.methods.clone();
        methods.sort();
        methods.dedup();
        for m in &methods {
            out.push_str(&format!("  +{}()\n", util::class_member(m)));
        }
        out.push_str("}\n");
    }

    let mut edges = analysis.type_edges.clone();
    edges.sort_by(|a, b| {
        a.from
            .cmp(&b.from)
            .then_with(|| a.to.cmp(&b.to))
            .then_with(|| type_edge_rank(&a.kind).cmp(&type_edge_rank(&b.kind)))
            .then_with(|| a.line.cmp(&b.line))
    });
    let mut seen: std::collections::BTreeSet<(String, String, u8)> =
        std::collections::BTreeSet::new();
    for e in &edges {
        if !declared.contains(e.from.as_str()) || !declared.contains(e.to.as_str()) {
            continue;
        }
        if !seen.insert((e.from.clone(), e.to.clone(), type_edge_rank(&e.kind))) {
            continue;
        }
        let from = util::sanitize("t", &e.from);
        let to = util::sanitize("t", &e.to);
        // `Inherit`: `to <|-- from` (from is the derived type); `Impl`:
        // `from ..|> to` (from realizes to); `Field`: `from *-- to`.
        let rel = match e.kind {
            TypeEdgeKind::Inherit => format!("{to} <|-- {from} : Inherit"),
            TypeEdgeKind::Impl => format!("{from} ..|> {to} : Impl"),
            TypeEdgeKind::Field => format!("{from} *-- {to} : Field"),
            TypeEdgeKind::Variant => format!("{from} --> {to} : Variant"),
        };
        out.push_str(&rel);
        out.push('\n');
    }
    out
}
