//! Graphviz DOT emitter. OWNER: PROD-2.
//!
//! Pure and deterministic. Renders the same four diagrams as the Mermaid
//! backend, from the shared [`super::util::Graph`]: sanitized node ids,
//! quoted+escaped labels, a per-diagram `rankdir`, and confidence/composition
//! styling carried as edge `style`/`arrowhead` attributes.
use super::util::{self, Arrow, Shape};
use super::Diagram;
use crate::analysis::Analysis;
use crate::model::Index;

/// Render `diagram` as DOT source (`digraph repowalk { … }`).
pub fn render(index: &Index, analysis: &Analysis, diagram: &Diagram) -> String {
    let (graph, rankdir) = match diagram {
        Diagram::CallGraph => (util::build_call_graph(index, util::FLOW_NODE_CAP), "LR"),
        Diagram::Cfg { symbol } => (util::build_cfg(analysis, symbol), "TB"),
        Diagram::Types => (util::build_types(analysis), "BT"),
        Diagram::Modules => (util::build_modules(analysis), "LR"),
        Diagram::Directory { imports } => {
            let options = super::directory::DirectoryOptions {
                import_edges: *imports,
                loc_labels: true,
            };
            (super::directory::build(index, analysis, &options), "TB")
        }
    };

    let mut out = String::from("digraph repowalk {\n");
    out.push_str(&format!("  rankdir={rankdir};\n"));
    if let Some(caption) = &graph.caption {
        out.push_str(&format!("  // {}\n", util::dot_escape(caption)));
    }
    out.push_str(
        "  node [shape=box, fillcolor=\"#14161B\", color=\"#343B47\", fontcolor=\"#E6E9EF\"];\n",
    );
    out.push_str("  edge [color=\"#8A93A3\", fontcolor=\"#E6E9EF\", arrowsize=0.7];\n");

    for n in &graph.nodes {
        out.push_str(&format!(
            "  {} [label=\"{}\", shape={}, style=\"{}\"];\n",
            n.id,
            util::dot_escape(&n.label),
            dot_shape(n.shape),
            dot_style(n.shape)
        ));
    }
    for e in &graph.edges {
        let mut attrs = format!("arrowhead={}", dot_arrow(e.arrow));
        if e.style == util::Style::Dashed {
            attrs.push_str(", style=dashed");
        }
        if !e.label.is_empty() {
            attrs.push_str(&format!(", label=\"{}\"", util::dot_escape(&e.label)));
        }
        out.push_str(&format!("  {} -> {} [{}];\n", e.from, e.to, attrs));
    }

    out.push_str("}\n");
    out
}

fn dot_shape(shape: Shape) -> &'static str {
    match shape {
        Shape::Box | Shape::Rounded | Shape::Stadium => "box",
        Shape::Oval => "ellipse",
        Shape::Diamond => "diamond",
        Shape::Hexagon => "hexagon",
    }
}

fn dot_style(shape: Shape) -> &'static str {
    match shape {
        Shape::Box | Shape::Rounded | Shape::Stadium => "rounded,filled",
        Shape::Oval | Shape::Diamond | Shape::Hexagon => "filled",
    }
}

fn dot_arrow(arrow: Arrow) -> &'static str {
    match arrow {
        Arrow::Normal => "normal",
        Arrow::Triangle => "onormal",
        Arrow::Diamond => "diamond",
        // Containment edges: a plain line, no head at all.
        Arrow::Open => "none",
    }
}
