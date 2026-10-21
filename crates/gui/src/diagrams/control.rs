//! Control-flow diagram view (CFG per function). OWNER: GRAPH-2.
//!
//! Contract to render (frozen in `crates/core/src/analysis.rs`): one
//! `FunctionCfg` per function with `CfgNode { kind, label, line }` and
//! `CfgEdge { kind, from, to }`; `CfgEdgeKind::Back` marks loop back-edges and
//! `cyclomatic`/`max_nesting` are already computed — never count decisions here.
//!
//! Painting follows the v1 flow renderer (`crate::flow::render`): a plain
//! `egui::Painter` canvas of themed lozenges and labelled arrows — no new
//! dependencies, and no per-frame clone of the analysis (we borrow
//! `app.analysis`). The picker orders functions exactly like
//! `complexity::hotspots` (cyclomatic desc, symbol asc) so it agrees with the
//! health panel.
use std::collections::{BTreeMap, HashMap, VecDeque};

use eframe::egui;
use repowalk_core::analysis::{Analysis, CfgEdge, CfgEdgeKind, CfgKind, CfgNode, FunctionCfg};
use repowalk_core::complexity::parse_symbol_id;
use repowalk_core::model::Index;

use crate::app::{App, Selection};
use crate::diagrams::interact;
use crate::theme;
use crate::ui_kit;

/// Render limits. The graph is a bounded census, not a full-graph render: over
/// this many nodes we draw only the entry-reachable subgraph and say so (the
/// same honesty rule the core emitters follow).
const MAX_NODES: usize = 300;

/// Canvas geometry (px). Fixed so a given `FunctionCfg` always lays out the
/// same way.
const NODE_MIN_W: f32 = 120.0;
const NODE_MAX_W: f32 = 260.0;
const NODE_H: f32 = 40.0;
const NODE_PAD_X: f32 = 8.0;
const H_GAP: f32 = 28.0;
const V_GAP: f32 = 48.0;
const SIDE_GAP: f32 = 24.0;
const PAD: f32 = 24.0;
/// Approximate monospace advance at `FONT_SMALL` — only used to size/truncate
/// node boxes deterministically without a font context (the canvas re-lays the
/// text out for real when it paints).
const CHAR_W: f32 = 6.6;

/// egui-memory key fragment for this canvas's interact layer. Full id is
/// `reverse("diagrams-cfg-canvas") + cfg.symbol`, so dragging one function's
/// CFG never bleeds into the next.
const STATE_ID: &str = "diagrams-cfg-canvas";

// ── Entry point ─────────────────────────────────────────────────────────────

pub fn ui(ui: &mut egui::Ui, app: &mut App) {
    if app.analysis.is_none() {
        ui_kit::empty_state(ui, "Control-flow diagrams", ui_kit::NO_ANALYSIS_HINT);
        return;
    }

    // Filter text lives in egui temp memory: this panel owns no field on
    // `DiagramsState` (integrator-owned) and never touches `mod.rs`.
    let filter_id = egui::Id::new("diagrams-cfg-filter");
    let mut filter: String = ui
        .ctx()
        .data_mut(|d| d.get_temp_mut_or_default::<String>(filter_id).clone());

    let mut clicked: Option<String> = None;
    {
        let analysis = app.analysis.as_ref().expect("analysis checked above");
        let index = app.index.as_deref();
        let selection = &app.selection;
        let selected = match selection {
            Some(Selection::Symbol(id)) => Some(id.as_str()),
            _ => None,
        };
        let cfg = selected.and_then(|id| analysis.cfgs.iter().find(|c| c.symbol == id));
        let avail_h = ui.available_height();

        ui.horizontal_top(|ui| {
            let picker_w = 288.0_f32.min(ui.available_width() * 0.42);
            ui.allocate_ui_with_layout(
                egui::vec2(picker_w, avail_h),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    clicked = picker(ui, analysis, index, &mut filter, selected);
                },
            );
            ui.separator();
            ui.allocate_ui_with_layout(
                egui::vec2(ui.available_width(), avail_h),
                egui::Layout::top_down(egui::Align::Min),
                |ui| match cfg {
                    Some(c) => canvas(ui, c, index),
                    None => missing_cfg(ui, analysis, selection, index),
                },
            );
        });
    }

    ui.ctx()
        .data_mut(|d| *d.get_temp_mut_or_default::<String>(filter_id) = filter);
    if let Some(id) = clicked {
        app.select(Selection::Symbol(id));
    }
}

// ── Function picker ─────────────────────────────────────────────────────────

/// Left rail: a filter box plus the function list, most-complex first.
/// Returns the symbol id that was clicked, if any.
fn picker(
    ui: &mut egui::Ui,
    analysis: &Analysis,
    index: Option<&Index>,
    filter: &mut String,
    selected: Option<&str>,
) -> Option<String> {
    theme::section_header(ui, "functions");
    ui.add(
        egui::TextEdit::singleline(filter)
            .hint_text("filter by name…")
            .desired_width(f32::INFINITY),
    );
    ui.add_space(theme::SPACE_1);

    // id → name lookup so the picker is linear, not O(functions × symbols).
    // This is the *index* (small, separate from the CFG graph we borrow).
    let names: HashMap<&str, &str> = index
        .map(|idx| {
            idx.symbols
                .iter()
                .map(|s| (s.id.as_str(), s.name.as_str()))
                .collect()
        })
        .unwrap_or_default();

    let needle = filter.trim().to_lowercase();
    let mut order: Vec<usize> = Vec::with_capacity(analysis.cfgs.len());
    for (i, cfg) in analysis.cfgs.iter().enumerate() {
        let name = names
            .get(cfg.symbol.as_str())
            .copied()
            .unwrap_or(cfg.symbol.as_str());
        if needle.is_empty()
            || name.to_lowercase().contains(&needle)
            || cfg.symbol.to_lowercase().contains(&needle)
        {
            order.push(i);
        }
    }
    // Same ordering as `complexity::hotspots`: cyclomatic desc, symbol asc.
    order.sort_by(|&a, &b| {
        let (ca, cb) = (&analysis.cfgs[a], &analysis.cfgs[b]);
        cb.cyclomatic
            .cmp(&ca.cyclomatic)
            .then_with(|| ca.symbol.cmp(&cb.symbol))
    });

    if order.is_empty() {
        ui.add_space(theme::SPACE_3);
        ui.label(
            egui::RichText::new(if analysis.cfgs.is_empty() {
                "analysis has no function CFGs yet"
            } else {
                "no function matches this filter"
            })
            .size(theme::FONT_SMALL)
            .color(theme::TEXT_MUTED),
        );
        return None;
    }

    let mut clicked = None;
    egui::ScrollArea::vertical()
        .id_salt("diagrams-cfg-picker")
        .auto_shrink([false, false])
        .show_rows(ui, theme::ROW_HEIGHT, order.len(), |ui, range| {
            for i in range {
                let cfg = &analysis.cfgs[order[i]];
                let name = names
                    .get(cfg.symbol.as_str())
                    .copied()
                    .unwrap_or(cfg.symbol.as_str());
                let is_sel = selected == Some(cfg.symbol.as_str());
                let row = theme::list_row(ui, is_sel, ("cfg-row", cfg.symbol.as_str()), |ui| {
                    let color = if is_sel {
                        theme::ACCENT
                    } else {
                        theme::TEXT_PRIMARY
                    };
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(name)
                                .size(theme::FONT_BODY)
                                .color(color),
                        )
                        .truncate(),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            egui::RichText::new(cfg.cyclomatic.to_string())
                                .size(theme::FONT_SMALL)
                                .strong()
                                .color(if cfg.cyclomatic > 10 {
                                    theme::WARNING
                                } else {
                                    theme::TEXT_MUTED
                                }),
                        );
                    });
                });
                let row = row.on_hover_text(format!(
                    "{}\n{name}\ncyclomatic {} · nesting {}",
                    cfg.symbol, cfg.cyclomatic, cfg.max_nesting
                ));
                if row.clicked() {
                    clicked = Some(cfg.symbol.clone());
                }
            }
        });
    clicked
}

// ── Canvas ──────────────────────────────────────────────────────────────────

fn canvas(ui: &mut egui::Ui, cfg: &FunctionCfg, index: Option<&Index>) {
    let state_id = interact::canvas_key(STATE_ID, index).with(cfg.symbol.as_str());
    let mut reset_requested = false;
    badge_strip(
        ui,
        cfg,
        &resolve_name(index, &cfg.symbol),
        &mut reset_requested,
    );

    let view = View::build(cfg);
    if view.capped {
        ui.add_space(theme::SPACE_1);
        ui.label(
            egui::RichText::new(format!(
                "capped at {MAX_NODES} nodes — showing the entry-reachable subgraph of {} \
                 ({} node(s) omitted)",
                view.total_nodes,
                view.total_nodes - view.nodes.len()
            ))
            .size(theme::FONT_SMALL)
            .color(theme::WARNING),
        );
    }
    ui.add_space(theme::SPACE_2);

    if view.nodes.is_empty() {
        ui.label(
            egui::RichText::new("this function's control-flow graph has no nodes")
                .size(theme::FONT_SMALL)
                .color(theme::TEXT_MUTED),
        );
        return;
    }

    let layout = layout(&view.nodes, &view.edges);

    // No ScrollArea here (B5-02): pan — dragging empty canvas — replaced
    // scrolling inside the canvas, so the plain wheel is free to zoom about the
    // cursor with no painter-coordinates-vs-scroll-rows fight. The painter's
    // clip keeps off-canvas content from bleeding over the rest of the panel.
    let (resp, painter) = ui.allocate_painter(ui.available_size(), egui::Sense::click_and_drag());
    let mut state = interact::CanvasState::load(ui.ctx(), state_id);
    if reset_requested {
        state.reset();
    }
    let origin = resp.rect.min.to_vec2();
    painter.rect_filled(
        resp.rect,
        egui::CornerRadius::same(theme::CORNER_MD),
        theme::SURFACE_E0,
    );

    // Base layout items for the interact layer: stable node ids paired with
    // their canonical rects. User drags/pan/zoom apply on top of `layout`.
    let id_key = |i: usize| format!("n{}", view.nodes[i].id);
    let items: Vec<(String, egui::Rect)> = view
        .nodes
        .iter()
        .enumerate()
        .map(|(i, _)| (id_key(i), layout.nodes[i].rect))
        .collect();
    let (hovered_id, actions) = interact::frame(&resp, &mut state, &items);
    if actions.double_clicked_empty {
        state.reset();
    }
    // Screen rects (offset + layer transform + canvas origin), reused by the
    // hit-test above, the painting below, and the edge elbows.
    let placed: Vec<egui::Rect> = view
        .nodes
        .iter()
        .enumerate()
        .map(|(i, _)| {
            interact::placed_rect(
                layout.nodes[i].rect,
                state.node_offset(&id_key(i)),
                state.layer,
            )
            .translate(origin)
        })
        .collect();

    paint_edges(&painter, &view, &layout, &placed, state.layer.scale);
    paint_nodes(&painter, &view, &placed, state.layer.scale);

    // Node labels are truncated to fit; reveal the real text on hover (unless
    // that pointer is busy dragging the node).
    let hovered = hovered_id.as_deref().and_then(|id| {
        id.strip_prefix('n')
            .and_then(|n| n.parse::<u32>().ok())
            .and_then(|nid| view.by_id.get(&nid).copied())
    });
    if let Some(i) = hovered.filter(|_| !state.is_dragging()) {
        let nd = view.nodes[i];
        let label = if nd.label.is_empty() {
            "(no label)"
        } else {
            nd.label.as_str()
        };
        let _ = resp.on_hover_text(format!(
            "{} · {:?} · L{}..{}\n{label}",
            kind_tag(nd.kind),
            nd.kind,
            nd.start_line,
            nd.end_line
        ));
    }
    state.store(ui.ctx(), state_id);
}

/// The cyclomatic / nesting badge strip above the canvas (+ the Reset action
/// for the interact layer, which writes back through `reset`).
fn badge_strip(ui: &mut egui::Ui, cfg: &FunctionCfg, name: &str, reset: &mut bool) {
    ui.horizontal(|ui| {
        ui.add(
            egui::Label::new(
                egui::RichText::new(name)
                    .size(theme::FONT_BODY)
                    .strong()
                    .color(theme::TEXT_PRIMARY),
            )
            .truncate(),
        );
        let (file, line) = parse_symbol_id(&cfg.symbol);
        ui.label(
            egui::RichText::new(format!("{file}:{line}"))
                .size(theme::FONT_SMALL)
                .color(theme::TEXT_MUTED),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .button("Reset")
                .on_hover_text(
                    "put every node back on the deterministic layout \
                     (double-click on empty canvas does the same)",
                )
                .clicked()
            {
                *reset = true;
            }
            badge(
                ui,
                "nesting",
                cfg.max_nesting,
                if cfg.max_nesting > 4 {
                    theme::WARNING
                } else {
                    theme::INFO
                },
            );
            badge(
                ui,
                "cyclomatic",
                cfg.cyclomatic,
                if cfg.cyclomatic > 10 {
                    theme::WARNING
                } else {
                    theme::ACCENT
                },
            );
            ui.label(
                egui::RichText::new(format!(
                    "{} nodes · {} edges",
                    cfg.nodes.len(),
                    cfg.edges.len()
                ))
                .size(theme::FONT_SMALL)
                .color(theme::TEXT_MUTED),
            );
        });
    });
}

fn badge(ui: &mut egui::Ui, label: &str, value: u32, color: egui::Color32) {
    egui::Frame::new()
        .fill(theme::SURFACE_E2)
        .stroke(egui::Stroke::new(1.0, theme::BORDER_SUBTLE))
        .corner_radius(egui::CornerRadius::same(theme::CORNER_SM))
        .inner_margin(egui::Margin::symmetric(theme::SPACE_2 as i8, 3))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = theme::SPACE_1;
                ui.label(
                    egui::RichText::new(label)
                        .size(theme::FONT_SMALL)
                        .color(theme::TEXT_MUTED),
                );
                ui.label(
                    egui::RichText::new(value.to_string())
                        .size(theme::FONT_SMALL)
                        .strong()
                        .color(color),
                );
            });
        });
}

/// Honest "nothing to draw here" states — a selection that has no CFG says so
/// rather than drawing an empty frame.
fn missing_cfg(
    ui: &mut egui::Ui,
    analysis: &Analysis,
    selection: &Option<Selection>,
    index: Option<&Index>,
) {
    let (title, detail) = match selection {
        Some(Selection::Symbol(id)) => (
            "no control-flow graph for this function".to_string(),
            format!(
                "{} · {id}\nthe extractor does not emit a CFG for every symbol; \
                 {} function CFG(s) are available",
                resolve_name(index, id),
                analysis.cfgs.len()
            ),
        ),
        Some(Selection::File(f)) => (
            "a file is selected, not a function".to_string(),
            format!("{f} — pick a function on the left to render its control flow"),
        ),
        None => (
            "no function selected".to_string(),
            format!(
                "pick one of the {} function CFG(s) on the left to render its control flow",
                analysis.cfgs.len()
            ),
        ),
    };
    ui.vertical_centered(|ui| {
        ui.add_space(theme::SPACE_8);
        ui.label(
            egui::RichText::new(title)
                .size(theme::FONT_HEADING)
                .color(theme::TEXT_SECONDARY),
        );
        ui.add_space(theme::SPACE_2);
        ui.label(
            egui::RichText::new(detail)
                .size(theme::FONT_SMALL)
                .color(theme::TEXT_MUTED),
        );
    });
}

// ── Bounded view ────────────────────────────────────────────────────────────

/// A capped, id-indexed projection of one `FunctionCfg`. Over `MAX_NODES`, only
/// the entry-reachable nodes (BFS order) are kept; every kept node's edges are
/// kept and everything else is dropped — and `capped` says so out loud.
struct View<'a> {
    nodes: Vec<&'a CfgNode>,
    edges: Vec<&'a CfgEdge>,
    /// node id → position in `nodes` / `layout.nodes`.
    by_id: BTreeMap<u32, usize>,
    capped: bool,
    total_nodes: usize,
}

impl<'a> View<'a> {
    fn build(cfg: &'a FunctionCfg) -> Self {
        let total_nodes = cfg.nodes.len();
        let mut by_id_full: BTreeMap<u32, usize> = BTreeMap::new();
        for (i, nd) in cfg.nodes.iter().enumerate() {
            by_id_full.insert(nd.id, i);
        }
        let mut adj: Vec<Vec<usize>> = vec![Vec::new(); total_nodes];
        for e in &cfg.edges {
            if let (Some(&a), Some(&b)) = (by_id_full.get(&e.from), by_id_full.get(&e.to)) {
                adj[a].push(b);
            }
        }
        for a in &mut adj {
            a.sort_unstable();
            a.dedup();
        }

        let entry = cfg
            .nodes
            .iter()
            .position(|nd| nd.kind == CfgKind::Entry)
            .or_else(|| {
                cfg.nodes
                    .iter()
                    .enumerate()
                    .min_by_key(|(_, nd)| nd.id)
                    .map(|(i, _)| i)
            });

        let mut order: Vec<usize> = Vec::with_capacity(total_nodes);
        let mut seen = vec![false; total_nodes];
        if let Some(s) = entry {
            let mut q = VecDeque::new();
            seen[s] = true;
            order.push(s);
            q.push_back(s);
            while let Some(u) = q.pop_front() {
                for &v in &adj[u] {
                    if !seen[v] {
                        seen[v] = true;
                        order.push(v);
                        q.push_back(v);
                    }
                }
            }
        }
        // Anything the walk did not reach (disconnected blocks) in id order, so
        // the view is still deterministic for a given graph.
        if order.len() < total_nodes {
            let mut rest: Vec<usize> = (0..total_nodes).filter(|&i| !seen[i]).collect();
            rest.sort_by_key(|&i| cfg.nodes[i].id);
            order.extend(rest);
        }

        let capped = order.len() > MAX_NODES;
        order.truncate(MAX_NODES);
        let nodes: Vec<&CfgNode> = order.iter().map(|&i| &cfg.nodes[i]).collect();
        let mut by_id = BTreeMap::new();
        for (idx, &i) in order.iter().enumerate() {
            by_id.insert(cfg.nodes[i].id, idx);
        }
        let edges: Vec<&CfgEdge> = cfg
            .edges
            .iter()
            .filter(|e| by_id.contains_key(&e.from) && by_id.contains_key(&e.to))
            .collect();
        View {
            nodes,
            edges,
            by_id,
            capped,
            total_nodes,
        }
    }
}

// ── Layout (pure + deterministic) ───────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq)]
struct NodePos {
    rect: egui::Rect,
    depth: usize,
}

#[derive(Clone, Debug, PartialEq)]
struct Layout {
    /// Parallel to the `nodes` slice handed to [`layout`].
    nodes: Vec<NodePos>,
    width: f32,
    height: f32,
}

/// Layered top-down layout by BFS depth from entry. Deterministic: nodes are
/// grouped by shortest depth and ordered by node id within each row, so the
/// same graph always produces the same canvas regardless of input order.
fn layout(nodes: &[&CfgNode], edges: &[&CfgEdge]) -> Layout {
    let n = nodes.len();
    if n == 0 {
        return Layout {
            nodes: Vec::new(),
            width: PAD * 2.0,
            height: PAD * 2.0,
        };
    }

    let mut by_id: BTreeMap<u32, usize> = BTreeMap::new();
    for (i, nd) in nodes.iter().enumerate() {
        by_id.insert(nd.id, i);
    }
    let mut adj: Vec<Vec<usize>> = vec![Vec::new(); n];
    for e in edges {
        if let (Some(&a), Some(&b)) = (by_id.get(&e.from), by_id.get(&e.to)) {
            adj[a].push(b);
        }
    }
    for a in &mut adj {
        a.sort_unstable();
        a.dedup();
    }

    let entry = nodes
        .iter()
        .position(|nd| nd.kind == CfgKind::Entry)
        .or_else(|| {
            nodes
                .iter()
                .enumerate()
                .min_by_key(|(_, nd)| nd.id)
                .map(|(i, _)| i)
        });

    let mut depth: Vec<Option<usize>> = vec![None; n];
    if let Some(s) = entry {
        let mut q = VecDeque::new();
        depth[s] = Some(0);
        q.push_back(s);
        while let Some(u) = q.pop_front() {
            let Some(du) = depth[u] else { continue };
            for &v in &adj[u] {
                if depth[v].is_none() {
                    depth[v] = Some(du + 1);
                    q.push_back(v);
                }
            }
        }
    }
    // Unreached nodes (e.g. no Entry) fall into one trailing row.
    let next_row = depth.iter().flatten().copied().max().map_or(0, |d| d + 1);
    for slot in &mut depth {
        if slot.is_none() {
            *slot = Some(next_row);
        }
    }
    let rows = depth.iter().flatten().copied().max().map_or(1, |d| d + 1);

    let mut groups: Vec<Vec<usize>> = vec![Vec::new(); rows];
    for (i, d) in depth.iter().enumerate() {
        if let Some(d) = *d {
            groups[d].push(i);
        }
    }
    for g in &mut groups {
        g.sort_by_key(|&i| nodes[i].id);
    }

    let widths: Vec<f32> = nodes.iter().map(|nd| node_width(&nd.label)).collect();
    let row_w: Vec<f32> = groups
        .iter()
        .map(|g| {
            if g.is_empty() {
                0.0
            } else {
                g.iter().map(|&i| widths[i]).sum::<f32>() + H_GAP * (g.len() as f32 - 1.0)
            }
        })
        .collect();
    let max_w = row_w.iter().copied().fold(NODE_MIN_W, f32::max);

    let mut out = vec![
        NodePos {
            rect: egui::Rect::NOTHING,
            depth: 0,
        };
        n
    ];
    for (d, g) in groups.iter().enumerate() {
        let y = PAD + d as f32 * (NODE_H + V_GAP);
        let mut x = PAD + (max_w - row_w[d]) * 0.5;
        for &i in g {
            out[i] = NodePos {
                rect: egui::Rect::from_min_size(egui::pos2(x, y), egui::vec2(widths[i], NODE_H)),
                depth: d,
            };
            x += widths[i] + H_GAP;
        }
    }

    Layout {
        nodes: out,
        width: max_w + PAD * 2.0,
        height: PAD * 2.0 + rows as f32 * NODE_H + (rows.saturating_sub(1)) as f32 * V_GAP,
    }
}

fn node_width(label: &str) -> f32 {
    let chars = label.chars().count() as f32;
    (chars * CHAR_W + NODE_PAD_X * 2.0 + 8.0).clamp(NODE_MIN_W, NODE_MAX_W)
}

// ── Painting ────────────────────────────────────────────────────────────────

fn paint_nodes(painter: &egui::Painter, view: &View, placed: &[egui::Rect], scale: f32) {
    let font = egui::FontId::new(theme::FONT_SMALL * scale, egui::FontFamily::Monospace);
    for (i, nd) in view.nodes.iter().enumerate() {
        paint_node(painter, placed[i], nd, &font);
    }
}

fn paint_node(painter: &egui::Painter, rect: egui::Rect, nd: &CfgNode, font: &egui::FontId) {
    let style = node_style(nd.kind);
    match nd.kind {
        CfgKind::Branch | CfgKind::Loop | CfgKind::Match => {
            painter.add(egui::Shape::convex_polygon(
                chamfer(rect, 10.0),
                style.fill,
                egui::Stroke::new(1.2, style.border),
            ));
        }
        CfgKind::Entry | CfgKind::Exit => {
            let cr = egui::CornerRadius::same(rect.height().min(22.0) as u8);
            painter.rect_filled(rect, cr, style.fill);
            painter.rect_stroke(
                rect,
                cr,
                egui::Stroke::new(1.4, style.border),
                egui::StrokeKind::Middle,
            );
        }
        _ => {
            let cr = egui::CornerRadius::same(theme::CORNER_MD);
            painter.rect_filled(rect, cr, style.fill);
            painter.rect_stroke(
                rect,
                cr,
                egui::Stroke::new(1.2, style.border),
                egui::StrokeKind::Middle,
            );
        }
    }

    let inner = rect.shrink2(egui::vec2(NODE_PAD_X, 5.0));
    let tag = format!("{}  L{}", kind_tag(nd.kind), nd.start_line);
    let galley_tag = painter.layout_no_wrap(tag, font.clone(), style.tag);
    let tag_h = galley_tag.size().y;
    painter.galley(egui::pos2(inner.left(), inner.top()), galley_tag, style.tag);

    let max_chars = ((inner.width() / CHAR_W).floor() as usize).max(4);
    let label = truncate_chars(&nd.label, max_chars);
    let galley_label = painter.layout_no_wrap(label, font.clone(), theme::TEXT_PRIMARY);
    painter.galley(
        egui::pos2(inner.left(), inner.top() + tag_h + 1.0),
        galley_label,
        theme::TEXT_PRIMARY,
    );
}

fn paint_edges(
    painter: &egui::Painter,
    view: &View,
    layout: &Layout,
    placed: &[egui::Rect],
    scale: f32,
) {
    let font = egui::FontId::new(theme::FONT_SMALL * scale, egui::FontFamily::Monospace);
    let mut lane = 0usize;
    for e in &view.edges {
        let (Some(&a), Some(&b)) = (view.by_id.get(&e.from), view.by_id.get(&e.to)) else {
            continue;
        };
        let ra = placed[a];
        let rb = placed[b];
        let back = e.kind == CfgEdgeKind::Back;
        let (color, dashed) = match e.kind {
            CfgEdgeKind::Back => (theme::WARNING, true),
            CfgEdgeKind::False => (theme::with_alpha(theme::TEXT_SECONDARY, 170), false),
            _ => (theme::with_alpha(theme::TEXT_SECONDARY, 195), false),
        };
        let stroke = egui::Stroke::new(if back { 1.6 } else { 1.3 }, color);

        let pts: Vec<egui::Pos2>;
        let label_at: egui::Pos2;
        if !back && layout.nodes[b].depth > layout.nodes[a].depth {
            // Forward: elbow through the empty gap just above the target row.
            let sx = ra.center().x;
            let tx = rb.center().x;
            let midy = rb.top() - V_GAP * 0.5 * scale;
            pts = vec![
                egui::pos2(sx, ra.bottom()),
                egui::pos2(sx, midy),
                egui::pos2(tx, midy),
                egui::pos2(tx, rb.top()),
            ];
            label_at = egui::pos2((sx + tx) * 0.5, midy - 2.0);
        } else {
            // Side lane: leaves and re-enters on the left, so a loop-closing
            // back edge visibly doubles back.
            lane += 1;
            let lane_x = ra.left().min(rb.left()) - SIDE_GAP * lane as f32 * scale;
            let sy = ra.center().y;
            let ty = rb.center().y;
            pts = vec![
                egui::pos2(ra.left(), sy),
                egui::pos2(lane_x, sy),
                egui::pos2(lane_x, ty),
                egui::pos2(rb.left(), ty),
            ];
            label_at = egui::pos2(lane_x, (sy + ty) * 0.5);
        }

        if dashed {
            for s in egui::Shape::dashed_line(&pts, stroke, 6.0, 4.0) {
                painter.add(s);
            }
        } else {
            painter.add(egui::Shape::line(pts.clone(), stroke));
        }
        if pts.len() >= 2 {
            arrow_head(painter, pts[pts.len() - 1], pts[pts.len() - 2], color);
        }

        if let Some(text) = edge_label(e.kind) {
            let galley = painter.layout_no_wrap(text.to_string(), font.clone(), color);
            let pos = label_at - egui::vec2(galley.size().x * 0.5, galley.size().y + 1.0);
            let bg = egui::Rect::from_min_size(
                pos - egui::vec2(2.0, 1.0),
                galley.size() + egui::vec2(4.0, 2.0),
            );
            painter.rect_filled(bg, egui::CornerRadius::ZERO, theme::SURFACE_E0);
            painter.galley(pos, galley, color);
        }
    }
}

fn arrow_head(painter: &egui::Painter, tip: egui::Pos2, from: egui::Pos2, color: egui::Color32) {
    let d = tip - from;
    if d.length_sq() < 1e-6 {
        return;
    }
    let dir = d.normalized();
    let n = egui::vec2(-dir.y, dir.x);
    let base = tip - dir * 8.0;
    painter.add(egui::Shape::convex_polygon(
        vec![tip, base + n * 4.0, base - n * 4.0],
        color,
        egui::Stroke::NONE,
    ));
}

/// A rectangle with its corners cut — a lozenge that still holds text. Reads as
/// "diamond-ish" for decision nodes without giving up legibility.
fn chamfer(rect: egui::Rect, c: f32) -> Vec<egui::Pos2> {
    let c = c.min(rect.width() * 0.5).min(rect.height() * 0.5);
    vec![
        egui::pos2(rect.left() + c, rect.top()),
        egui::pos2(rect.right() - c, rect.top()),
        egui::pos2(rect.right(), rect.top() + c),
        egui::pos2(rect.right(), rect.bottom() - c),
        egui::pos2(rect.right() - c, rect.bottom()),
        egui::pos2(rect.left() + c, rect.bottom()),
        egui::pos2(rect.left(), rect.bottom() - c),
        egui::pos2(rect.left(), rect.top() + c),
    ]
}

// ── Small helpers ───────────────────────────────────────────────────────────

struct NodeStyle {
    fill: egui::Color32,
    border: egui::Color32,
    tag: egui::Color32,
}

fn node_style(kind: CfgKind) -> NodeStyle {
    match kind {
        CfgKind::Entry => NodeStyle {
            fill: theme::with_alpha(theme::SUCCESS, 34),
            border: theme::SUCCESS,
            tag: theme::SUCCESS,
        },
        CfgKind::Exit => NodeStyle {
            fill: theme::with_alpha(theme::DANGER, 34),
            border: theme::DANGER,
            tag: theme::DANGER,
        },
        CfgKind::Branch => NodeStyle {
            fill: theme::with_alpha(theme::ACCENT, 30),
            border: theme::ACCENT,
            tag: theme::ACCENT,
        },
        CfgKind::Loop => NodeStyle {
            fill: theme::with_alpha(theme::INFO, 30),
            border: theme::INFO,
            tag: theme::INFO,
        },
        CfgKind::Match => NodeStyle {
            fill: theme::with_alpha(theme::WARNING, 30),
            border: theme::WARNING,
            tag: theme::WARNING,
        },
        CfgKind::Return => NodeStyle {
            fill: theme::with_alpha(theme::INFO, 16),
            border: theme::BORDER_STRONG,
            tag: theme::INFO,
        },
        CfgKind::Call => NodeStyle {
            fill: theme::SURFACE_E1,
            border: theme::BORDER_SUBTLE,
            tag: theme::TEXT_MUTED,
        },
        CfgKind::Block => NodeStyle {
            fill: theme::SURFACE_E2,
            border: theme::BORDER_STRONG,
            tag: theme::TEXT_MUTED,
        },
    }
}

fn kind_tag(kind: CfgKind) -> &'static str {
    match kind {
        CfgKind::Entry => "▶ ENTRY",
        CfgKind::Exit => "■ EXIT",
        CfgKind::Block => "BLOCK",
        CfgKind::Branch => "BRANCH",
        CfgKind::Loop => "LOOP",
        CfgKind::Match => "MATCH",
        CfgKind::Call => "CALL",
        CfgKind::Return => "RETURN",
    }
}

fn edge_label(kind: CfgEdgeKind) -> Option<&'static str> {
    match kind {
        CfgEdgeKind::Normal => None,
        CfgEdgeKind::True => Some("true"),
        CfgEdgeKind::False => Some("false"),
        CfgEdgeKind::Back => Some("back"),
    }
}

fn resolve_name(index: Option<&Index>, id: &str) -> String {
    index
        .and_then(|idx| idx.symbols.iter().find(|s| s.id == id))
        .map(|s| s.name.clone())
        .unwrap_or_else(|| id.to_string())
}

fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let take = max.saturating_sub(1);
    let mut out: String = s.chars().take(take).collect();
    out.push('…');
    out
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn node(id: u32, kind: CfgKind, label: &str) -> CfgNode {
        CfgNode {
            id,
            kind,
            start_line: id as usize + 1,
            end_line: id as usize + 2,
            label: label.to_string(),
        }
    }

    fn edge(from: u32, to: u32, kind: CfgEdgeKind) -> CfgEdge {
        CfgEdge {
            from,
            to,
            kind,
            line: from as usize + 1,
        }
    }

    fn cfg_of(symbol: &str, nodes: Vec<CfgNode>, edges: Vec<CfgEdge>) -> FunctionCfg {
        FunctionCfg {
            symbol: symbol.into(),
            nodes,
            edges,
            cyclomatic: 1,
            max_nesting: 0,
        }
    }

    #[test]
    fn layout_is_deterministic_and_depth_layered() {
        let cfg = cfg_of(
            "src/a.rs:1",
            vec![
                node(0, CfgKind::Entry, "entry"),
                node(1, CfgKind::Branch, "if x"),
                node(2, CfgKind::Block, "then"),
                node(3, CfgKind::Block, "else"),
                node(4, CfgKind::Exit, "exit"),
            ],
            vec![
                edge(0, 1, CfgEdgeKind::Normal),
                edge(1, 2, CfgEdgeKind::True),
                edge(1, 3, CfgEdgeKind::False),
                edge(2, 4, CfgEdgeKind::Normal),
                edge(3, 4, CfgEdgeKind::Normal),
            ],
        );
        let nodes: Vec<&CfgNode> = cfg.nodes.iter().collect();
        let edges: Vec<&CfgEdge> = cfg.edges.iter().collect();

        let a = layout(&nodes, &edges);
        let b = layout(&nodes, &edges);
        assert_eq!(a, b, "layout must be deterministic for a given graph");

        // entry → branch → both arms → exit, one BFS hop per row.
        assert_eq!(a.nodes[0].depth, 0);
        assert_eq!(a.nodes[1].depth, 1);
        assert_eq!(a.nodes[2].depth, 2);
        assert_eq!(a.nodes[3].depth, 2);
        assert_eq!(a.nodes[4].depth, 3);
        assert!(a.width > 0.0 && a.height > 0.0);
    }

    #[test]
    fn over_limit_graphs_are_capped_but_counted() {
        let total = MAX_NODES + 50;
        let mut nodes = vec![node(0, CfgKind::Entry, "entry")];
        for i in 1..total as u32 {
            nodes.push(node(i, CfgKind::Block, "block"));
        }
        let mut edges = Vec::new();
        for i in 0..total as u32 - 1 {
            edges.push(edge(i, i + 1, CfgEdgeKind::Normal));
        }
        let cfg = cfg_of("src/big.rs:1", nodes, edges);

        let view = View::build(&cfg);
        assert!(view.capped, "over-limit graph must report capped");
        assert_eq!(view.nodes.len(), MAX_NODES);
        assert_eq!(view.total_nodes, total);
        // The capped subgraph still lays out without panicking.
        let l = layout(&view.nodes, &view.edges);
        assert!(l.nodes.len() == MAX_NODES);
    }

    #[test]
    fn empty_graph_and_dangling_edges_do_not_panic() {
        // No nodes at all, plus an edge whose endpoints do not exist.
        let cfg = cfg_of(
            "src/empty.rs:1",
            vec![],
            vec![edge(0, 1, CfgEdgeKind::Back)],
        );
        let view = View::build(&cfg);
        assert!(view.nodes.is_empty());
        assert!(view.edges.is_empty());
        assert!(view.by_id.is_empty());
        assert!(!view.capped);

        let l = layout(&view.nodes, &view.edges);
        assert!(l.nodes.is_empty());
        assert!(l.width > 0.0 && l.height > 0.0);
    }

    #[test]
    fn disconnected_nodes_and_back_edges_stay_in_the_view() {
        // No Entry node: BFS seeds from the lowest id, then the lone
        // disconnected node is appended, not dropped.
        let cfg = cfg_of(
            "src/mixed.rs:9",
            vec![
                node(0, CfgKind::Block, "a"),
                node(1, CfgKind::Block, "b"),
                node(7, CfgKind::Block, "island"),
            ],
            vec![
                edge(0, 1, CfgEdgeKind::Normal),
                edge(1, 0, CfgEdgeKind::Back),
            ],
        );
        let view = View::build(&cfg);
        assert_eq!(view.nodes.len(), 3, "disconnected nodes are not dropped");
        assert_eq!(view.edges.len(), 2, "back edge endpoints both survive");
        assert!(view.nodes.iter().any(|nd| nd.id == 7));
    }
}
