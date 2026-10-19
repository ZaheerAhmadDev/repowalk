//! Graph interaction: hover preview, click → neighborhood highlight + dim,
//! A→B flow trace via BFS (plan §7.3). OWNER: RSK-20.
use eframe::egui;
use repowalk_core::model::Index;
use std::collections::BTreeSet;

use crate::app::{App, Selection, TracePick};
use crate::graph::filter;
use crate::graph::render::{load_state, NodeInfo};
use crate::theme;

/// BFS shortest call path from `a` to `b` over resolved call edges.
/// Returns the node id path including both endpoints, or None (honest "no path").
///
/// Delegates to core's canonical trace (flow::CallGraph::build): call edges
/// are keyed by the CALLER's enclosing NAME in the index, and the old local
/// BFS keyed adjacency on that raw name — so a path from a SYMBOL ID couldn't
/// even take its first hop and Graph mode said "no call path" for pairs Flow
/// mode traced fine (two features in one app disagreeing).
pub fn shortest_call_path(idx: &Index, a: &str, b: &str) -> Option<Vec<String>> {
    repowalk_core::flow::trace(idx, a, b)
}

/// The focus node plus its direct callers/callees — the "keep lit" set used to
/// emphasise a selection's neighbourhood (focus + context dimming).
pub fn neighborhood(idx: &Index, focus: &str) -> BTreeSet<String> {
    let mut keep: BTreeSet<String> = BTreeSet::from([focus.to_string()]);
    for e in &idx.edges {
        if e.caller == focus {
            keep.insert(e.callee.clone());
        }
        if e.callee == focus {
            keep.insert(e.caller.clone());
        }
    }
    keep
}

/// Human-readable node name for banners: symbol name, or bare file path.
fn display_name(idx: &Index, id: &str) -> String {
    if let Some(sym) = idx.symbols.iter().find(|s| s.id == id) {
        return sym.name.clone();
    }
    id.strip_prefix("file:")
        .map(|p| p.to_string())
        .unwrap_or_else(|| id.to_string())
}

/// Small tinted strip button (Cancel / Clear affordances).
fn strip_button(ui: &mut egui::Ui, label: &str, color: egui::Color32) -> bool {
    ui.add(
        egui::Button::new(
            egui::RichText::new(label)
                .size(theme::FONT_SMALL)
                .color(color),
        )
        .fill(egui::Color32::TRANSPARENT)
        .stroke(egui::Stroke::new(1.0, color.gamma_multiply(0.35)))
        .corner_radius(egui::CornerRadius::same(theme::CORNER_SM)),
    )
    .clicked()
}

/// What the node under the cursor IS, in plain words — title + detail lines
/// for the hover card. The semantics are the index contract: symbols, whole
/// files or merged modules; arrows are calls (or file imports) the analysis
/// resolved, thicker/bigger when more code points in (fan-in).
fn node_tag(idx: &Index, id: &str, info: &NodeInfo) -> (String, Vec<String>) {
    if let Some(module) = id.strip_prefix("cluster:") {
        (
            format!("module {module}"),
            vec![
                format!(
                    "a collapsed box: the {} symbol(s) of this module, merged",
                    info.cluster_size.max(1)
                ),
                "click to expand it into its symbols".to_string(),
                "arrows crossing it = imports/calls to or from other modules".to_string(),
            ],
        )
    } else if let Some(s) = idx.symbols.iter().find(|s| s.id == id) {
        let mut lines = vec![
            format!(
                "defined at {}:{} (module {})",
                s.file, s.start_line, info.module
            ),
            format!(
                "{} incoming arrow{} = code that calls or uses it",
                info.fan_in,
                if info.fan_in == 1 { "" } else { "s" }
            ),
            "arrows out = what it calls/imports · dashed = guessed by name".to_string(),
        ];
        if !s.signature.is_empty() {
            lines.push(s.signature.clone());
        }
        (format!("{:?} {}", s.kind, s.name), lines)
    } else {
        let path = id.strip_prefix("file:").unwrap_or(id);
        (
            format!("file {path}"),
            vec![
                "a whole file: all of its symbols merged into one dot".to_string(),
                format!(
                    "{} incoming arrow{} = other code importing it",
                    info.fan_in,
                    if info.fan_in == 1 { "" } else { "s" }
                ),
                "click in Explorer or the file tree to drill into the file".to_string(),
            ],
        )
    }
}

pub fn handle(ui: &mut egui::Ui, app: &mut App) {
    let Some(idx) = app.index.clone() else {
        return;
    };

    // Trace banner (plan §3.2 pick 2): persistent strip while picking or
    // showing a resolved path.
    let banner_active = !matches!(app.trace_pick, TracePick::None) || app.trace.is_some();
    if banner_active {
        let mut cancel = false;
        let mut clear = false;
        egui::Frame::new()
            .fill(theme::SURFACE_E1)
            .stroke(egui::Stroke::new(1.0, theme::BORDER_SUBTLE))
            .inner_margin(egui::Margin::symmetric(
                theme::SPACE_3 as i8,
                theme::SPACE_2 as i8,
            ))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    match app.trace_pick {
                        TracePick::PickA => {
                            ui.label(
                                egui::RichText::new("Click node A")
                                    .size(theme::FONT_SMALL)
                                    .color(theme::TEXT_PRIMARY),
                            );
                            if strip_button(ui, "Cancel", theme::DANGER) {
                                cancel = true;
                            }
                        }
                        TracePick::PickB => {
                            ui.label(
                                egui::RichText::new("Click node B (from A)")
                                    .size(theme::FONT_SMALL)
                                    .color(theme::TEXT_PRIMARY),
                            );
                            if strip_button(ui, "Cancel", theme::DANGER) {
                                cancel = true;
                            }
                        }
                        TracePick::None => {}
                    }
                    // The resolved trace is only shown while not picking —
                    // the stale banner used to keep rendering below the
                    // "Click node A" prompt until a new trace resolved.
                    if app.trace.is_some() && matches!(app.trace_pick, TracePick::None) {
                        if let Some((a, b)) = app.trace.clone() {
                            match shortest_call_path(&idx, &a, &b) {
                                Some(p) => {
                                    // Short chains read in full; long ones abbreviate.
                                    let shown = if p.len() > 4 {
                                        format!(
                                            "{} → … → {}",
                                            display_name(&idx, &p[0]),
                                            display_name(&idx, &p[p.len() - 1])
                                        )
                                    } else {
                                        p.iter()
                                            .map(|id| display_name(&idx, id))
                                            .collect::<Vec<_>>()
                                            .join(" → ")
                                    };
                                    ui.colored_label(
                                        theme::SUCCESS,
                                        egui::RichText::new(format!(
                                            "{} ({} hops)",
                                            shown,
                                            p.len() - 1
                                        ))
                                        .size(theme::FONT_SMALL),
                                    );
                                }
                                None => {
                                    ui.colored_label(
                                        theme::DANGER,
                                        egui::RichText::new(format!(
                                            "no call path from {} to {}",
                                            display_name(&idx, &a),
                                            display_name(&idx, &b),
                                        ))
                                        .size(theme::FONT_SMALL),
                                    );
                                }
                            }
                        }
                        if strip_button(ui, "Clear", theme::TEXT_SECONDARY) {
                            clear = true;
                        }
                    }
                });
            });
        if cancel {
            app.trace_pick = TracePick::None;
            app.pending_trace_a = None;
        }
        if clear {
            app.trace = None;
        }
    }

    // Hover preview strip (under the legend): signature in CODE + file:line.
    let hovered: Option<(String, NodeInfo)> = app.graph_state.as_ref().and_then(|(_, g)| {
        g.hovered_node().and_then(|h| g.node(h)).map(|node| {
            let payload = node.payload();
            (payload.id.clone(), payload.clone())
        })
    });
    let hovered_id: Option<String> = hovered.as_ref().map(|(id, _)| id.clone());
    if let Some(hovered) = hovered_id {
        egui::Frame::new()
            .fill(theme::SURFACE_E2)
            .inner_margin(egui::Margin::symmetric(
                theme::SPACE_2 as i8,
                theme::SPACE_1 as i8,
            ))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    if let Some(module) = hovered.strip_prefix("cluster:") {
                        // Collapsed module cluster: real module name + hint.
                        ui.colored_label(
                            theme::TEXT_PRIMARY,
                            egui::RichText::new(format!("▢ {module}"))
                                .family(egui::FontFamily::Monospace)
                                .size(theme::FONT_CODE),
                        );
                        ui.colored_label(
                            theme::TEXT_MUTED,
                            egui::RichText::new("collapsed module — click to expand")
                                .size(theme::FONT_SMALL),
                        );
                    } else if let Some(s) = idx.symbols.iter().find(|s| s.id == hovered) {
                        ui.colored_label(
                            theme::TEXT_PRIMARY,
                            egui::RichText::new(&s.signature)
                                .family(egui::FontFamily::Monospace)
                                .size(theme::FONT_CODE),
                        );
                        ui.colored_label(
                            theme::TEXT_MUTED,
                            egui::RichText::new(format!("{}:{}", s.file, s.start_line))
                                .size(theme::FONT_SMALL),
                        );
                        // The id is inspector/tooltip-only — never a node label.
                        ui.colored_label(
                            theme::TEXT_DISABLED,
                            egui::RichText::new(&s.id)
                                .family(egui::FontFamily::Monospace)
                                .size(theme::FONT_SMALL),
                        );
                    } else {
                        // File-level node: show its path in code face + id.
                        let path = hovered.strip_prefix("file:").unwrap_or(hovered.as_str());
                        ui.colored_label(
                            theme::TEXT_SECONDARY,
                            egui::RichText::new(path)
                                .family(egui::FontFamily::Monospace)
                                .size(theme::FONT_SMALL),
                        );
                    }
                });
            });
    }

    // Plain-language hover card at the cursor — the understanding pass: what
    // this node IS, what its arrows mean (the strip above stays the technical
    // view: name in code face, file:line, id).
    if let Some((id, info)) = hovered {
        let (title, lines) = node_tag(&idx, &id, &info);
        egui::Popup::new(
            egui::Id::new("graph-hover-card"),
            ui.ctx().clone(),
            egui::PopupAnchor::Pointer,
            egui::LayerId::new(egui::Order::Tooltip, egui::Id::new("graph-hover-card")),
        )
        .kind(egui::PopupKind::Tooltip)
        .sense(egui::Sense::hover())
        .interactable(false)
        .show(|ui| {
            ui.label(
                egui::RichText::new(&title)
                    .strong()
                    .color(theme::TEXT_PRIMARY),
            );
            for line in lines {
                ui.label(
                    egui::RichText::new(line)
                        .size(theme::FONT_SMALL)
                        .color(theme::TEXT_SECONDARY),
                );
            }
        });
    }

    // Node click: trace picking first, else selection sync. Cluster nodes are
    // handled by `render::ui` (collapse/expand), so they are ignored here.
    //
    // Clicks arrive as GraphChange::NodeClicked events relayed through temp
    // memory by render (a one-frame hop — `handle` runs before the widget).
    // The crate's PERSISTENT node selection used to be read instead, so
    // after one frame every further frame re-consumed the same node: pick A
    // became pick B with the same id and the banner always resolved to
    // "A (0 hops)".
    let clicked_id: Option<String> = ui
        .ctx()
        .data_mut(|d| d.remove_temp::<String>(egui::Id::new("graph-node-clicked")));
    if let Some(id) = clicked_id {
        if !id.starts_with("cluster:") {
            match app.trace_pick {
                TracePick::PickA => {
                    app.pending_trace_a = Some(id);
                    app.trace_pick = TracePick::PickB;
                    return;
                }
                TracePick::PickB => {
                    if let Some(a) = app.pending_trace_a.take() {
                        app.trace = Some((a, id));
                    }
                    app.trace_pick = TracePick::None;
                    return;
                }
                TracePick::None => {}
            }
            if let Some(sym) = idx.symbols.iter().find(|s| s.id == id) {
                // Canvas click: keep the graph on screen — the highlight
                // inside the view is the visible effect. `select` here used
                // to reuse the repo tab as a text tab, yanking the reader
                // out of Graph mode so the highlight could only be seen
                // after navigating back.
                app.select_in_view(Selection::Symbol(sym.id.clone()));
            } else if let Some(path) = id.strip_prefix("file:") {
                app.select_in_view(Selection::File(path.to_string()));
            }
        }
    }

    // Focus+context dimming. A node stays lit when it matches the active search
    // OR falls in the focused neighbourhood (selection or resolved trace path).
    // No search and no focus ⇒ nothing is dimmed.
    let focus: Option<String> = app.selection.clone().map(|s| match s {
        Selection::Symbol(id) => id,
        Selection::File(p) => format!("file:{p}"),
    });
    let mut keep: Option<BTreeSet<String>> = focus.as_ref().map(|f| neighborhood(&idx, f));

    // A resolved trace highlights the PATH, not the neighbourhood: the
    // traversed edges go full-strength ACCENT (selected) and everything off
    // the path dims — the path used to be readable only via the text banner.
    let mut trace_edges: BTreeSet<(String, String)> = BTreeSet::new();
    if app.trace.is_some() && matches!(app.trace_pick, TracePick::None) {
        if let Some((a, b)) = app.trace.clone() {
            if let Some(path) = shortest_call_path(&idx, &a, &b) {
                keep = Some(path.iter().cloned().collect());
                trace_edges = path
                    .windows(2)
                    .map(|w| (w[0].clone(), w[1].clone()))
                    .collect();
            }
        }
    }

    let ui_state = load_state(ui.ctx());
    let search_active = !ui_state.search.trim().is_empty();

    if let Some((_, g)) = app.graph_state.as_mut() {
        let indices: Vec<_> = g.g().node_indices().collect();
        for idx_in_graph in indices {
            let (search_ok, focus_ok) = {
                let Some(node) = g.g().node_weight(idx_in_graph) else {
                    continue;
                };
                let p = node.payload();
                let search_ok = !search_active
                    || filter::matches_search(
                        &ui_state.search,
                        &[&p.label, &p.short, &p.id, &p.module],
                    );
                let focus_ok = keep.as_ref().is_none_or(|k| k.contains(&p.id));
                (search_ok, focus_ok)
            };
            if let Some(node) = g.g_mut().node_weight_mut(idx_in_graph) {
                node.display_mut().dimmed = !search_ok && !focus_ok;
            }
        }

        // Edge dim pass (above: nodes). While a trace is resolved, edges on
        // the path light up accent; everything else fades.
        if !trace_edges.is_empty() {
            // StableGraph edge iteration via petgraph's visitor traits.
            // Collect (id, endpoints, payload-ids) FIRST — the immutable
            // borrow must be released before the mutable dim pass below.
            use petgraph::visit::{EdgeRef as _, IntoEdgeReferences as _};
            let edges: Vec<(_, _, Option<String>, Option<String>)> = g
                .g()
                .edge_references()
                .map(|r| {
                    (
                        r.id(),
                        r.source(),
                        g.g()
                            .node_weight(r.source())
                            .map(|n| n.payload().id.clone()),
                        g.g()
                            .node_weight(r.target())
                            .map(|n| n.payload().id.clone()),
                    )
                })
                .collect();
            for (e_idx, _, sp, tp) in edges {
                let on_path = sp
                    .zip(tp)
                    .is_some_and(|(s, t)| trace_edges.contains(&(s, t)));
                if let Some(w) = g.g_mut().edge_weight_mut(e_idx) {
                    w.display_mut().dimmed = !on_path;
                    w.display_mut().selected = on_path;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use repowalk_core::model::{CallSite, Edge, EdgeKind, Symbol};

    fn sym(id: &str, name: &str) -> Symbol {
        Symbol {
            id: id.into(),
            name: name.into(),
            kind: repowalk_core::model::SymbolKind::Function,
            signature: format!("fn {name}()"),
            doc: String::new(),
            start_line: id.rsplit_once(':').unwrap().1.parse().unwrap(),
            end_line: 0,
            container: None,
            file: id.rsplit_once(':').unwrap().0.into(),
        }
    }

    fn idx_chain() -> Index {
        // a → b → c (call edges only)
        let mut idx = Index::new("t");
        for (id, name) in [("a.rs:1", "a"), ("a.rs:5", "b"), ("a.rs:9", "c")] {
            idx.symbols.push(sym(id, name));
        }
        idx.call_sites.push(CallSite {
            file: "a.rs".into(),
            callee: "b".into(),
            line: 2,
            enclosing: Some("a.rs:1".into()),
        });
        idx.edges.push(Edge {
            caller: "a.rs:1".into(),
            callee: "a.rs:5".into(),
            line: 2,
            kind: EdgeKind::Call,
            confidence: repowalk_core::model::Confidence::Exact,
        });
        idx.edges.push(Edge {
            caller: "a.rs:5".into(),
            callee: "a.rs:9".into(),
            line: 6,
            kind: EdgeKind::Call,
            confidence: repowalk_core::model::Confidence::Exact,
        });
        idx
    }

    #[test]
    fn bfs_finds_shortest_call_path() {
        let idx = idx_chain();
        let path = shortest_call_path(&idx, "a.rs:1", "a.rs:9").unwrap();
        assert_eq!(path, vec!["a.rs:1", "a.rs:5", "a.rs:9"]);
    }

    #[test]
    fn bfs_reports_no_path_honestly() {
        let idx = idx_chain();
        // edges are a→b→c; asking for the reverse direction must be None
        assert!(shortest_call_path(&idx, "a.rs:9", "a.rs:1").is_none());
    }

    #[test]
    fn bfs_same_node_is_single_hop() {
        let idx = idx_chain();
        assert_eq!(
            shortest_call_path(&idx, "a.rs:1", "a.rs:1").unwrap(),
            vec!["a.rs:1"]
        );
    }

    #[test]
    fn neighborhood_keeps_selection_and_direct_neighbours() {
        let idx = idx_chain();
        let keep = neighborhood(&idx, "a.rs:5");
        assert!(keep.contains("a.rs:5"), "selected stays lit");
        assert!(keep.contains("a.rs:1"), "caller stays lit");
        assert!(keep.contains("a.rs:9"), "callee stays lit");
        // A fourth, unconnected symbol is NOT in the neighbourhood.
        let mut idx2 = idx_chain();
        idx2.symbols.push(sym("d.rs:1", "d"));
        let keep2 = neighborhood(&idx2, "a.rs:5");
        assert!(!keep2.contains("d.rs:1"));
    }
}
