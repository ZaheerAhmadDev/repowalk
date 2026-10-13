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

use crate::app::App;

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
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WalkNodeShape {
    pub pos: egui::Pos2,
    pub radius: f32,
    pub color: egui::Color32,
    pub label: String,
    pub entrypoint: bool,
    pub dimmed: bool,
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
            color: App::module_color(&p.payload.module),
            label: p.label.clone(),
            entrypoint: p.payload.entrypoint,
            dimmed: false,
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
        let mut color = self.color;
        if self.dimmed {
            color = egui::Color32::from_rgba_unmultiplied(90, 90, 100, 90);
        }
        res.push(
            egui::epaint::CircleShape {
                center,
                radius,
                fill: color,
                stroke: egui::Stroke::new(1.0, egui::Color32::BLACK.gamma_multiply(0.4)),
            }
            .into(),
        );
        if self.entrypoint {
            res.push(egui::Shape::circle_stroke(
                center,
                radius + 3.0,
                egui::Stroke::new(2.0, color),
            ));
        }
        let galley =
            ctx.painter
                .layout_no_wrap(self.label.clone(), egui::FontId::monospace(11.0), color);
        let half = galley.size() * 0.5;
        let text = egui::epaint::TextShape::new(
            center - egui::vec2(half.x, radius + half.y + 2.0),
            galley,
            color,
        );
        res.push(text.into());
        res
    }

    fn update(&mut self, state: &NodeProps<NodeInfo>) {
        // Fan-in / module color are baked at build time; the graph is rebuilt
        // whenever the index changes, so only live geometry moves here.
        self.pos = state.location();
        self.label = state.label.clone();
    }
}

/// Straight edge; dashed + `?` badge when the resolver was not Exact.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WalkEdgeShape {
    pub dashed: bool,
    pub kind: EdgeKind,
    pub dimmed: bool,
}

impl From<EdgeProps<EdgeInfo>> for WalkEdgeShape {
    fn from(p: EdgeProps<EdgeInfo>) -> Self {
        WalkEdgeShape {
            dashed: p.payload.dashed,
            kind: p.payload.kind,
            dimmed: false,
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
        let mut color = match self.kind {
            EdgeKind::Call => egui::Color32::from_rgb(171, 178, 191),
            EdgeKind::Import => egui::Color32::from_rgb(86, 182, 194),
        };
        let mut width = 1.5;
        if self.dimmed {
            color = egui::Color32::from_rgba_unmultiplied(80, 80, 90, 60);
            width = 1.0;
        }
        let stroke = egui::Stroke::new(width, color);
        let mut res = Vec::new();
        if self.dashed {
            res.extend(egui::Shape::dashed_line(&[a, b], stroke, 6.0, 5.0));
            let mid = (a.to_vec2() + b.to_vec2()) * 0.5;
            let galley = ctx.painter.layout_no_wrap(
                "?".to_string(),
                egui::FontId::proportional(11.0),
                color,
            );
            let text = egui::epaint::TextShape::new(
                egui::Pos2::ZERO + mid - galley.size() * 0.5,
                galley,
                color,
            );
            res.push(text.into());
        } else {
            res.push(egui::Shape::line_segment([a, b], stroke));
        }
        res
    }

    fn update(&mut self, _state: &EdgeProps<EdgeInfo>) {
        // Confidence tiers are fixed per edge at build time.
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
}

/// Legend row (plan §3.2 pick 3).
pub fn legend(ui: &mut egui::Ui, app: &mut App) {
    ui.horizontal_wrapped(|ui| {
        ui.weak("size=fan-in · color=module · dashed ? = low confidence");
        ui.separator();
        ui.toggle_value(&mut app.edge_calls, "calls");
        ui.toggle_value(&mut app.edge_imports, "imports");
    });
}
