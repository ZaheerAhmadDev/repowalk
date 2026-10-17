//! Hand-rolled SVG emitter. OWNER: PROD-2.
//!
//! Pure and deterministic: no crate, no external assets, no I/O. Graphs are laid
//! out with plain arithmetic — a left-to-right longest-path layering over the
//! shared [`super::util::Graph`] — then drawn as rounded rects and `<text>`
//! labels with `<line>` + arrow-`<marker>` edges, using the app's dark palette.
//! Graphs above [`util::SVG_NODE_CAP`] degrade to the top-degree subgraph and
//! say so in a `<text>` caption.
use super::util::{self, Arrow, GEdge, GNode, Graph, Shape, Style};
use super::Diagram;
use crate::analysis::Analysis;
use crate::model::Index;
use std::collections::{BTreeMap, BTreeSet};

const NODE_W: f64 = 168.0;
const NODE_H: f64 = 34.0;
const H_GAP: f64 = 70.0;
const V_GAP: f64 = 22.0;
const MARGIN: f64 = 24.0;
const TITLE_H: f64 = 34.0;
const CAPTION_H: f64 = 20.0;

/// Render `diagram` as a standalone SVG document.
pub fn render(index: &Index, analysis: &Analysis, diagram: &Diagram) -> String {
    let (graph, title, cap) = match diagram {
        Diagram::CallGraph => (
            util::build_call_graph(index, usize::MAX),
            "Call graph".to_string(),
            util::SVG_NODE_CAP,
        ),
        Diagram::Cfg { symbol } => (
            util::build_cfg(analysis, symbol),
            format!("Control-flow graph — {symbol}"),
            util::SVG_NODE_CAP,
        ),
        Diagram::Types => (
            util::build_types(analysis),
            "Type graph".to_string(),
            util::SVG_NODE_CAP,
        ),
        Diagram::Modules => (
            util::build_modules(analysis),
            "Module structure".to_string(),
            util::SVG_NODE_CAP,
        ),
        Diagram::Directory { imports } => {
            let options = super::directory::DirectoryOptions {
                import_edges: *imports,
                loc_labels: true,
            };
            (
                super::directory::build(index, analysis, &options),
                "Directory tree".to_string(),
                // The degree-based legibility cap would drop tree leaves;
                // a containment tree is drawn whole or not at all.
                usize::MAX,
            )
        }
    };
    render_graph(&graph, &title, cap)
}

fn fmt(v: f64) -> String {
    format!("{v:.1}")
}

/// Longest-path layer index per node id (iterated to a fixed point; cycles are
/// bounded by `nodes.len()` relaxation passes).
fn depths(nodes: &[&GNode], edges: &[&GEdge]) -> BTreeMap<String, usize> {
    let mut d: BTreeMap<String, usize> = nodes.iter().map(|n| (n.id.clone(), 0)).collect();
    for _ in 0..nodes.len() {
        let mut changed = false;
        for e in edges {
            let (Some(&fd), Some(&td)) = (d.get(e.from.as_str()), d.get(e.to.as_str())) else {
                continue;
            };
            if td < fd + 1 {
                d.insert(e.to.clone(), fd + 1);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    d
}

fn render_graph(graph: &Graph, title: &str, cap: usize) -> String {
    let mut nodes: Vec<&GNode> = graph.nodes.iter().collect();
    nodes.sort_by(|a, b| a.id.cmp(&b.id));
    let mut edges: Vec<&GEdge> = graph.edges.iter().collect();
    let mut caption = graph.caption.clone();

    // Legibility cap: keep the highest-degree subgraph.
    if nodes.len() > cap {
        let mut degree: BTreeMap<&str, usize> = BTreeMap::new();
        for e in &edges {
            *degree.entry(e.from.as_str()).or_default() += 1;
            *degree.entry(e.to.as_str()).or_default() += 1;
        }
        let total = nodes.len();
        nodes.sort_by(|a, b| {
            degree[b.id.as_str()]
                .cmp(&degree[a.id.as_str()])
                .then_with(|| a.id.cmp(&b.id))
        });
        let keep: BTreeSet<&str> = nodes.iter().take(cap).map(|n| n.id.as_str()).collect();
        nodes.retain(|n| keep.contains(n.id.as_str()));
        edges.retain(|e| keep.contains(e.from.as_str()) && keep.contains(e.to.as_str()));
        caption = Some(format!(
            "showing top {} of {} nodes by degree (legibility cap)",
            cap, total
        ));
    }

    let depth = depths(&nodes, &edges);
    let mut cols: BTreeMap<usize, Vec<&str>> = BTreeMap::new();
    for n in &nodes {
        cols.entry(depth[&n.id]).or_default().push(&n.id);
    }
    let max_depth = cols.keys().copied().max().unwrap_or(0);
    let max_rows = cols.values().map(Vec::len).max().unwrap_or(0);

    let top = MARGIN + TITLE_H + if caption.is_some() { CAPTION_H } else { 0.0 };
    let mut pos: BTreeMap<&str, (f64, f64)> = BTreeMap::new();
    for (layer, ids) in &cols {
        for (row, id) in ids.iter().enumerate() {
            let x = MARGIN + *layer as f64 * (NODE_W + H_GAP) + NODE_W / 2.0;
            let y = top + row as f64 * (NODE_H + V_GAP) + NODE_H / 2.0;
            pos.insert(*id, (x, y));
        }
    }
    let width = MARGIN * 2.0 + max_depth as f64 * (NODE_W + H_GAP) + NODE_W;
    let height = if max_rows == 0 {
        top + MARGIN
    } else {
        top + max_rows as f64 * NODE_H + (max_rows - 1) as f64 * V_GAP + MARGIN
    };

    let mut out = String::new();
    out.push_str(&format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}\" height=\"{h}\" viewBox=\"0 0 {w} {h}\">\n",
        w = fmt(width),
        h = fmt(height)
    ));
    out.push_str(&defs());
    out.push_str(STYLE);
    out.push_str(&format!("<title>{}</title>\n", util::xml_escape(title)));
    out.push_str(&format!(
        "<rect class=\"bg\" x=\"0\" y=\"0\" width=\"{}\" height=\"{}\"/>\n",
        fmt(width),
        fmt(height)
    ));
    out.push_str(&format!(
        "<text class=\"title\" x=\"{}\" y=\"{}\">{}</text>\n",
        fmt(MARGIN),
        fmt(MARGIN + 16.0),
        util::xml_escape(title)
    ));
    if let Some(c) = &caption {
        out.push_str(&format!(
            "<text class=\"caption\" x=\"{}\" y=\"{}\">{}</text>\n",
            fmt(MARGIN),
            fmt(MARGIN + 16.0 + 16.0),
            util::xml_escape(c)
        ));
    }

    out.push_str("<g class=\"edges\">\n");
    for e in &edges {
        out.push_str(&edge_svg(e, &pos));
    }
    out.push_str("</g>\n");
    out.push_str("<g class=\"nodes\">\n");
    for n in &nodes {
        out.push_str(&node_svg(n, &pos));
    }
    out.push_str("</g>\n");
    out.push_str("</svg>\n");
    out
}

/// Arrow head marker per edge, `None` when the edge has no head (plain lines).
fn marker_id(arrow: Arrow) -> Option<&'static str> {
    match arrow {
        Arrow::Normal => Some("arrow-normal"),
        Arrow::Triangle => Some("arrow-hollow"),
        Arrow::Diamond => Some("arrow-diamond"),
        Arrow::Open => None,
    }
}

fn defs() -> String {
    let mut s = String::from("<defs>\n");
    for id in ["arrow-normal", "arrow-hollow"] {
        let class = if id == "arrow-hollow" {
            "arrowhead-hollow"
        } else {
            "arrowhead"
        };
        s.push_str(&format!(
            "<marker id=\"{id}\" viewBox=\"0 0 10 10\" refX=\"9\" refY=\"5\" markerWidth=\"7\" markerHeight=\"7\" orient=\"auto-start-reverse\"><path d=\"M 0 0 L 10 5 L 0 10 z\" class=\"{class}\"/></marker>\n"
        ));
    }
    s.push_str("<marker id=\"arrow-diamond\" viewBox=\"0 0 10 10\" refX=\"9\" refY=\"5\" markerWidth=\"8\" markerHeight=\"8\" orient=\"auto-start-reverse\"><path d=\"M 0 5 L 4 1 L 9 5 L 4 9 z\" class=\"arrowhead\"/></marker>\n");
    s.push_str("</defs>\n");
    s
}

const STYLE: &str = "<style>\n\
.bg { fill: #0F1115; }\n\
.node { fill: #14161B; stroke: #343B47; stroke-width: 1; }\n\
.label { fill: #E6E9EF; font: 12px ui-monospace, SFMono-Regular, Menlo, monospace; }\n\
.title { fill: #E6E9EF; font: bold 15px ui-sans-serif, system-ui, sans-serif; }\n\
.caption { fill: #8A93A3; font: italic 11px ui-sans-serif, system-ui, sans-serif; }\n\
.edge { stroke: #8A93A3; stroke-width: 1.2; fill: none; }\n\
.edge.dashed { stroke: #5A6270; stroke-dasharray: 5 4; }\n\
.edge-label { fill: #A8B0BD; font: 10px ui-sans-serif, system-ui, sans-serif; }\n\
.arrowhead { fill: #8A93A3; }\n\
.arrowhead-hollow { fill: #0F1115; stroke: #8A93A3; stroke-width: 1; }\n\
</style>\n";

fn edge_svg(e: &GEdge, pos: &BTreeMap<&str, (f64, f64)>) -> String {
    let (Some(&(sx, sy)), Some(&(tx, ty))) = (pos.get(e.from.as_str()), pos.get(e.to.as_str()))
    else {
        return String::new();
    };
    let half = NODE_W / 2.0;
    let (x1, x2) = if tx > sx {
        (sx + half, tx - half)
    } else if tx < sx {
        (sx - half, tx + half)
    } else {
        (sx, tx)
    };
    let back = tx <= sx;
    let class = if e.style == Style::Dashed || back {
        "edge dashed"
    } else {
        "edge"
    };
    let marker = match marker_id(e.arrow) {
        Some(id) => format!(" marker-end=\"url(#{id})\""),
        None => String::new(),
    };
    let mut out = format!(
        "<line class=\"{class}\" x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\"{}/>\n",
        fmt(x1),
        fmt(sy),
        fmt(x2),
        fmt(ty),
        marker
    );
    if !e.label.is_empty() {
        let mx = (x1 + x2) / 2.0;
        let my = (sy + ty) / 2.0 - 4.0;
        out.push_str(&format!(
            "<text class=\"edge-label\" x=\"{}\" y=\"{}\" text-anchor=\"middle\">{}</text>\n",
            fmt(mx),
            fmt(my),
            util::xml_escape(&e.label)
        ));
    }
    out
}

fn node_svg(n: &GNode, pos: &BTreeMap<&str, (f64, f64)>) -> String {
    let Some(&(cx, cy)) = pos.get(n.id.as_str()) else {
        return String::new();
    };
    let hw = NODE_W / 2.0;
    let hh = NODE_H / 2.0;
    let shape = match n.shape {
        Shape::Box | Shape::Rounded | Shape::Stadium | Shape::Oval => {
            let rx = match n.shape {
                Shape::Box => 4.0,
                Shape::Rounded => 8.0,
                _ => hh,
            };
            format!(
                "<rect class=\"node\" x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" rx=\"{}\"/>",
                fmt(cx - hw),
                fmt(cy - hh),
                fmt(NODE_W),
                fmt(NODE_H),
                fmt(rx)
            )
        }
        Shape::Diamond => {
            let dh = hh * 1.4;
            format!(
                "<polygon class=\"node\" points=\"{} {} {} {} {} {} {} {}\"/>",
                fmt(cx),
                fmt(cy - dh),
                fmt(cx + hw),
                fmt(cy),
                fmt(cx),
                fmt(cy + dh),
                fmt(cx - hw),
                fmt(cy)
            )
        }
        Shape::Hexagon => {
            let cut = 12.0;
            format!(
                "<polygon class=\"node\" points=\"{} {} {} {} {} {} {} {} {} {} {} {}\"/>",
                fmt(cx - hw + cut),
                fmt(cy - hh),
                fmt(cx + hw - cut),
                fmt(cy - hh),
                fmt(cx + hw),
                fmt(cy),
                fmt(cx + hw - cut),
                fmt(cy + hh),
                fmt(cx - hw + cut),
                fmt(cy + hh),
                fmt(cx - hw),
                fmt(cy)
            )
        }
    };
    format!(
        "{shape}\n<text class=\"label\" x=\"{}\" y=\"{}\" text-anchor=\"middle\">{}</text>\n",
        fmt(cx),
        fmt(cy + 4.0),
        util::xml_escape(&n.label)
    )
}
