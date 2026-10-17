//! Shared, deterministic graph model for the export emitters (PROD-2).
//!
//! The four emitters (Mermaid, DOT, SVG, report) all describe the same four
//! diagrams. Building one backend-agnostic [`Graph`] here — with all ordering,
//! deduping and capping decisions made once — is what guarantees the outputs
//! agree and stay byte-stable: no I/O, no clock, no environment, no hashing
//! seed. Emitters only translate the graph into their own syntax.
use crate::analysis::{
    Analysis, CfgEdgeKind, CfgKind, CfgNode, ModuleEdgeKind, TypeEdgeKind, TypeKind,
};
use crate::model::{Confidence, EdgeKind, Index};
use std::collections::{BTreeMap, BTreeSet};

/// Backend-neutral node shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    Box,
    Rounded,
    Stadium,
    Oval,
    Diamond,
    Hexagon,
}

/// Backend-neutral arrow head.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arrow {
    Normal,
    /// Hollow triangle: inheritance / realization.
    Triangle,
    /// Filled diamond: composition.
    Diamond,
    /// No head at all: plain containment line (directory tree edges).
    Open,
}

/// Line style.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Style {
    Solid,
    Dashed,
}

/// One graph node; `id` is already sanitized and unique.
#[derive(Debug, Clone)]
pub struct GNode {
    pub id: String,
    pub label: String,
    pub shape: Shape,
}

/// One directed edge; `from`/`to` are sanitized node ids.
#[derive(Debug, Clone)]
pub struct GEdge {
    pub from: String,
    pub to: String,
    pub label: String,
    pub arrow: Arrow,
    pub style: Style,
}

/// A renderable graph plus an optional caption (degradation/cap note).
#[derive(Debug, Clone, Default)]
pub struct Graph {
    pub nodes: Vec<GNode>,
    pub edges: Vec<GEdge>,
    pub caption: Option<String>,
}

/// Default call-graph node cap for Mermaid/DOT (the SVG backend uses its own).
pub const FLOW_NODE_CAP: usize = 150;
/// SVG legibility cap: beyond this only the top-degree subgraph is drawn.
pub const SVG_NODE_CAP: usize = 400;

// ---------------------------------------------------------------------------
// text / id helpers
// ---------------------------------------------------------------------------

/// FNV-1a 32-bit — deterministic (no `DefaultHasher` seed), used only to keep
/// sanitized ids unique when two raw ids collapse to the same character slug.
fn fnv1a(s: &str) -> u32 {
    let mut h: u32 = 0x811c_9dc5;
    for b in s.as_bytes() {
        h ^= *b as u32;
        h = h.wrapping_mul(0x0100_0193);
    }
    h
}

/// Sanitize `raw` into `[A-Za-z0-9_]`, prefixed with a letter so it can never
/// start with a digit or equal a Mermaid/DOT keyword, and suffixed with a hash
/// so distinct raws never collide after punctuation is folded to `_`.
pub fn sanitize(prefix: &str, raw: &str) -> String {
    let mut s = String::with_capacity(prefix.len() + raw.len() + 10);
    s.push_str(prefix);
    s.push('_');
    for ch in raw.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            s.push(ch);
        } else {
            s.push('_');
        }
    }
    s.push('_');
    s.push_str(&format!("{:08x}", fnv1a(raw)));
    s
}

/// Display name for a symbol id, falling back to the id when unknown.
pub fn symbol_name(index: &Index, id: &str) -> String {
    index
        .symbols
        .iter()
        .find(|s| s.id == id)
        .map(|s| s.name.clone())
        .unwrap_or_else(|| id.to_string())
}

/// Escape a label for a Mermaid quoted string.
pub fn mermaid_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("#quot;"),
            '\n' | '\r' | '\t' => out.push(' '),
            c => out.push(c),
        }
    }
    out
}

/// Escape a label for a DOT double-quoted string.
pub fn dot_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' | '\r' | '\t' => out.push(' '),
            c => out.push(c),
        }
    }
    out
}

/// Escape XML/SVG text content: `& < > " '`.
pub fn xml_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            c => out.push(c),
        }
    }
    out
}

/// Escape a member string for a Mermaid `classDiagram` body (`<`/`>` become
/// `~` so generics survive; newlines fold to spaces).
pub fn class_member(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '<' | '>' => '~',
            '\n' | '\r' | '\t' => ' ',
            c => c,
        })
        .collect()
}

fn conf_rank(c: Confidence) -> u8 {
    match c {
        Confidence::Exact => 0,
        Confidence::NameOnly => 1,
        Confidence::Ambiguous => 2,
        Confidence::Unresolved => 3,
    }
}

fn plural(n: usize) -> &'static str {
    if n == 1 {
        ""
    } else {
        "s"
    }
}

// ---------------------------------------------------------------------------
// diagram builders
// ---------------------------------------------------------------------------

/// Deduped `Call` edges: one entry per `(caller, callee)`, keeping the
/// strongest confidence seen. Sorted by `(caller, callee)`.
pub fn call_edges(index: &Index) -> Vec<(String, String, Confidence)> {
    let mut best: BTreeMap<(String, String), Confidence> = BTreeMap::new();
    for e in &index.edges {
        if e.kind != EdgeKind::Call {
            continue;
        }
        let key = (e.caller.clone(), e.callee.clone());
        match best.get(&key) {
            Some(c) if conf_rank(*c) <= conf_rank(e.confidence) => {}
            _ => {
                best.insert(key, e.confidence);
            }
        }
    }
    best.into_iter()
        .map(|((from, to), c)| (from, to, c))
        .collect()
}

/// Call graph over `Index::edges`, capped to the `cap` highest-degree nodes.
pub fn build_call_graph(index: &Index, cap: usize) -> Graph {
    let edges = call_edges(index);
    if edges.is_empty() {
        return Graph {
            nodes: vec![GNode {
                id: sanitize("n", "placeholder"),
                label: "(no calls)".to_string(),
                shape: Shape::Box,
            }],
            edges: Vec::new(),
            caption: Some("no call edges in index".to_string()),
        };
    }

    let mut degree: BTreeMap<&str, usize> = BTreeMap::new();
    for (from, to, _) in &edges {
        *degree.entry(from.as_str()).or_default() += 1;
        *degree.entry(to.as_str()).or_default() += 1;
    }
    let total = degree.len();
    let mut ranked: Vec<&str> = degree.keys().copied().collect();
    ranked.sort_by(|a, b| degree[b].cmp(&degree[a]).then_with(|| a.cmp(b)));

    let capped = total > cap;
    let keep: BTreeSet<&str> = ranked.iter().take(cap).copied().collect();

    // BTreeSet iterates sorted, so node declaration order is lexicographic.
    let nodes: Vec<GNode> = keep
        .iter()
        .map(|id| GNode {
            id: sanitize("n", id),
            label: symbol_name(index, id),
            shape: Shape::Rounded,
        })
        .collect();

    let edges: Vec<GEdge> = edges
        .iter()
        .filter(|(from, to, _)| keep.contains(from.as_str()) && keep.contains(to.as_str()))
        .map(|(from, to, conf)| GEdge {
            from: sanitize("n", from),
            to: sanitize("n", to),
            label: String::new(),
            arrow: Arrow::Normal,
            style: if *conf == Confidence::Exact {
                Style::Solid
            } else {
                Style::Dashed
            },
        })
        .collect();

    Graph {
        nodes,
        edges,
        caption: capped.then(|| format!("showing top {cap} of {total} nodes by degree")),
    }
}

fn cfg_node_label(n: &CfgNode) -> String {
    let base = if n.label.trim().is_empty() {
        match n.kind {
            CfgKind::Entry => "entry",
            CfgKind::Exit => "exit",
            CfgKind::Block => "block",
            CfgKind::Branch => "branch",
            CfgKind::Loop => "loop",
            CfgKind::Match => "match",
            CfgKind::Call => "call",
            CfgKind::Return => "return",
        }
        .to_string()
    } else {
        n.label.trim().to_string()
    };
    if n.start_line == n.end_line {
        format!("{base} (L{})", n.start_line)
    } else {
        format!("{base} (L{}-{})", n.start_line, n.end_line)
    }
}

fn shape_for_cfg(kind: CfgKind) -> Shape {
    match kind {
        CfgKind::Entry | CfgKind::Exit | CfgKind::Return => Shape::Stadium,
        CfgKind::Block => Shape::Box,
        CfgKind::Call => Shape::Rounded,
        CfgKind::Branch | CfgKind::Loop => Shape::Diamond,
        CfgKind::Match => Shape::Hexagon,
    }
}

fn cfg_edge_rank(kind: CfgEdgeKind) -> u8 {
    match kind {
        CfgEdgeKind::Normal => 0,
        CfgEdgeKind::True => 1,
        CfgEdgeKind::False => 2,
        CfgEdgeKind::Back => 3,
    }
}

/// Control-flow graph of one function, or a one-node placeholder when the
/// symbol has no CFG (never empty output).
pub fn build_cfg(analysis: &Analysis, symbol: &str) -> Graph {
    let Some(cfg) = analysis.cfgs.iter().find(|c| c.symbol == symbol) else {
        return Graph {
            nodes: vec![GNode {
                id: sanitize("n", "cfg-missing"),
                label: format!("no CFG for {symbol}"),
                shape: Shape::Box,
            }],
            edges: Vec::new(),
            caption: Some(format!("no CFG for symbol '{symbol}'")),
        };
    };

    let mut cnodes = cfg.nodes.clone();
    cnodes.sort_by_key(|n| n.id);
    let nodes: Vec<GNode> = cnodes
        .iter()
        .map(|n| GNode {
            id: format!("n{}", n.id),
            label: cfg_node_label(n),
            shape: shape_for_cfg(n.kind),
        })
        .collect();

    let mut cedges = cfg.edges.clone();
    cedges.sort_by(|a, b| {
        a.from
            .cmp(&b.from)
            .then_with(|| a.to.cmp(&b.to))
            .then_with(|| cfg_edge_rank(a.kind).cmp(&cfg_edge_rank(b.kind)))
            .then_with(|| a.line.cmp(&b.line))
    });
    let edges: Vec<GEdge> = cedges
        .iter()
        .map(|e| GEdge {
            from: format!("n{}", e.from),
            to: format!("n{}", e.to),
            label: match e.kind {
                CfgEdgeKind::Normal => String::new(),
                CfgEdgeKind::True => "true".to_string(),
                CfgEdgeKind::False => "false".to_string(),
                CfgEdgeKind::Back => "back".to_string(),
            },
            arrow: Arrow::Normal,
            style: if e.kind == CfgEdgeKind::Back {
                Style::Dashed
            } else {
                Style::Solid
            },
        })
        .collect();

    Graph {
        nodes,
        edges,
        caption: None,
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

/// Type / class diagram from `Analysis::types` + `Analysis::type_edges`.
pub fn build_types(analysis: &Analysis) -> Graph {
    if analysis.types.is_empty() {
        return Graph {
            nodes: vec![GNode {
                id: sanitize("t", "placeholder"),
                label: "(no types)".to_string(),
                shape: Shape::Box,
            }],
            edges: Vec::new(),
            caption: Some("no types declared".to_string()),
        };
    }

    let declared: BTreeSet<&str> = analysis.types.iter().map(|t| t.id.as_str()).collect();

    let mut types = analysis.types.clone();
    types.sort_by(|a, b| a.id.cmp(&b.id));
    let nodes: Vec<GNode> = types
        .iter()
        .map(|t| GNode {
            id: sanitize("t", &t.id),
            label: if t.name.is_empty() {
                t.id.clone()
            } else {
                t.name.clone()
            },
            // Traits/interfaces read as ellipses; concrete types as boxes.
            shape: match t.kind {
                TypeKind::Trait | TypeKind::Interface => Shape::Oval,
                _ => Shape::Box,
            },
        })
        .collect();

    let mut raw = analysis.type_edges.clone();
    raw.sort_by(|a, b| {
        a.from
            .cmp(&b.from)
            .then_with(|| a.to.cmp(&b.to))
            .then_with(|| type_edge_rank(&a.kind).cmp(&type_edge_rank(&b.kind)))
            .then_with(|| a.line.cmp(&b.line))
    });
    let mut seen: BTreeSet<(String, String, u8)> = BTreeSet::new();
    let mut edges: Vec<GEdge> = Vec::new();
    for e in &raw {
        if !declared.contains(e.from.as_str()) || !declared.contains(e.to.as_str()) {
            continue;
        }
        if !seen.insert((e.from.clone(), e.to.clone(), type_edge_rank(&e.kind))) {
            continue;
        }
        let (arrow, style, label) = match e.kind {
            TypeEdgeKind::Inherit => (Arrow::Triangle, Style::Solid, "inherit"),
            TypeEdgeKind::Impl => (Arrow::Triangle, Style::Dashed, "impl"),
            TypeEdgeKind::Field => (Arrow::Diamond, Style::Solid, "field"),
            TypeEdgeKind::Variant => (Arrow::Normal, Style::Solid, "variant"),
        };
        edges.push(GEdge {
            from: sanitize("t", &e.from),
            to: sanitize("t", &e.to),
            label: label.to_string(),
            arrow,
            style,
        });
    }

    Graph {
        nodes,
        edges,
        caption: None,
    }
}

fn module_edge_rank(kind: &ModuleEdgeKind) -> u8 {
    match kind {
        ModuleEdgeKind::Import => 0,
        ModuleEdgeKind::Submodule => 1,
    }
}

/// Crate / directory / module structure from `Analysis::modules` +
/// `Analysis::module_edges`, with import counts on the edge labels.
pub fn build_modules(analysis: &Analysis) -> Graph {
    if analysis.modules.is_empty() {
        return Graph {
            nodes: vec![GNode {
                id: sanitize("m", "placeholder"),
                label: "(no modules)".to_string(),
                shape: Shape::Box,
            }],
            edges: Vec::new(),
            caption: Some("no module graph available".to_string()),
        };
    }

    let declared: BTreeSet<&str> = analysis.modules.iter().map(|m| m.id.as_str()).collect();

    let mut mods = analysis.modules.clone();
    mods.sort_by(|a, b| a.id.cmp(&b.id));
    let nodes: Vec<GNode> = mods
        .iter()
        .map(|m| GNode {
            id: sanitize("m", &m.id),
            label: format!("{} ({} loc, {} sym)", m.id, m.loc, m.symbols),
            shape: Shape::Box,
        })
        .collect();

    let mut raw = analysis.module_edges.clone();
    raw.sort_by(|a, b| {
        a.from
            .cmp(&b.from)
            .then_with(|| a.to.cmp(&b.to))
            .then_with(|| module_edge_rank(&a.kind).cmp(&module_edge_rank(&b.kind)))
    });
    let mut seen: BTreeSet<(String, String, u8)> = BTreeSet::new();
    let mut edges: Vec<GEdge> = Vec::new();
    for e in &raw {
        if !declared.contains(e.from.as_str()) || !declared.contains(e.to.as_str()) {
            continue;
        }
        if !seen.insert((e.from.clone(), e.to.clone(), module_edge_rank(&e.kind))) {
            continue;
        }
        let label = match e.kind {
            ModuleEdgeKind::Import => format!("{} import{}", e.count, plural(e.count)),
            ModuleEdgeKind::Submodule => "submodule".to_string(),
        };
        edges.push(GEdge {
            from: sanitize("m", &e.from),
            to: sanitize("m", &e.to),
            label,
            arrow: Arrow::Normal,
            style: Style::Solid,
        });
    }

    Graph {
        nodes,
        edges,
        caption: None,
    }
}
