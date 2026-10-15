//! Graph rendering: egui_graphs adapter, custom shapes (size=fan-in, color=module,
//! dashed=? for non-Exact edges), legend, force-directed layout. OWNER: RSK-19.
use eframe::egui;
use egui_graphs::{DisplayEdge, DisplayNode, DrawContext, EdgeProps, Graph, Node, NodeProps};
use petgraph::graph::DefaultIx;
use petgraph::stable_graph::{NodeIndex, StableGraph};
use petgraph::Directed;
use repowalk_core::model::{Confidence, EdgeKind, Index};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::app::{App, TracePick};
use crate::theme;

/// Node payload: everything the custom shape needs to draw honestly.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NodeInfo {
    /// Symbol id, or `file:<path>` for file-level nodes.
    pub id: String,
    pub label: String,
    pub module: String,
    pub fan_in: usize,
    pub entrypoint: bool,
}

impl std::fmt::Display for NodeInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.label)
    }
}

/// Edge payload: dashed when the resolver was not Exact (honest edges, plan §4.4).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EdgeInfo {
    pub dashed: bool,
    pub kind: EdgeKind,
}

impl std::fmt::Display for EdgeInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("")
    }
}

/// Node sized by fan-in, colored by module, ring-marked entrypoints.
/// Fill stays on the SURFACE_E3 token so every node reads as the same kind of
/// object (similarity); the module color moves to the border (color channel).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WalkNodeShape {
    pub pos: egui::Pos2,
    pub radius: f32,
    pub color: egui::Color32,
    pub label: String,
    pub entrypoint: bool,
    pub dimmed: bool,
    pub selected: bool,
    pub hovered: bool,
}

/// size = fan-in (plan §4.4): r = 5 + 2·√fan_in, capped.
pub fn node_radius(fan_in: usize) -> f32 {
    (5.0 + 2.0 * (fan_in as f32).sqrt()).min(26.0)
}

impl From<NodeProps<NodeInfo>> for WalkNodeShape {
    fn from(p: NodeProps<NodeInfo>) -> Self {
        WalkNodeShape {
            pos: p.location(),
            radius: node_radius(p.payload.fan_in),
            color: theme::module_color(&p.payload.module),
            label: p.label.clone(),
            entrypoint: p.payload.entrypoint,
            dimmed: false,
            selected: p.selected,
            hovered: p.hovered,
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

        // Selection / hover: crisp 2px ACCENT ring on top of the halo.
        if lit {
            res.push(egui::Shape::circle_stroke(
                center,
                radius + 1.5,
                egui::Stroke::new(2.0, theme::ACCENT),
            ));
        }

        // Label below the node in monospace, small; muted when dimmed.
        let galley = ctx.painter.layout_no_wrap(
            self.label.clone(),
            egui::FontId::new(theme::FONT_SMALL, egui::FontFamily::Monospace),
            label_color,
        );
        let half = galley.size() * 0.5;
        let text = egui::epaint::TextShape::new(
            center + egui::vec2(-half.x, radius + theme::SPACE_1),
            galley,
            label_color,
        );
        res.push(text.into());
        res
    }

    fn update(&mut self, state: &NodeProps<NodeInfo>) {
        // Fan-in / module color are baked at build time; the graph is rebuilt
        // whenever the index changes, so only live geometry + interaction
        // state move here.
        self.pos = state.location();
        self.label = state.label.clone();
        self.selected = state.selected;
        self.hovered = state.hovered;
    }
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

/// Build the call+import graph from the index, skipping Unresolved edges.
pub fn build_graph(idx: &Index, edge_calls: bool, edge_imports: bool) -> WalkGraph {
    let mut fan_in: std::collections::HashMap<String, usize> = Default::default();
    let mut pet = StableGraph::<NodeInfo, EdgeInfo, Directed>::new();
    let mut node_of: std::collections::BTreeMap<String, NodeIndex<DefaultIx>> = BTreeMap::new();
    let entrypoints: std::collections::BTreeSet<&str> = idx
        .files
        .iter()
        .filter(|f| f.is_entrypoint)
        .map(|f| f.path.as_str())
        .collect();

    for e in &idx.edges {
        let keep = match e.kind {
            EdgeKind::Call => edge_calls,
            EdgeKind::Import => edge_imports,
        };
        if !keep || e.confidence == Confidence::Unresolved {
            continue;
        }
        *fan_in.entry(e.callee.clone()).or_insert(0) += 1;

        let mut node_for =
            |id: &str, pet: &mut StableGraph<NodeInfo, EdgeInfo, Directed>| -> NodeIndex {
                if let Some(n) = node_of.get(id) {
                    return *n;
                }
                let (module, label, entry) = match id.strip_prefix("file:") {
                    Some(path) => (
                        App::module_of(path),
                        path.to_string(),
                        entrypoints.contains(path),
                    ),
                    None => {
                        let sym = idx.symbols.iter().find(|s| s.id == id);
                        (
                            sym.map(|s| App::module_of(&s.file))
                                .unwrap_or_else(|| "(root)".to_string()),
                            label_of(idx, id),
                            sym.is_some_and(|s| entrypoints.contains(s.file.as_str())),
                        )
                    }
                };
                let n = pet.add_node(NodeInfo {
                    id: id.to_string(),
                    label,
                    module,
                    fan_in: 0,
                    entrypoint: entry,
                });
                node_of.insert(id.to_string(), n);
                n
            };

        let a = node_for(&e.caller, &mut pet);
        let b = node_for(&e.callee, &mut pet);
        if a != b && pet.find_edge(a, b).is_none() {
            pet.add_edge(
                a,
                b,
                EdgeInfo {
                    dashed: !matches!(e.confidence, Confidence::Exact),
                    kind: e.kind,
                },
            );
        }
    }

    // Fill fan-in counts now that all edges are known.
    for (id, n) in &node_of {
        if let Some(info) = pet.node_weight_mut(*n) {
            info.fan_in = fan_in.get(id).copied().unwrap_or(0);
        }
    }

    Graph::from(&pet)
}

fn label_of(idx: &Index, id: &str) -> String {
    if let Some(sym) = idx.symbols.iter().find(|s| s.id == id) {
        return sym.name.clone();
    }
    if let Some(path) = id.strip_prefix("file:") {
        return path.to_string();
    }
    id.to_string()
}

pub fn ui(ui: &mut egui::Ui, app: &mut App) {
    let Some(idx) = app.index.clone() else {
        return;
    };
    let signature = (
        idx.edges.len(),
        idx.symbols.len(),
        app.edge_calls,
        app.edge_imports,
    );
    let stale = app
        .graph_state
        .as_ref()
        .map(|(sig, _)| *sig != signature)
        .unwrap_or(true);
    if stale {
        let g = build_graph(&idx, app.edge_calls, app.edge_imports);
        app.graph_state = Some((signature, g));
    }
    let Some((_, g)) = app.graph_state.as_mut() else {
        return;
    };

    // The canvas is a deliberate BG_BASE plane, framed so it reads as the
    // graph's own surface rather than leftover panel space.
    egui::Frame::new()
        .fill(theme::BG_BASE)
        .stroke(egui::Stroke::new(1.0, theme::BORDER_SUBTLE))
        .show(ui, |ui| {
            egui::ScrollArea::both()
                .id_salt("graph-area")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    egui_graphs::GraphView::<
                        egui_graphs::FruchtermanReingoldState,
                        egui_graphs::LayoutForceDirected<egui_graphs::FruchtermanReingold>,
                    >::new()
                    .with_interactions(
                        &egui_graphs::SettingsInteraction::new().with_node_clicking_enabled(true),
                    )
                    .with_navigations(
                        &egui_graphs::SettingsNavigation::new()
                            .with_zoom_and_pan_enabled(true)
                            .with_fit_to_screen_enabled(true),
                    )
                    .show(ui, g);
                });
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

/// Legend row (plan §3.2 pick 3): fixed strip above the canvas — module
/// swatches, encoding hints, edge toggles, and the trace entry point.
pub fn legend(ui: &mut egui::Ui, app: &mut App) {
    let response = egui::Frame::new()
        .fill(theme::SURFACE_E0)
        .inner_margin(egui::Margin::symmetric(
            theme::SPACE_3 as i8,
            theme::SPACE_2 as i8,
        ))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                // Module color swatches (legend by example, not a wall of text).
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

                let hint = |text: &str| {
                    egui::RichText::new(text)
                        .size(theme::FONT_SMALL)
                        .color(theme::TEXT_MUTED)
                };
                ui.label(hint("size = fan-in"));
                ui.label(hint("dashed ? = low confidence"));

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
        });
    // Hairline that separates the fixed strip from the canvas below.
    ui.painter().hline(
        response.response.rect.x_range(),
        response.response.rect.bottom(),
        egui::Stroke::new(1.0, theme::BORDER_SUBTLE),
    );
}
