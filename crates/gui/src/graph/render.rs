//! Graph rendering (F3 / batch B3): egui_graphs adapter with zoom-aware
//! real-name labels, module clustering, confidence/kind/module filters, a
//! search-to-highlight box, and honest empty/capped states.
//!
//! v1 graph by RSK-19; v2 upgrade owned by GRAPH-06. Node size = fan-in, color =
//! module, and every non-`Exact` edge stays dashed with a `?` (honesty R-20).
use eframe::egui;
use egui_graphs::{DisplayEdge, DisplayNode, DrawContext, EdgeProps, Graph, Node, NodeProps};
use petgraph::graph::DefaultIx;
use petgraph::stable_graph::{NodeIndex, StableGraph};
use petgraph::Directed;
use repowalk_core::model::{Confidence, EdgeKind, Index, Symbol};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

use crate::app::{App, TracePick};
use crate::graph::filter::{self, BuildSig, ConfidenceFilter, GraphUiState, NODE_CAP};
use crate::theme;

/// Node payload: everything the custom shape needs to draw honestly.
///
/// `label` is always a **real name** (symbol name or file path), never an id;
/// `short` is the low-zoom abbreviation. Ids live only in the hover tooltip.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NodeInfo {
    /// Symbol id, `file:<path>` for file nodes, or `cluster:<module>`.
    pub id: String,
    /// Full real name (symbol name / file path / module).
    pub label: String,
    /// Short name for low zoom (leaf segment, truncated).
    pub short: String,
    pub module: String,
    pub fan_in: usize,
    pub entrypoint: bool,
    /// True when this node stands in for a collapsed module.
    pub is_cluster: bool,
    /// Distinct members represented by a cluster node (1 for normal nodes).
    pub cluster_size: usize,
    /// File flagged as a god-file/over-connected file (v2 metrics emphasis).
    pub god: bool,
}

impl std::fmt::Display for NodeInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.label)
    }
}

/// Edge payload: dashed when the resolver was not Exact (honest edges, §4.4).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EdgeInfo {
    pub dashed: bool,
    pub kind: EdgeKind,
    pub confidence: Confidence,
}

impl std::fmt::Display for EdgeInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("")
    }
}

/// size = fan-in (plan §4.4): r = 5 + 2·√fan_in, capped.
pub fn node_radius(fan_in: usize) -> f32 {
    (5.0 + 2.0 * (fan_in as f32).sqrt()).min(26.0)
}

/// Cluster super-node radius grows with the number of collapsed members.
pub fn cluster_radius(size: usize) -> f32 {
    (9.0 + 3.0 * (size as f32).sqrt()).min(40.0)
}

/// Node sized by fan-in, colored by module, ring-marked entrypoints; clusters
/// draw as a rounded box. Fill stays on SURFACE_E3 so every node reads as the
/// same kind of object; module color moves to the border.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WalkNodeShape {
    pub pos: egui::Pos2,
    pub radius: f32,
    pub color: egui::Color32,
    pub label: String,
    pub short: String,
    pub entrypoint: bool,
    pub dimmed: bool,
    pub selected: bool,
    pub hovered: bool,
    pub is_cluster: bool,
    pub cluster_size: usize,
    pub god: bool,
}

impl From<NodeProps<NodeInfo>> for WalkNodeShape {
    fn from(p: NodeProps<NodeInfo>) -> Self {
        let is_cluster = p.payload.is_cluster;
        WalkNodeShape {
            pos: p.location(),
            radius: if is_cluster {
                cluster_radius(p.payload.cluster_size)
            } else {
                node_radius(p.payload.fan_in)
            },
            color: theme::module_color(&p.payload.module),
            label: p.label.clone(),
            short: p.payload.short.clone(),
            entrypoint: p.payload.entrypoint,
            dimmed: false,
            selected: p.selected,
            hovered: p.hovered,
            is_cluster,
            cluster_size: p.payload.cluster_size,
            god: p.payload.god,
        }
    }
}

impl DisplayNode<NodeInfo, EdgeInfo, Directed, DefaultIx> for WalkNodeShape {
    fn is_inside(&self, pos: egui::Pos2) -> bool {
        (pos - self.pos).length() <= self.radius
    }

    fn closest_boundary_point(&self, dir: egui::Vec2) -> egui::Pos2 {
        self.pos + dir.normalized() * self.radius
    }

    fn shapes(&mut self, ctx: &DrawContext) -> Vec<egui::Shape> {
        let mut res = Vec::new();
        let center = ctx.meta.canvas_to_screen_pos(self.pos);
        let radius = ctx.meta.canvas_to_screen_size(self.radius);
        let zoom = ctx.meta.zoom;
        let lit = !self.dimmed && (self.selected || self.hovered);

        let (fill, border, label_color) = if self.dimmed {
            let dim = theme::dim_alpha(90);
            (dim, dim, theme::TEXT_MUTED)
        } else {
            let label = if lit {
                theme::TEXT_PRIMARY
            } else {
                theme::TEXT_SECONDARY
            };
            (theme::SURFACE_E3, self.color, label)
        };

        // Zoom-aware level of detail. Full name on hover/selection or when
        // zoomed in; short name at low zoom; hidden when unreadable. Clusters
        // keep their "module (n)" caption longer because they carry meaning.
        let caption: Option<String> = if self.is_cluster {
            if lit || zoom >= 0.45 {
                let name = if lit || zoom >= 0.8 {
                    self.label.as_str()
                } else {
                    self.short.as_str()
                };
                Some(format!("{name} ({})", self.cluster_size))
            } else {
                None
            }
        } else if lit || zoom >= ZOOM_FULL {
            Some(self.label.clone())
        } else if zoom >= ZOOM_SHORT {
            Some(self.short.clone())
        } else {
            None
        };

        if self.is_cluster {
            let rect = egui::Rect::from_center_size(center, egui::vec2(radius * 2.2, radius * 1.5));
            let cr = egui::CornerRadius::same(theme::CORNER_MD);
            res.push(egui::Shape::rect_filled(
                rect,
                cr,
                theme::with_alpha(self.color, if self.dimmed { 24 } else { 46 }),
            ));
            res.push(egui::Shape::rect_stroke(
                rect,
                cr,
                egui::Stroke::new(1.5, border),
                egui::StrokeKind::Inside,
            ));
            if lit {
                res.push(egui::Shape::rect_stroke(
                    rect.expand(2.0),
                    egui::CornerRadius::same(theme::CORNER_MD + 1),
                    egui::Stroke::new(2.0, theme::ACCENT),
                    egui::StrokeKind::Outside,
                ));
            }
            if let Some(text) = caption {
                let galley = ctx.painter.layout_no_wrap(
                    text,
                    egui::FontId::new(theme::FONT_SMALL, egui::FontFamily::Monospace),
                    label_color,
                );
                let pos = rect.center() - galley.size() * 0.5;
                res.push(egui::epaint::TextShape::new(pos, galley, label_color).into());
            }
            return res;
        }

        // Subtle accent halo behind selected / hovered nodes.
        if lit {
            res.push(egui::Shape::circle_filled(
                center,
                radius + 4.0,
                theme::accent_subtle(35),
            ));
        }

        res.push(
            egui::epaint::CircleShape {
                center,
                radius,
                fill,
                stroke: egui::Stroke::new(1.5, border),
            }
            .into(),
        );

        // Entrypoint affordance: consistent outer ring, ACCENT when lit.
        if self.entrypoint {
            let ring = if self.dimmed {
                border
            } else {
                theme::ACCENT.gamma_multiply(0.75)
            };
            res.push(egui::Shape::circle_stroke(
                center,
                radius + 5.0,
                egui::Stroke::new(1.5, ring),
            ));
        }

        // God-file emphasis: a small WARNING dot on the node's shoulder.
        if self.god && !self.dimmed {
            res.push(egui::Shape::circle_filled(
                center + egui::vec2(radius * 0.7, -radius * 0.7),
                2.5,
                theme::WARNING,
            ));
        }

        // Selection / hover: crisp 2px ACCENT ring on top of the halo.
        if lit {
            res.push(egui::Shape::circle_stroke(
                center,
                radius + 1.5,
                egui::Stroke::new(2.0, theme::ACCENT),
            ));
        }

        // Label below the node in monospace, small; muted when dimmed.
        if let Some(text) = caption {
            let galley = ctx.painter.layout_no_wrap(
                text,
                egui::FontId::new(theme::FONT_SMALL, egui::FontFamily::Monospace),
                label_color,
            );
            let half = galley.size() * 0.5;
            let shape = egui::epaint::TextShape::new(
                center + egui::vec2(-half.x, radius + theme::SPACE_1),
                galley,
                label_color,
            );
            res.push(shape.into());
        }
        res
    }

    fn update(&mut self, state: &NodeProps<NodeInfo>) {
        // Fan-in / module color / LOD strings are baked at build time; the graph
        // is rebuilt whenever its signature changes, so only live geometry and
        // interaction state move here.
        self.pos = state.location();
        self.label = state.label.clone();
        self.selected = state.selected;
        self.hovered = state.hovered;
    }
}

/// Below this zoom only short labels are drawn.
const ZOOM_FULL: f32 = 1.15;
/// Below this zoom no labels are drawn (glyphs would be unreadable).
const ZOOM_SHORT: f32 = 0.55;

/// egui_graphs custom id: keys the persisted layout state and (pan/zoom)
/// metadata for the one graph view in the app. Keep in sync with `with_id`.
const GRAPH_ID: &str = "repowalk-graph";

// ── Interaction knobs (B5-03) ───────────────────────────────────────────────
//
// egui_graphs 0.32 already provides node drag (`SettingsInteraction::
// with_dragging_enabled`, which implicitly enables node clicking + hover),
// primary-drag pan over empty space, and zoom (Ctrl+wheel / pinch). The
// click-vs-drag threshold inside the crate rides on egui's click decision, so
// a sub-threshold press keeps counting as a click (selection stays intact).
// What the crate does NOT do out of the box here: select nodes for us
// (`with_node_selection_enabled` was off, so the app's selection reader saw
// nothing), and the Fruchterman–Reingold layout never stops stepping, which
// would shove a dragged node back onto nearby nodes forever. So we enable
// selection + dragging, and freeze the simulation once it has settled — after
// that, nodes stay exactly where the user drags them (B5-03).

/// Hard cap on simulation steps before the force layout freezes regardless of
/// displacement (no endless CPU burn on a stubborn layout).
pub const LAYOUT_STEPS_CAP: u64 = 600;
/// Freeze only after this many steps, so the initial spread has a chance to
/// act before we pin everything down.
pub const LAYOUT_SETTLE_MIN_STEPS: u64 = 90;
/// Average per-node displacement (graph units) under which the layout counts
/// as settled.
pub const LAYOUT_SETTLE_AVG: f32 = 0.25;

/// Pure freeze predicate: settled (few steps done + tiny average movement) or
/// the hard cap. Note `None` displacement (never measured) never freezes.
pub fn layout_should_freeze(steps: u64, avg_displacement: Option<f32>) -> bool {
    if steps >= LAYOUT_STEPS_CAP {
        return true;
    }
    steps >= LAYOUT_SETTLE_MIN_STEPS && avg_displacement.is_some_and(|avg| avg < LAYOUT_SETTLE_AVG)
}

/// Pure fit-to-rect transform using the crate's metadata semantics
/// (`screen = pan + canvas·zoom`): `screen = pan + canvas_pos·zoom`. Returns
/// `None` for empty/invalid input (never a wild zoom).
pub fn fit_pan_zoom(
    bounds: egui::Rect,
    canvas: egui::Vec2,
    padding: f32,
) -> Option<(f32, egui::Vec2)> {
    let dim = bounds.size();
    let ok = dim.x.is_finite()
        && dim.y.is_finite()
        && dim.x > 0.0
        && dim.y > 0.0
        && canvas.x.is_finite()
        && canvas.y.is_finite()
        && canvas.x > 0.0
        && canvas.y > 0.0;
    if !ok {
        return None;
    }
    let size = dim * (1.0 + padding).max(1.0);
    let zoom = (canvas.x / size.x).min(canvas.y / size.y);
    if !zoom.is_finite() || zoom <= 0.0 {
        return None;
    }
    Some((zoom, canvas * 0.5 - bounds.center().to_vec2() * zoom))
}

/// Perspective bounds of all nodes (position ± node half-extents + a label
/// allowance below, so labels aren't clipped by a tight fit).
fn node_bounds(g: &WalkGraph) -> egui::Rect {
    let mut r = egui::Rect::NOTHING;
    for n in g.g().node_weights() {
        let d = n.display();
        let (hx, hy) = if d.is_cluster {
            (d.radius * 1.1, d.radius * 0.75)
        } else {
            (d.radius, d.radius)
        };
        r.min.x = r.min.x.min(d.pos.x - hx);
        r.min.y = r.min.y.min(d.pos.y - hy);
        r.max.x = r.max.x.max(d.pos.x + hx);
        r.max.y = r.max.y.max(d.pos.y + hy + 34.0);
    }
    r
}

/// Store a computed fit into egui_graphs' (pan/zoom) metadata so the next
/// frame draws fitted. Honest no-op when the bounds degenerate.
fn store_fit(ui: &mut egui::Ui, g: &WalkGraph, canvas: egui::Vec2) {
    if let Some((zoom, pan)) = fit_pan_zoom(node_bounds(g), canvas, 0.1) {
        let mut meta = egui_graphs::MetadataFrame::new(Some(GRAPH_ID.to_string())).load(ui);
        meta.zoom = zoom;
        meta.pan = pan;
        meta.save(ui);
    }
}

/// Whether the user has manually panned/zoomed/moved a node since the last
/// (re)build — auto-fit defers to manual framing. egui temp memory.
#[derive(Clone, Debug, Default, PartialEq)]
struct NavFlags {
    user_adjusted: bool,
}

fn nav_flags_id() -> egui::Id {
    egui::Id::new("repowalk_graph_nav_flags")
}

fn load_nav_flags(ui: &egui::Ui) -> NavFlags {
    ui.ctx()
        .data_mut(|d| d.get_temp::<NavFlags>(nav_flags_id()))
        .unwrap_or_default()
}

fn save_nav_flags(ui: &egui::Ui, flags: NavFlags) {
    ui.ctx().data_mut(|d| d.insert_temp(nav_flags_id(), flags));
}

/// Point the persisted force-layout state at a fresh, running simulation
/// (used after a graph rebuild and by the legend's `relayout` control).
pub fn arm_layout(ui: &mut egui::Ui) {
    let mut st = egui_graphs::get_layout_state::<egui_graphs::FruchtermanReingoldState>(
        ui,
        Some(GRAPH_ID.to_string()),
    );
    st.is_running = true;
    st.step_count = 0;
    st.last_avg_displacement = None;
    egui_graphs::set_layout_state::<egui_graphs::FruchtermanReingoldState>(
        ui,
        st,
        Some(GRAPH_ID.to_string()),
    );
    save_nav_flags(ui, NavFlags::default());
}

/// Straight edge; dashed + `?` badge when the resolver was not Exact.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WalkEdgeShape {
    pub dashed: bool,
    pub kind: EdgeKind,
    pub dimmed: bool,
    pub selected: bool,
}

impl From<EdgeProps<EdgeInfo>> for WalkEdgeShape {
    fn from(p: EdgeProps<EdgeInfo>) -> Self {
        WalkEdgeShape {
            dashed: p.payload.dashed,
            kind: p.payload.kind,
            dimmed: false,
            selected: p.selected,
        }
    }
}

impl DisplayEdge<NodeInfo, EdgeInfo, Directed, DefaultIx, WalkNodeShape> for WalkEdgeShape {
    fn is_inside(
        &self,
        start: &Node<NodeInfo, EdgeInfo, Directed, DefaultIx, WalkNodeShape>,
        end: &Node<NodeInfo, EdgeInfo, Directed, DefaultIx, WalkNodeShape>,
        pos: egui::Pos2,
    ) -> bool {
        let (a, b) = (start.location(), end.location());
        let ab = b - a;
        let t = ((pos - a).dot(ab) / ab.length_sq()).clamp(0.0, 1.0);
        let closest = a + ab * t;
        (pos - closest).length() <= 6.0
    }

    fn shapes(
        &mut self,
        start: &Node<NodeInfo, EdgeInfo, Directed, DefaultIx, WalkNodeShape>,
        end: &Node<NodeInfo, EdgeInfo, Directed, DefaultIx, WalkNodeShape>,
        ctx: &DrawContext,
    ) -> Vec<egui::Shape> {
        let a = ctx.meta.canvas_to_screen_pos(start.location());
        let b = ctx.meta.canvas_to_screen_pos(end.location());

        // Kind → color: calls stay neutral, imports get the INFO tint; both at
        // ~55% alpha so edges recede behind nodes (figure/ground).
        const EDGE_ALPHA: u8 = 140;
        let (base, badge) = match self.kind {
            EdgeKind::Call => (
                theme::with_alpha(theme::TEXT_SECONDARY, EDGE_ALPHA),
                theme::TEXT_SECONDARY,
            ),
            EdgeKind::Import => (theme::with_alpha(theme::INFO, EDGE_ALPHA), theme::INFO),
        };
        let (color, width) = if self.dimmed {
            (theme::dim_alpha(55), 1.0)
        } else if self.selected {
            (theme::ACCENT, 1.8)
        } else {
            (base, 1.4)
        };
        let stroke = egui::Stroke::new(width, color);
        let mut res = Vec::new();
        if self.dashed {
            res.extend(egui::Shape::dashed_line(&[a, b], stroke, 6.0, 5.0));
            if !self.dimmed {
                let galley = ctx.painter.layout_no_wrap(
                    "?".to_string(),
                    egui::FontId::new(theme::FONT_SMALL, egui::FontFamily::Monospace),
                    badge,
                );
                let mid = (a.to_vec2() + b.to_vec2()) * 0.5;
                let text = egui::epaint::TextShape::new(
                    egui::Pos2::ZERO + mid - galley.size() * 0.5,
                    galley,
                    badge,
                );
                res.push(text.into());
            }
        } else {
            res.push(egui::Shape::line_segment([a, b], stroke));
        }
        res
    }

    fn update(&mut self, state: &EdgeProps<EdgeInfo>) {
        // Confidence tiers are fixed per edge at build time; only selection
        // highlight is live.
        self.selected = state.selected;
    }
}

pub type WalkGraph = Graph<NodeInfo, EdgeInfo, Directed, DefaultIx, WalkNodeShape, WalkEdgeShape>;

// ── Persistent view state (egui temp memory) ────────────────────────────────

fn state_id() -> egui::Id {
    egui::Id::new("repowalk_graph_ui_state")
}

/// Load the remembered filter/cluster state for this graph view.
pub fn load_state(ctx: &egui::Context) -> GraphUiState {
    ctx.data(|d| d.get_temp::<GraphUiState>(state_id()))
        .unwrap_or_default()
}

/// Store the filter/cluster state so the next frame (and `interaction`) sees it.
pub fn save_state(ctx: &egui::Context, state: GraphUiState) {
    ctx.data_mut(|d| d.insert_temp(state_id(), state));
}

// ── Build ───────────────────────────────────────────────────────────────────

/// Everything that steers a rebuild (folded into [`BuildSig`] by the caller).
pub struct BuildParams<'a> {
    pub calls: bool,
    pub imports: bool,
    pub confidence: ConfidenceFilter,
    pub module: Option<&'a str>,
    pub collapsed: &'a BTreeSet<String>,
    pub god_files: &'a BTreeSet<String>,
    pub cap: usize,
}

/// The built graph plus honest counts for the caps banner.
pub struct BuiltGraph {
    pub graph: WalkGraph,
    pub total_nodes: usize,
    pub shown_nodes: usize,
    pub truncated: bool,
}

/// Internal per-display-node accumulator.
#[derive(Clone)]
struct NodeMeta {
    module: String,
    label: String,
    short: String,
    entrypoint: bool,
    god: bool,
    is_cluster: bool,
}

/// Descriptor for one raw node id (symbol / file), before clustering.
struct RawInfo {
    module: String,
    label: String,
    short: String,
    entrypoint: bool,
    god: bool,
}

fn raw_info(
    raw: &str,
    sym_of: &BTreeMap<&str, &Symbol>,
    entry_files: &BTreeSet<&str>,
    god: &BTreeSet<String>,
) -> RawInfo {
    if let Some(path) = raw.strip_prefix("file:") {
        RawInfo {
            module: App::module_of(path),
            label: path.to_string(),
            short: basename(path),
            entrypoint: entry_files.contains(path),
            god: god.contains(path),
        }
    } else if let Some(sym) = sym_of.get(raw) {
        RawInfo {
            module: App::module_of(&sym.file),
            label: sym.name.clone(),
            short: short_label(&sym.name),
            entrypoint: entry_files.contains(sym.file.as_str()),
            god: god.contains(&sym.file),
        }
    } else {
        RawInfo {
            module: "(root)".to_string(),
            label: raw.to_string(),
            short: short_label(raw),
            entrypoint: false,
            god: false,
        }
    }
}

/// Map a raw id to its display node, applying the collapse set, and fold the
/// member's attributes into the display node's accumulated metadata.
fn ensure_node(
    raw: &str,
    info: &RawInfo,
    collapsed: &BTreeSet<String>,
    meta: &mut BTreeMap<String, NodeMeta>,
    members: &mut BTreeMap<String, BTreeSet<String>>,
) -> String {
    match filter::cluster_key(&info.module, collapsed) {
        Some(cluster) => {
            members
                .entry(cluster.clone())
                .or_default()
                .insert(raw.to_string());
            let m = meta.entry(cluster.clone()).or_insert_with(|| NodeMeta {
                module: info.module.clone(),
                label: info.module.clone(),
                short: short_label(&info.module),
                entrypoint: false,
                god: false,
                is_cluster: true,
            });
            m.entrypoint |= info.entrypoint;
            m.god |= info.god;
            cluster
        }
        None => {
            let m = meta.entry(raw.to_string()).or_insert_with(|| NodeMeta {
                module: info.module.clone(),
                label: info.label.clone(),
                short: info.short.clone(),
                entrypoint: false,
                god: false,
                is_cluster: false,
            });
            m.entrypoint |= info.entrypoint;
            m.god |= info.god;
            raw.to_string()
        }
    }
}

/// Build the call+import graph from the index.
///
/// Drops `Unresolved` edges (no trustworthy endpoint), applies the kind,
/// confidence and module filters, reroutes collapsed modules to super-nodes,
/// and — when the node count would exceed `cap` — keeps the top-fan-in nodes
/// and reports `truncated`. Node and edge order is deterministic.
pub fn build_graph(idx: &Index, p: &BuildParams) -> BuiltGraph {
    let entry_files: BTreeSet<&str> = idx
        .files
        .iter()
        .filter(|f| f.is_entrypoint)
        .map(|f| f.path.as_str())
        .collect();
    let sym_of: BTreeMap<&str, &Symbol> = idx.symbols.iter().map(|s| (s.id.as_str(), s)).collect();

    let mut meta: BTreeMap<String, NodeMeta> = BTreeMap::new();
    let mut members: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut edges: BTreeMap<(String, String), (EdgeKind, Confidence)> = BTreeMap::new();

    for e in &idx.edges {
        if e.confidence == Confidence::Unresolved {
            continue;
        }
        if !filter::kind_kept(e.kind, p.calls, p.imports) {
            continue;
        }
        if !p.confidence.keeps(e.confidence) {
            continue;
        }
        let a_info = raw_info(&e.caller, &sym_of, &entry_files, p.god_files);
        let b_info = raw_info(&e.callee, &sym_of, &entry_files, p.god_files);
        if let Some(m) = p.module {
            if a_info.module != m || b_info.module != m {
                continue;
            }
        }
        let a = ensure_node(&e.caller, &a_info, p.collapsed, &mut meta, &mut members);
        let b = ensure_node(&e.callee, &b_info, p.collapsed, &mut meta, &mut members);
        if a == b {
            continue;
        }
        edges.entry((a, b)).or_insert((e.kind, e.confidence));
    }

    // Fan-in over deduplicated display edges.
    let mut fan_in: BTreeMap<String, usize> = BTreeMap::new();
    for (_, callee) in edges.keys() {
        *fan_in.entry(callee.clone()).or_insert(0) += 1;
    }

    // Deterministic cap: top fan-in, ties broken by id.
    let total_nodes = meta.len();
    let mut keep: BTreeSet<String> = meta.keys().cloned().collect();
    let mut truncated = false;
    if p.cap > 0 && keep.len() > p.cap {
        let mut ranked: Vec<(usize, String)> = keep
            .iter()
            .map(|id| (fan_in.get(id).copied().unwrap_or(0), id.clone()))
            .collect();
        ranked.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
        ranked.truncate(p.cap);
        keep = ranked.into_iter().map(|(_, id)| id).collect();
        truncated = true;
    }

    let mut pet = StableGraph::<NodeInfo, EdgeInfo, Directed>::new();
    let mut nodes_of: BTreeMap<String, NodeIndex<DefaultIx>> = BTreeMap::new();
    for id in &keep {
        let m = &meta[id];
        let cluster_size = members.get(id).map_or(1, |s| s.len());
        let n = pet.add_node(NodeInfo {
            id: id.clone(),
            label: m.label.clone(),
            short: m.short.clone(),
            module: m.module.clone(),
            fan_in: fan_in.get(id).copied().unwrap_or(0),
            entrypoint: m.entrypoint,
            is_cluster: m.is_cluster,
            cluster_size,
            god: m.god,
        });
        nodes_of.insert(id.clone(), n);
    }
    for ((a, b), (kind, confidence)) in &edges {
        if !keep.contains(a) || !keep.contains(b) {
            continue;
        }
        let (Some(&na), Some(&nb)) = (nodes_of.get(a), nodes_of.get(b)) else {
            continue;
        };
        if na == nb || pet.find_edge(na, nb).is_some() {
            continue;
        }
        pet.add_edge(
            na,
            nb,
            EdgeInfo {
                kind: *kind,
                confidence: *confidence,
                dashed: filter::is_dashed(*confidence),
            },
        );
    }

    let shown_nodes = keep.len();
    BuiltGraph {
        graph: Graph::from(&pet),
        total_nodes,
        shown_nodes,
        truncated,
    }
}

/// Leaf segment of a symbol name: `a::b::c` → `c`, `Type::method` → `method`.
fn short_label(name: &str) -> String {
    let tail = name.rsplit("::").next().unwrap_or(name);
    let tail = tail.rsplit('.').next().unwrap_or(tail);
    if tail.chars().count() > 16 {
        format!("{}…", tail.chars().take(15).collect::<String>())
    } else {
        tail.to_string()
    }
}

fn basename(path: &str) -> String {
    path.rsplit('/').next().unwrap_or(path).to_string()
}

// ── Render ──────────────────────────────────────────────────────────────────

/// The graph canvas. Rebuilds only when the graph signature changes; otherwise
/// reuses the graph already in `App::graph_state`.
pub fn ui(ui: &mut egui::Ui, app: &mut App) {
    let mut state = load_state(ui.ctx());
    let Some(idx) = app.index.clone() else {
        empty_canvas(
            ui,
            "No index yet — open a repository to build the call graph.",
        );
        return;
    };

    let sig = BuildSig {
        repo: idx.repo_root.clone(),
        indexed_at: idx.indexed_at.clone(),
        edges: idx.edges.len(),
        symbols: idx.symbols.len(),
        calls: app.edge_calls,
        imports: app.edge_imports,
        confidence: state.confidence,
        module: state.module.clone(),
        collapsed: state.collapsed.clone(),
    };
    if app.graph_state.is_none() || state.last_sig.as_ref() != Some(&sig) {
        let god: BTreeSet<String> = app
            .analysis
            .as_ref()
            .map(|a| {
                repowalk_core::metrics::god_files(&idx, a)
                    .into_iter()
                    .map(|g| g.path)
                    .collect()
            })
            .unwrap_or_default();
        let built = build_graph(
            &idx,
            &BuildParams {
                calls: app.edge_calls,
                imports: app.edge_imports,
                confidence: state.confidence,
                module: state.module.as_deref(),
                collapsed: &state.collapsed,
                god_files: &god,
                cap: NODE_CAP,
            },
        );
        state.last_sig = Some(sig);
        state.total_nodes = built.total_nodes;
        state.shown_nodes = built.shown_nodes;
        state.truncated = built.truncated;
        app.graph_state = Some((
            (
                idx.edges.len(),
                idx.symbols.len(),
                app.edge_calls,
                app.edge_imports,
            ),
            built.graph,
        ));
        // Fresh graph, fresh positions: re-run (and re-fit) the force layout.
        arm_layout(ui);
        save_state(ui.ctx(), state.clone());
    }

    egui::Frame::new()
        .fill(theme::BG_BASE)
        .stroke(egui::Stroke::new(1.0, theme::BORDER_SUBTLE))
        .show(ui, |ui| {
            if state.truncated {
                caps_banner(ui, &state);
            }
            if state.shown_nodes == 0 {
                empty_canvas_inline(ui, &state, app);
                return;
            }
            // No ScrollArea wrapper: the viewer works in one transformable
            // canvas (pan/zoom), so a scroller would double-handle wheel
            // events and clip nothing that the viewer doesn't clip itself.
            let mut toggle_cluster: Option<String> = None;
            if let Some((_, g)) = app.graph_state.as_mut() {
                let mut interacted = false;
                let response = egui_graphs::GraphView::<
                    egui_graphs::FruchtermanReingoldState,
                    egui_graphs::LayoutForceDirected<egui_graphs::FruchtermanReingold>,
                >::new()
                .with_id(Some(GRAPH_ID.to_string()))
                .with_interactions(
                    &egui_graphs::SettingsInteraction::new()
                        .with_dragging_enabled(true)
                        .with_node_selection_enabled(true),
                )
                .with_navigations(
                    &egui_graphs::SettingsNavigation::new()
                        .with_zoom_and_pan_enabled(true)
                        // Per-frame refit would cancel user pan/zoom after
                        // every gesture; the crate still fits on the first
                        // frame (`first_frame_pending`), and B5-03's auto-fit
                        // keeps re-fitting only until the layout settles.
                        .with_fit_to_screen_enabled(false),
                )
                .show(ui, g);
                for change in &response.changes {
                    match change {
                        egui_graphs::GraphChange::NodeClicked { node } => {
                            if let Some(n) = g.node(*node) {
                                if n.payload().is_cluster {
                                    toggle_cluster = Some(n.payload().module.clone());
                                }
                            }
                        }
                        egui_graphs::GraphChange::Panned { .. }
                        | egui_graphs::GraphChange::Zoomed { .. }
                        | egui_graphs::GraphChange::NodeMoved { .. }
                        | egui_graphs::GraphChange::NodeDragStarted { .. } => {
                            interacted = true;
                        }
                        _ => {}
                    }
                }

                // While the force layout is still breathing, keep the whole
                // graph fitted (unless the user took manual control), and
                // freeze the simulation once it settles so dragged nodes stay
                // exactly where the user put them.
                let mut flags = load_nav_flags(ui);
                if interacted && !flags.user_adjusted {
                    flags.user_adjusted = true;
                    save_nav_flags(ui, flags.clone());
                }
                let settle = egui_graphs::get_layout_state::<egui_graphs::FruchtermanReingoldState>(
                    ui,
                    Some(GRAPH_ID.to_string()),
                );
                if settle.is_running {
                    if !flags.user_adjusted {
                        store_fit(ui, g, response.response.rect.size());
                    }
                    if layout_should_freeze(settle.step_count, settle.last_avg_displacement) {
                        let mut st = settle;
                        st.is_running = false;
                        egui_graphs::set_layout_state::<egui_graphs::FruchtermanReingoldState>(
                            ui,
                            st,
                            Some(GRAPH_ID.to_string()),
                        );
                    }
                }
            }
            // Cluster nodes toggle their module; symbols/files are selected
            // by `interaction::handle` (single source).
            if let Some(module) = toggle_cluster {
                let mut st = load_state(ui.ctx());
                if !st.collapsed.remove(&module) {
                    st.collapsed.insert(module);
                }
                save_state(ui.ctx(), st);
            }
        });
}

/// Honest caps banner: never silently drop nodes.
fn caps_banner(ui: &mut egui::Ui, state: &GraphUiState) {
    egui::Frame::new()
        .fill(theme::with_alpha(theme::WARNING, 22))
        .inner_margin(egui::Margin::symmetric(
            theme::SPACE_3 as i8,
            theme::SPACE_1 as i8,
        ))
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new(format!(
                    "showing {} of {} nodes — capped at {} for legibility (top fan-in kept)",
                    state.shown_nodes, state.total_nodes, NODE_CAP
                ))
                .size(theme::FONT_SMALL)
                .color(theme::WARNING),
            );
        });
}

fn empty_canvas(ui: &mut egui::Ui, msg: &str) {
    egui::Frame::new()
        .fill(theme::BG_BASE)
        .stroke(egui::Stroke::new(1.0, theme::BORDER_SUBTLE))
        .show(ui, |ui| {
            ui.add_space(theme::SPACE_8);
            ui.vertical_centered(|ui| {
                ui.label(
                    egui::RichText::new(msg)
                        .size(theme::FONT_BODY)
                        .color(theme::TEXT_SECONDARY),
                );
            });
        });
}

/// Empty-filter state drawn inside the canvas frame (not a blank surface).
fn empty_canvas_inline(ui: &mut egui::Ui, state: &GraphUiState, app: &App) {
    let filtered = state.module.is_some()
        || state.confidence == ConfidenceFilter::ExactOnly
        || !app.edge_calls
        || !app.edge_imports;
    let (head, sub) = if filtered {
        (
            "No nodes match the current filters.",
            "Clear the module/confidence filter or re-enable calls/imports above.",
        )
    } else {
        (
            "No resolved call/import edges to draw for this index.",
            "The index has symbols but no resolved edges (all edges may be unresolved).",
        )
    };
    ui.add_space(theme::SPACE_6);
    ui.vertical_centered(|ui| {
        ui.label(
            egui::RichText::new(head)
                .size(theme::FONT_BODY)
                .color(theme::TEXT_SECONDARY),
        );
        ui.label(
            egui::RichText::new(sub)
                .size(theme::FONT_SMALL)
                .color(theme::TEXT_MUTED),
        );
    });
}

/// Compact toggle styled for the legend strip: muted when off, accent wash +
/// ACCENT text when on (theme `selection` tokens drive the selected state).
fn legend_toggle(ui: &mut egui::Ui, on: &mut bool, label: &str) {
    ui.scope(|ui| {
        let style = ui.style_mut();
        style.visuals.override_text_color = Some(theme::TEXT_MUTED);
        style.visuals.selection.bg_fill = theme::accent_subtle(40);
        style.visuals.selection.stroke = egui::Stroke::new(1.0, theme::ACCENT);
        let w = &mut style.visuals.widgets;
        w.inactive.weak_bg_fill = egui::Color32::TRANSPARENT;
        w.inactive.bg_fill = egui::Color32::TRANSPARENT;
        w.inactive.bg_stroke = egui::Stroke::new(1.0, theme::BORDER_SUBTLE);
        w.hovered.weak_bg_fill = theme::SURFACE_HOVER;
        w.hovered.bg_fill = theme::SURFACE_HOVER;
        w.hovered.bg_stroke = egui::Stroke::new(1.0, theme::BORDER_STRONG);
        ui.toggle_value(on, egui::RichText::new(label).size(theme::FONT_SMALL));
    });
}

fn hint(text: &str) -> egui::RichText {
    egui::RichText::new(text)
        .size(theme::FONT_SMALL)
        .color(theme::TEXT_MUTED)
}

/// Legend / filter strip (plan §3.2 pick 3): module swatches, encoding hints,
/// edge toggles, filters (confidence / module / search), cluster controls, and
/// the trace entry point. Fixed above the canvas; never scrolls with the graph.
pub fn legend(ui: &mut egui::Ui, app: &mut App) {
    let mut state = load_state(ui.ctx());
    let response = egui::Frame::new()
        .fill(theme::SURFACE_E0)
        .inner_margin(egui::Margin::symmetric(
            theme::SPACE_3 as i8,
            theme::SPACE_2 as i8,
        ))
        .show(ui, |ui| {
            // Row 1 — module swatches, encoding hints, edge-kind toggles.
            ui.horizontal(|ui| {
                if let Some(idx) = app.index.clone() {
                    let mut modules: Vec<String> =
                        idx.files.iter().map(|f| App::module_of(&f.path)).collect();
                    modules.sort();
                    modules.dedup();
                    modules.truncate(8);
                    ui.spacing_mut().item_spacing.x = theme::SPACE_1;
                    for m in &modules {
                        let (rect, resp) =
                            ui.allocate_exact_size(egui::vec2(9.0, 9.0), egui::Sense::hover());
                        ui.painter()
                            .circle_filled(rect.center(), 3.5, theme::module_color(m));
                        resp.on_hover_text(m.clone());
                    }
                    ui.spacing_mut().item_spacing.x = theme::SPACE_2;
                    ui.add_space(theme::SPACE_1);
                }

                // Dynamic: the encoding/gesture hints step aside on a narrow
                // pane — the hover cards carry the same explanations.
                if ui.available_width() > 640.0 {
                    ui.label(hint(
                        "dots = code (symbols · files · modules) · arrows = \u{201c}uses\u{201d}",
                    ));
                    ui.label(hint("size = fan-in"));
                    ui.label(hint("dashed ? = low confidence"));
                    ui.label(hint("▢ cluster · ◆ god file"));
                    ui.label(hint(
                        "drag nodes · drag empty space to pan · ctrl+scroll to zoom",
                    ));
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .add(
                            egui::Button::new(
                                egui::RichText::new("trace A→B")
                                    .size(theme::FONT_SMALL)
                                    .color(theme::ACCENT),
                            )
                            .fill(egui::Color32::TRANSPARENT)
                            .stroke(egui::Stroke::new(1.0, theme::accent_subtle(90)))
                            .corner_radius(egui::CornerRadius::same(theme::CORNER_SM)),
                        )
                        .clicked()
                    {
                        app.trace_pick = TracePick::PickA;
                        app.pending_trace_a = None;
                    }
                    legend_toggle(ui, &mut app.edge_imports, "imports");
                    legend_toggle(ui, &mut app.edge_calls, "calls");
                });
            });

            // Row 2 — filters: confidence, module, search, cluster reset.
            ui.horizontal_wrapped(|ui| {
                let mut exact_only = state.confidence == ConfidenceFilter::ExactOnly;
                legend_toggle(ui, &mut exact_only, "exact only");
                if exact_only != (state.confidence == ConfidenceFilter::ExactOnly) {
                    state.confidence = if exact_only {
                        ConfidenceFilter::ExactOnly
                    } else {
                        ConfidenceFilter::All
                    };
                }

                ui.add_space(theme::SPACE_1);
                ui.label(hint("module"));
                egui::ComboBox::from_id_salt("graph-module-filter")
                    .selected_text(state.module.clone().unwrap_or_else(|| "all".to_string()))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut state.module, None, "all modules");
                        if let Some(idx) = app.index.clone() {
                            let mut mods: Vec<String> =
                                idx.files.iter().map(|f| App::module_of(&f.path)).collect();
                            mods.sort();
                            mods.dedup();
                            for m in mods {
                                ui.selectable_value(&mut state.module, Some(m.clone()), &m);
                            }
                        }
                    });

                ui.add_space(theme::SPACE_1);
                ui.label(hint("find"));
                ui.add(
                    egui::TextEdit::singleline(&mut state.search)
                        .hint_text("symbol / module…")
                        .desired_width(140.0)
                        .font(egui::TextStyle::Small),
                );

                ui.add_space(theme::SPACE_1);
                // Collapse/expand a module into a single super-node. Clicking an
                // on-canvas cluster also expands it; this menu creates clusters.
                ui.menu_button(
                    egui::RichText::new("cluster ▾").size(theme::FONT_SMALL),
                    |ui| {
                        if let Some(idx) = app.index.clone() {
                            let mut mods: Vec<String> =
                                idx.files.iter().map(|f| App::module_of(&f.path)).collect();
                            mods.sort();
                            mods.dedup();
                            if mods.is_empty() {
                                ui.label(hint("no modules"));
                            }
                            for m in mods {
                                let mut on = state.collapsed.contains(&m);
                                if ui.checkbox(&mut on, &m).changed() {
                                    if on {
                                        state.collapsed.insert(m.clone());
                                    } else {
                                        state.collapsed.remove(&m);
                                    }
                                }
                            }
                        }
                    },
                );

                if !state.collapsed.is_empty() {
                    ui.add_space(theme::SPACE_1);
                    if ui
                        .button(
                            egui::RichText::new(format!(
                                "expand {} clusters",
                                state.collapsed.len()
                            ))
                            .size(theme::FONT_SMALL),
                        )
                        .clicked()
                    {
                        state.collapsed.clear();
                    }
                }

                if let Some((_, g)) = &app.graph_state {
                    ui.add_space(theme::SPACE_2);
                    let counts = format!("{} nodes · {} edges", g.node_count(), g.edge_count());
                    ui.label(hint(&counts));
                }
                ui.add_space(theme::SPACE_2);
                // Re-run the force layout from scratch; it re-fits while it
                // settles, then freezes — user drags survive after that.
                if ui
                    .button(egui::RichText::new("relayout").size(theme::FONT_SMALL))
                    .on_hover_text("Restart the force layout and re-fit the view")
                    .clicked()
                {
                    arm_layout(ui);
                }
            });
        });
    // Hairline that separates the fixed strip from the canvas below.
    ui.painter().hline(
        response.response.rect.x_range(),
        response.response.rect.bottom(),
        egui::Stroke::new(1.0, theme::BORDER_SUBTLE),
    );
    save_state(ui.ctx(), state);
}

#[cfg(test)]
mod tests {
    use super::*;
    use repowalk_core::model::{Edge, SymbolKind};

    fn sym(id: &str, name: &str) -> Symbol {
        let (file, line) = id.rsplit_once(':').unwrap();
        Symbol {
            id: id.into(),
            name: name.into(),
            kind: SymbolKind::Function,
            signature: format!("fn {name}()"),
            doc: String::new(),
            start_line: line.parse().unwrap(),
            end_line: 0,
            container: None,
            file: file.into(),
        }
    }

    fn ed(caller: &str, callee: &str, kind: EdgeKind, confidence: Confidence) -> Edge {
        Edge {
            caller: caller.into(),
            callee: callee.into(),
            line: 1,
            kind,
            confidence,
        }
    }

    /// alpha → beta (exact call), alpha → gamma (name-only import).
    fn idx_two_modules() -> Index {
        let mut idx = Index::new("t");
        idx.symbols.push(sym("src/a.rs:1", "alpha"));
        idx.symbols.push(sym("src/a.rs:5", "beta"));
        idx.symbols.push(sym("src/b.rs:1", "gamma"));
        idx.edges.push(ed(
            "src/a.rs:1",
            "src/a.rs:5",
            EdgeKind::Call,
            Confidence::Exact,
        ));
        idx.edges.push(ed(
            "src/a.rs:1",
            "src/b.rs:1",
            EdgeKind::Import,
            Confidence::NameOnly,
        ));
        idx
    }

    fn params<'a>(collapsed: &'a BTreeSet<String>) -> BuildParams<'a> {
        BuildParams {
            calls: true,
            imports: true,
            confidence: ConfidenceFilter::All,
            module: None,
            collapsed,
            god_files: &EMPTY_GOD,
            cap: NODE_CAP,
        }
    }

    // A function-local `&'static` empty set keeps `params` borrow-clean.
    static EMPTY_GOD: BTreeSet<String> = BTreeSet::new();

    #[test]
    fn labels_are_real_names_and_non_exact_edges_are_dashed() {
        let idx = idx_two_modules();
        let collapsed = BTreeSet::new();
        let built = build_graph(&idx, &params(&collapsed));

        let mut labels: Vec<String> = built
            .graph
            .g()
            .node_weights()
            .map(|n| n.payload().label.clone())
            .collect();
        labels.sort();
        assert_eq!(
            labels,
            vec!["alpha", "beta", "gamma"],
            "real names, not ids"
        );
        // Ids must never be used as labels.
        assert!(built
            .graph
            .g()
            .node_weights()
            .all(|n| !n.payload().label.contains("src/")));

        let dashed: Vec<EdgeKind> = built
            .graph
            .g()
            .edge_weights()
            .filter(|e| e.payload().dashed)
            .map(|e| e.payload().kind)
            .collect();
        assert_eq!(
            dashed,
            vec![EdgeKind::Import],
            "only the NameOnly edge dashes"
        );
        assert_eq!(built.shown_nodes, 3);
        assert!(!built.truncated);
    }

    #[test]
    fn clustering_is_deterministic_and_collapses_only_listed_modules() {
        let idx = idx_two_modules();
        let mut collapsed = BTreeSet::new();
        collapsed.insert("src".to_string());
        let a = build_graph(&idx, &params(&collapsed));
        let b = build_graph(&idx, &params(&collapsed));

        // src has alpha+beta+gamma as distinct members → one super-node.
        let ids_a: Vec<(String, usize)> = a
            .graph
            .g()
            .node_weights()
            .map(|n| (n.payload().id.clone(), n.payload().cluster_size))
            .collect();
        let ids_b: Vec<(String, usize)> = b
            .graph
            .g()
            .node_weights()
            .map(|n| (n.payload().id.clone(), n.payload().cluster_size))
            .collect();
        assert_eq!(ids_a, ids_b, "cluster assignment is deterministic");
        assert_eq!(ids_a, vec![("cluster:src".to_string(), 3)]);
        let node = a.graph.g().node_weights().next().unwrap();
        assert!(node.payload().is_cluster);
        assert_eq!(
            node.payload().label,
            "src",
            "cluster shows the real module name"
        );

        // A non-collapsed module keeps its individual nodes.
        let open = BTreeSet::new();
        let c = build_graph(&idx, &params(&open));
        assert!(c.graph.g().node_weights().all(|n| !n.payload().is_cluster));
    }

    #[test]
    fn empty_and_filtered_states_do_not_panic_and_report_zero() {
        // Empty index.
        let empty = Index::new("t");
        let collapsed = BTreeSet::new();
        let built = build_graph(&empty, &params(&collapsed));
        assert_eq!(built.shown_nodes, 0);
        assert_eq!(built.total_nodes, 0);

        // Module filter that matches nothing → empty graph, no panic.
        let idx = idx_two_modules();
        let mut p = params(&collapsed);
        p.module = Some("nope");
        let filtered = build_graph(&idx, &p);
        assert_eq!(filtered.shown_nodes, 0);
        assert_eq!(filtered.graph.g().node_count(), 0);

        // Exact-only drops the NameOnly import (still keeps exact call nodes).
        let mut p2 = params(&collapsed);
        p2.confidence = ConfidenceFilter::ExactOnly;
        let exact = build_graph(&idx, &p2);
        assert!(exact.graph.g().edge_weights().all(|e| !e.payload().dashed));
    }

    #[test]
    fn cap_truncates_deterministically_and_is_reported() {
        let idx = idx_two_modules();
        let collapsed = BTreeSet::new();
        let mut p = params(&collapsed);
        p.cap = 1;
        let a = build_graph(&idx, &p);
        let b = build_graph(&idx, &p);
        assert!(a.truncated && b.truncated, "cap must be reported");
        assert_eq!(a.total_nodes, 3);
        assert_eq!(a.shown_nodes, 1);
        let id_a: Vec<String> = a
            .graph
            .g()
            .node_weights()
            .map(|n| n.payload().id.clone())
            .collect();
        let id_b: Vec<String> = b
            .graph
            .g()
            .node_weights()
            .map(|n| n.payload().id.clone())
            .collect();
        assert_eq!(id_a, id_b, "cap keeps a deterministic top-fan-in slice");
        // alpha has fan-in 0; beta and gamma each 1 → kept node is a callee.
        assert_eq!(id_a.len(), 1);
    }

    #[test]
    fn short_label_is_the_leaf_segment() {
        assert_eq!(short_label("foo::bar::baz"), "baz");
        assert_eq!(short_label("Type::method"), "method");
        assert_eq!(short_label("plain"), "plain");
        assert_eq!(basename("src/graph/render.rs"), "render.rs");
    }

    #[test]
    fn freeze_only_after_settling_or_cap() {
        // Too few steps: never frozen, even at zero displacement.
        assert!(!layout_should_freeze(
            LAYOUT_SETTLE_MIN_STEPS - 1,
            Some(0.0)
        ));
        // Settled: enough steps + tiny average displacement.
        assert!(layout_should_freeze(
            LAYOUT_SETTLE_MIN_STEPS,
            Some(LAYOUT_SETTLE_AVG * 0.9)
        ));
        // Still breathing: displacement above the settle threshold.
        assert!(!layout_should_freeze(
            LAYOUT_SETTLE_MIN_STEPS + 10,
            Some(LAYOUT_SETTLE_AVG * 3.0)
        ));
        // No measurement yet (never froze on missing data).
        assert!(!layout_should_freeze(LAYOUT_SETTLE_MIN_STEPS + 10, None));
        // Hard cap freezes regardless of displacement.
        assert!(layout_should_freeze(LAYOUT_STEPS_CAP, Some(100.0)));
        assert!(layout_should_freeze(LAYOUT_STEPS_CAP, None));
    }

    #[test]
    fn fit_pan_zoom_matches_metadata_semantics_and_rejects_junk() {
        use eframe::egui::{pos2, vec2};
        let bounds = egui::Rect::from_min_size(pos2(0.0, 0.0), vec2(100.0, 50.0));
        let canvas = vec2(200.0, 125.0);
        let (zoom, pan) = fit_pan_zoom(bounds, canvas, 0.0).unwrap();
        // Height is the binding axis: 125 / 50 = 2.5; width allows 2.0.
        assert_eq!(zoom, 2.0);
        // screen = pan + canvas·zoom maps the bounds center to canvas center.
        let screen_center = pan + bounds.center().to_vec2() * zoom;
        assert!((screen_center - canvas * 0.5).length() < 1e-4);
        // Padding widens the margins (smaller zoom, same centered result).
        let (z2, p2) = fit_pan_zoom(bounds, canvas, 0.1).unwrap();
        assert!(z2 < zoom);
        let screen2 = p2 + bounds.center().to_vec2() * z2;
        assert!((screen2 - canvas * 0.5).length() < 1e-4);
        // Degenerate input: no wild zoom.
        assert!(fit_pan_zoom(egui::Rect::NOTHING, canvas, 0.1).is_none());
        assert!(fit_pan_zoom(bounds, vec2(0.0, 125.0), 0.1).is_none());
        assert!(fit_pan_zoom(
            egui::Rect::from_min_size(pos2(0.0, 0.0), vec2(f32::NAN, 3.0)),
            canvas,
            0.1
        )
        .is_none());
    }

    /// Spot-check against a real learned index: every visible node label is a
    /// real name (symbol name / file path), never a `path:line` id.
    #[test]
    fn learned_index_labels_are_real_names() {
        let root = std::env::temp_dir().join(format!("rw-graph-label-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(
            root.join("src/main.rs"),
            "fn main() {\n    helper();\n}\n\nfn helper() {}\n",
        )
        .unwrap();

        let (idx, _analysis) = repowalk_core::learn::learn(&root, &|_| {}).unwrap();
        let collapsed = BTreeSet::new();
        let built = build_graph(&idx, &params(&collapsed));
        assert!(
            built.graph.g().edge_count() >= 1,
            "expected main→helper to resolve"
        );
        for node in built.graph.g().node_weights() {
            let p = node.payload();
            if p.id.starts_with("file:") {
                assert!(!p.label.is_empty());
            } else {
                assert!(
                    !p.label.contains(':'),
                    "node label {:?} looks like an id, not a real name",
                    p.label
                );
            }
        }
        let _ = std::fs::remove_dir_all(&root);
    }
}
