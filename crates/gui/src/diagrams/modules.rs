//! Module-structure diagram view. OWNER: GRAPH-3.
//!
//! Reads `analysis.modules` / `analysis.module_edges`. Those are populated by
//! `core::modules::build` through the learn pipeline (card CORE-9) — an empty
//! list here means the pipeline has not filled them yet, which this view says
//! out loud rather than drawing an empty frame.
//!
//! Renders the crate → dir → module → file tree as deterministic depth layers:
//! `Submodule` edges carry the nesting, `Import` edges are labelled with their
//! aggregated `count`, and nodes/edges that participate in a cycle (via core
//! [`repowalk_core::modules::cycles`]) are ringed in DANGER. Clicking a `File`
//! node opens it through the shared selection. Layout is capped for legibility.
use eframe::egui;
use egui::{Align2, FontId, Pos2, Rect, Stroke, Vec2};
use repowalk_core::analysis::{Analysis, ModuleEdge, ModuleEdgeKind, ModuleKind, ModuleNode};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::app::{App, Selection};
use crate::diagrams::interact;
use crate::theme;

/// egui-memory key for this canvas's interact layer (state survives tab
/// switches within one session; the layout id is global — one module graph).
const STATE_ID: &str = "diagrams-modules-interact";

// ── Geometry: fixed node size, layered grid → deterministic layout ──────────
const NODE_W: f32 = 184.0;
const NODE_H: f32 = 48.0;
const GAP_X: f32 = 64.0;
const GAP_Y: f32 = 18.0;
const TEXT_CHARS: usize = 26;

/// Legibility caps: draw at most this many nodes / import edges; the header
/// states the cap rather than quietly truncating.
pub const NODE_CAP: usize = 250;
pub const EDGE_CAP: usize = 400;

// ── Layout model (pure, unit-testable) ──────────────────────────────────────

/// One placed module node, in canvas coordinates (top-left origin).
#[derive(Debug, Clone, PartialEq)]
pub struct ModuleBox {
    pub id: String,
    pub kind: ModuleKind,
    pub depth: usize,
    pub pos: Pos2,
    pub size: Vec2,
    pub loc: usize,
    pub symbols: usize,
}

/// Deterministic placement of a capped prefix of the module graph.
#[derive(Debug, Clone, PartialEq)]
pub struct ModuleLayout {
    pub boxes: Vec<ModuleBox>,
    pub width: f32,
    pub height: f32,
    /// Nodes drawn / nodes present (cap honesty).
    pub shown: usize,
    pub total: usize,
    /// Number of depth layers used.
    pub layers: usize,
}

fn kind_rank(kind: ModuleKind) -> u8 {
    match kind {
        ModuleKind::Crate => 0,
        ModuleKind::Dir => 1,
        ModuleKind::Module => 2,
        ModuleKind::File => 3,
    }
}

fn kind_label(kind: ModuleKind) -> &'static str {
    match kind {
        ModuleKind::Crate => "crate",
        ModuleKind::Dir => "dir",
        ModuleKind::Module => "module",
        ModuleKind::File => "file",
    }
}

/// `parent → children` over `Submodule` edges (sorted, deduped).
fn submodule_children(edges: &[ModuleEdge]) -> BTreeMap<&str, Vec<&str>> {
    let mut map: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for e in edges {
        if e.kind == ModuleEdgeKind::Submodule {
            map.entry(e.from.as_str()).or_default().push(e.to.as_str());
        }
    }
    for children in map.values_mut() {
        children.sort_unstable();
        children.dedup();
    }
    map
}

/// `child → parent` over `Submodule` edges (first writer wins on the sorted
/// edge list, so a node reachable from several roots still resolves stably).
fn parent_map(edges: &[ModuleEdge]) -> BTreeMap<&str, &str> {
    let mut map: BTreeMap<&str, &str> = BTreeMap::new();
    for e in edges {
        if e.kind == ModuleEdgeKind::Submodule {
            map.entry(e.to.as_str()).or_insert(e.from.as_str());
        }
    }
    map
}

/// Depth of every node reachable from a root (`Crate` nodes, plus any node with
/// no `Submodule` parent). Breadth-first with sorted roots/children, so the
/// assignment is deterministic; unreachable nodes fall back to depth 0.
fn depths(nodes: &[ModuleNode], children: &BTreeMap<&str, Vec<&str>>) -> BTreeMap<String, usize> {
    let has_parent: BTreeSet<&str> = children.values().flatten().copied().collect();
    let mut roots: Vec<&str> = nodes
        .iter()
        .filter(|n| n.kind == ModuleKind::Crate || !has_parent.contains(n.id.as_str()))
        .map(|n| n.id.as_str())
        .collect();
    roots.sort_unstable();
    roots.dedup();

    let mut depth: BTreeMap<String, usize> = BTreeMap::new();
    let mut queue: VecDeque<(&str, usize)> = roots.into_iter().map(|r| (r, 0)).collect();
    while let Some((id, d)) = queue.pop_front() {
        if depth.contains_key(id) {
            continue; // already assigned at a shallower/equal depth (BFS order)
        }
        depth.insert(id.to_string(), d);
        if let Some(kids) = children.get(id) {
            for k in kids {
                queue.push_back((k, d + 1));
            }
        }
    }
    depth
}

/// Place `nodes` in depth columns, sorted by id. Only the first [`NODE_CAP`]
/// nodes are placed; `total` still reports the full count.
pub fn layout(nodes: &[ModuleNode], edges: &[ModuleEdge]) -> ModuleLayout {
    let total = nodes.len();
    let children = submodule_children(edges);
    let depth = depths(nodes, &children);

    let mut order: Vec<&ModuleNode> = nodes.iter().collect();
    order.sort_by(|a, b| {
        a.id.cmp(&b.id)
            .then_with(|| kind_rank(a.kind).cmp(&kind_rank(b.kind)))
    });
    order.truncate(NODE_CAP);

    let mut by_depth: BTreeMap<usize, Vec<&ModuleNode>> = BTreeMap::new();
    let mut max_depth = 0usize;
    for n in &order {
        let d = depth.get(n.id.as_str()).copied().unwrap_or(0);
        max_depth = max_depth.max(d);
        by_depth.entry(d).or_default().push(n);
    }

    let mut boxes = Vec::with_capacity(order.len());
    let mut width = 0.0f32;
    let mut height = 0.0f32;
    for (d, layer) in &by_depth {
        let x = *d as f32 * (NODE_W + GAP_X);
        for (i, n) in layer.iter().enumerate() {
            let y = i as f32 * (NODE_H + GAP_Y);
            boxes.push(ModuleBox {
                id: n.id.clone(),
                kind: n.kind,
                depth: *d,
                pos: Pos2::new(x, y),
                size: Vec2::new(NODE_W, NODE_H),
                loc: n.loc,
                symbols: n.symbols,
            });
            width = width.max(x + NODE_W);
            height = height.max(y + NODE_H);
        }
    }

    ModuleLayout {
        boxes,
        width,
        height,
        shown: order.len(),
        total,
        layers: max_depth + 1,
    }
}

/// Node ids that take part in any cycle (multi-node strongly-connected
/// component), from the core [`repowalk_core::modules::cycles`] result.
pub fn cycle_ids(cycles: &[Vec<String>]) -> BTreeSet<String> {
    cycles.iter().flatten().cloned().collect()
}

/// The crate/subtree root of `id`, walking `Submodule` parents (cycle-guarded).
fn root_of(id: &str, parent: &BTreeMap<&str, &str>) -> String {
    let mut cur = id;
    let mut guard = 0usize;
    while let Some(p) = parent.get(cur) {
        if *p == cur || guard > parent.len() {
            break;
        }
        cur = *p;
        guard += 1;
    }
    cur.to_string()
}

/// Honest empty state: `Some(reason)` when the pipeline produced no modules.
pub fn empty_notice(a: &Analysis) -> Option<String> {
    a.modules.is_empty().then(|| {
        format!(
            "analysis ready, but the module graph is empty ({} module edge(s)) — learn() has \
             not populated Analysis.modules yet (card CORE-9)",
            a.module_edges.len()
        )
    })
}

// ── Rendering ───────────────────────────────────────────────────────────────

fn font(size: f32, mono: bool) -> FontId {
    FontId::new(
        size,
        if mono {
            egui::FontFamily::Monospace
        } else {
            egui::FontFamily::Proportional
        },
    )
}

fn elide(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let mut out: String = text.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

fn arrow_head(painter: &egui::Painter, from: Pos2, to: Pos2, stroke: Stroke) {
    let dir = to - from;
    if dir.length_sq() < 1.0 {
        return;
    }
    let dir = dir.normalized();
    let back = to - dir * 9.0;
    let perp = Vec2::new(-dir.y, dir.x) * 4.0;
    let s = Stroke::new(stroke.width, stroke.color);
    painter.line_segment([to, back + perp], s);
    painter.line_segment([to, back - perp], s);
}

/// Centered empty-state card — the local multi-line variant of
/// `ui_kit::empty_state` (it takes a list of lines): it never claims the view
/// still "lands with" a card, because it has landed; only the pipeline data is
/// missing.
fn notice(ui: &mut egui::Ui, title: &str, lines: &[String]) {
    ui.add_space(theme::SPACE_8);
    ui.vertical_centered(|ui| {
        ui.label(
            egui::RichText::new(title)
                .size(theme::FONT_HEADING)
                .color(theme::TEXT_SECONDARY),
        );
        ui.add_space(theme::SPACE_2);
        for line in lines {
            ui.label(
                egui::RichText::new(line)
                    .size(theme::FONT_SMALL)
                    .color(theme::TEXT_PRIMARY),
            );
        }
    });
}

fn header(
    ui: &mut egui::Ui,
    lay: &ModuleLayout,
    edge_count: usize,
    cycles: &[Vec<String>],
    reset: &mut bool,
) {
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(format!(
                "{} node(s) · {} edge(s) · {} layer(s)",
                lay.total, edge_count, lay.layers
            ))
            .size(theme::FONT_SMALL)
            .color(theme::TEXT_SECONDARY),
        );
        if lay.shown < lay.total || edge_count > EDGE_CAP {
            ui.label(
                egui::RichText::new(format!(
                    "capped at {NODE_CAP} nodes / {EDGE_CAP} imports (showing {})",
                    lay.shown
                ))
                .size(theme::FONT_SMALL)
                .color(theme::WARNING),
            );
        }
        if !cycles.is_empty() {
            let label = ui.label(
                egui::RichText::new(format!("⟳ {} cycle(s)", cycles.len()))
                    .size(theme::FONT_SMALL)
                    .color(theme::DANGER),
            );
            let detail = cycles
                .iter()
                .take(6)
                .map(|c| c.join(" ↔ "))
                .collect::<Vec<_>>()
                .join("\n");
            label.on_hover_text(detail);
        }
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
            ui.label(
                egui::RichText::new(
                    "box = one module (a folder of the repo) · \
                     arrows crossing boxes = imports between modules (labelled ×N) · \
                     tree faint · ⟳ = cycle · wheel zoom · drag pan · drag box move",
                )
                .size(theme::FONT_SMALL)
                .color(theme::TEXT_MUTED),
            );
        });
    });
    theme::separator(ui);
}

/// Draw the module tree/DAG for the current analysis.
pub fn ui(ui: &mut egui::Ui, app: &mut App) {
    // Arc clone only — the analysis itself is never deep-cloned per frame.
    let Some(analysis) = app.analysis.clone() else {
        notice(
            ui,
            "Module diagrams",
            &["no analysis yet — press Learn (or run `repowalk learn`) first".to_string()],
        );
        return;
    };
    if let Some(msg) = empty_notice(&analysis) {
        notice(ui, "Module diagrams", &[msg]);
        return;
    }

    let cycles = repowalk_core::modules::cycles(&analysis.modules, &analysis.module_edges);
    let cycle_ids = cycle_ids(&cycles);
    let lay = layout(&analysis.modules, &analysis.module_edges);
    let parent = parent_map(&analysis.module_edges);
    let selected_file = match &app.selection {
        Some(Selection::File(path)) => Some(path.clone()),
        _ => None,
    };
    let mut reset_requested = false;

    header(
        ui,
        &lay,
        analysis.module_edges.len(),
        &cycles,
        &mut reset_requested,
    );

    // box id → position in `lay.boxes` / the placed screen-rect list below.
    let by_id: BTreeMap<&str, usize> = lay
        .boxes
        .iter()
        .enumerate()
        .map(|(i, b)| (b.id.as_str(), i))
        .collect();

    egui::Frame::new()
        .fill(theme::BG_BASE)
        .stroke(Stroke::new(1.0, theme::BORDER_SUBTLE))
        .show(ui, |ui| {
            // No ScrollArea (B5-02): pan — dragging empty canvas — replaced
            // scrolling inside the canvas, so the plain wheel is free to zoom
            // about the cursor (nodes here sit in fixed depth columns, so a
            // scroll bar said nothing that a pan drag does not say better).
            let (rect, resp) =
                ui.allocate_exact_size(ui.available_size(), egui::Sense::click_and_drag());
            let painter = ui.painter_at(rect);
            let origin = rect.min;
            let mut state = interact::CanvasState::load(ui.ctx(), egui::Id::new(STATE_ID));
            if reset_requested {
                state.reset();
            }
            let s = state.layer.scale;

            // Base depth-layer items for the interact layer; user drags/pan/
            // zoom apply strictly on top of the deterministic `layout`.
            let items: Vec<(String, Rect)> = lay
                .boxes
                .iter()
                .map(|b| (b.id.clone(), Rect::from_min_size(b.pos, b.size)))
                .collect();
            let (hovered, actions) = interact::frame(&resp, &mut state, &items);
            if actions.double_clicked_empty {
                state.reset();
            }
            let placed: Vec<Rect> = lay
                .boxes
                .iter()
                .map(|b| {
                    interact::placed_rect(
                        Rect::from_min_size(b.pos, b.size),
                        state.node_offset(&b.id),
                        state.layer,
                    )
                    .translate(origin.to_vec2())
                })
                .collect();

            // Nesting tree first, faint, so it recedes behind edges/nodes.
            // Endpoints follow dragged nodes.
            for e in &analysis.module_edges {
                if e.kind != ModuleEdgeKind::Submodule {
                    continue;
                }
                let (Some(&a), Some(&b)) = (by_id.get(e.from.as_str()), by_id.get(e.to.as_str()))
                else {
                    continue;
                };
                painter.line_segment(
                    [placed[a].center(), placed[b].center()],
                    Stroke::new(1.0, theme::with_alpha(theme::TEXT_MUTED, 70)),
                );
            }

            // Import edges, labelled with the aggregated count.
            for e in analysis
                .module_edges
                .iter()
                .filter(|e| e.kind == ModuleEdgeKind::Import)
                .take(EDGE_CAP)
            {
                let (Some(&a), Some(&b)) = (by_id.get(e.from.as_str()), by_id.get(e.to.as_str()))
                else {
                    continue;
                };
                let in_cycle = cycle_ids.contains(&e.from) && cycle_ids.contains(&e.to);
                let color = if in_cycle {
                    theme::DANGER
                } else {
                    theme::with_alpha(theme::INFO, 170)
                };
                let stroke = Stroke::new(if in_cycle { 1.8 } else { 1.2 }, color);
                let pa = placed[a].center();
                let pb = placed[b].center();
                painter.line_segment([pa, pb], stroke);
                arrow_head(&painter, pa, pb, stroke);
                let mid = pa + (pb - pa) * 0.5;
                painter.text(
                    mid,
                    Align2::CENTER_CENTER,
                    format!("×{}", e.count),
                    font(theme::FONT_SMALL * s, true),
                    color,
                );
            }

            // Nodes on top. `placed` is parallel to `lay.boxes`.
            for (bi, b) in lay.boxes.iter().enumerate() {
                let brect = placed[bi];
                let corner = egui::CornerRadius::same(theme::CORNER_MD);
                let group_color = theme::module_color(&root_of(&b.id, &parent));
                let sel = selected_file.as_deref() == Some(b.id.as_str());
                let hov = hovered.as_deref() == Some(b.id.as_str());
                let in_cycle = cycle_ids.contains(&b.id);

                let fill = if sel {
                    theme::SURFACE_SELECTED
                } else {
                    theme::SURFACE_E2
                };
                painter.rect_filled(brect, corner, fill);
                let (border, bw) = if in_cycle {
                    (theme::DANGER, 2.0)
                } else if sel || hov {
                    (theme::ACCENT, 2.0)
                } else {
                    (group_color, 1.4)
                };
                painter.rect_stroke(
                    brect,
                    corner,
                    Stroke::new(bw, border),
                    egui::StrokeKind::Middle,
                );

                let pad = Vec2::new(theme::SPACE_2 * s, (theme::SPACE_1 + 1.0) * s);
                painter.text(
                    brect.min + pad,
                    Align2::LEFT_TOP,
                    elide(&b.id, TEXT_CHARS),
                    font(theme::FONT_SMALL * s, true),
                    theme::TEXT_PRIMARY,
                );
                painter.text(
                    brect.right_top() + Vec2::new(-theme::SPACE_2 * s, (theme::SPACE_1 + 1.0) * s),
                    Align2::RIGHT_TOP,
                    kind_label(b.kind),
                    font(theme::FONT_SMALL * s, false),
                    group_color,
                );
                painter.text(
                    brect.min + Vec2::new(theme::SPACE_2 * s, 20.0 * s),
                    Align2::LEFT_TOP,
                    format!("{} loc · {} sym", b.loc, b.symbols),
                    font(theme::FONT_SMALL * s, false),
                    theme::TEXT_MUTED,
                );
                if in_cycle {
                    painter.text(
                        brect.right_bottom()
                            + Vec2::new(-theme::SPACE_2 * s, -(theme::SPACE_1 + 1.0) * s),
                        Align2::RIGHT_BOTTOM,
                        "⟳",
                        font(theme::FONT_BODY * s, false),
                        theme::DANGER,
                    );
                }
            }

            // Click a file node → open it through the shared selection (egui's
            // click-vs-drag threshold keeps an unmoved click a click).
            if let Some(id) = actions.clicked {
                if let Some(&bi) = by_id.get(id.as_str()) {
                    if lay.boxes[bi].kind == ModuleKind::File {
                        app.select(Selection::File(lay.boxes[bi].id.clone()));
                    }
                }
            }

            state.store(ui.ctx(), egui::Id::new(STATE_ID));
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(id: &str, kind: ModuleKind, loc: usize, symbols: usize) -> ModuleNode {
        ModuleNode {
            id: id.into(),
            kind,
            loc,
            symbols,
        }
    }

    fn edge(from: &str, to: &str, kind: ModuleEdgeKind, count: usize) -> ModuleEdge {
        ModuleEdge {
            from: from.into(),
            to: to.into(),
            kind,
            count,
        }
    }

    fn sample() -> (Vec<ModuleNode>, Vec<ModuleEdge>) {
        let nodes = vec![
            node("repo", ModuleKind::Crate, 60, 3),
            node("src", ModuleKind::Module, 60, 3),
            node("src/a.rs", ModuleKind::File, 30, 2),
            node("src/b.rs", ModuleKind::File, 30, 1),
        ];
        let edges = vec![
            edge("repo", "src", ModuleEdgeKind::Submodule, 1),
            edge("src", "src/a.rs", ModuleEdgeKind::Submodule, 1),
            edge("src", "src/b.rs", ModuleEdgeKind::Submodule, 1),
            edge("src/a.rs", "src/b.rs", ModuleEdgeKind::Import, 2),
        ];
        (nodes, edges)
    }

    #[test]
    fn layout_is_deterministic_and_layered() {
        let (nodes, edges) = sample();
        let first = layout(&nodes, &edges);
        let second = layout(&nodes, &edges);
        assert_eq!(first, second, "same input must lay out identically");
        assert_eq!(first.total, 4);
        assert_eq!(first.shown, 4);
        assert_eq!(first.layers, 3);

        let crate_box = first.boxes.iter().find(|b| b.id == "repo").unwrap();
        let file_box = first.boxes.iter().find(|b| b.id == "src/a.rs").unwrap();
        assert_eq!(crate_box.depth, 0);
        assert_eq!(file_box.depth, 2);
        assert!(crate_box.pos.x < file_box.pos.x, "depth grows to the right");
        // Edge order does not affect the result.
        let mut shuffled = edges.clone();
        shuffled.reverse();
        assert_eq!(layout(&nodes, &shuffled), first);
    }

    #[test]
    fn layout_caps_nodes_but_reports_total() {
        let mut nodes = vec![node("repo", ModuleKind::Crate, 0, 0)];
        for i in 0..NODE_CAP {
            nodes.push(node(&format!("src/f{i:04}.rs"), ModuleKind::File, 1, 0));
        }
        let l = layout(&nodes, &[]);
        assert_eq!(l.shown, NODE_CAP);
        assert_eq!(l.total, NODE_CAP + 1);
        assert_eq!(l.boxes.len(), NODE_CAP);
    }

    #[test]
    fn cycle_ids_marks_members_only() {
        let nodes = vec![
            node("a", ModuleKind::File, 0, 0),
            node("b", ModuleKind::File, 0, 0),
            node("c", ModuleKind::File, 0, 0),
        ];
        let edges = vec![
            edge("a", "b", ModuleEdgeKind::Import, 1),
            edge("b", "a", ModuleEdgeKind::Import, 1),
            edge("a", "c", ModuleEdgeKind::Import, 1),
        ];
        let ids = cycle_ids(&repowalk_core::modules::cycles(&nodes, &edges));
        assert!(ids.contains("a") && ids.contains("b"), "a↔b is a cycle");
        assert!(!ids.contains("c"), "c is not on a cycle");
    }

    #[test]
    fn empty_graph_is_an_empty_layout() {
        let l = layout(&[], &[]);
        assert!(l.boxes.is_empty());
        assert_eq!(l.shown, 0);
        assert_eq!(l.total, 0);
        assert_eq!(l.layers, 1);
    }

    #[test]
    fn empty_notice_names_core_9() {
        let empty = Analysis::new();
        let msg = empty_notice(&empty).expect("notice for an empty module graph");
        assert!(
            msg.contains("CORE-9"),
            "message names the pipeline card: {msg}"
        );

        let mut filled = Analysis::new();
        filled.modules.push(node("repo", ModuleKind::Crate, 0, 0));
        assert!(empty_notice(&filled).is_none());
    }
}
