//! Graph interaction: hover preview, click → neighborhood highlight + dim,
//! A→B flow trace via BFS (plan §7.3). OWNER: RSK-20.
use eframe::egui;
use repowalk_core::model::{EdgeKind, Index};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::app::{App, Selection, TracePick};
use crate::theme;

/// BFS shortest call path from `a` to `b` over resolved call edges.
/// Returns the node id path including both endpoints, or None (honest "no path").
pub fn shortest_call_path(idx: &Index, a: &str, b: &str) -> Option<Vec<String>> {
    if a == b {
        return Some(vec![a.to_string()]);
    }
    let mut adj: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for e in &idx.edges {
        if e.kind == EdgeKind::Call && e.callee != e.caller {
            adj.entry(e.caller.as_str())
                .or_default()
                .push(e.callee.as_str());
        }
    }
    for v in adj.values_mut() {
        v.sort_unstable();
        v.dedup();
    }
    let mut prev: BTreeMap<&str, &str> = BTreeMap::new();
    let mut seen: BTreeSet<&str> = BTreeSet::from([a]);
    let mut queue = VecDeque::from([a]);
    while let Some(cur) = queue.pop_front() {
        for next in adj.get(cur).into_iter().flatten() {
            if !seen.insert(next) {
                continue;
            }
            prev.insert(next, cur);
            if *next == b {
                let mut path = vec![b.to_string()];
                let mut step = b;
                while let Some(&p) = prev.get(step) {
                    path.push(p.to_string());
                    step = p;
                }
                path.reverse();
                return Some(path);
            }
            queue.push_back(*next);
        }
    }
    None
}

/// Node ids dimmed when `selected` is highlighted: everything except the
/// selected node and its direct callers/callees.
pub fn dimmed_nodes(idx: &Index, selected: &str) -> BTreeSet<String> {
    let mut keep: BTreeSet<String> = BTreeSet::from([selected.to_string()]);
    for e in &idx.edges {
        if e.caller == selected {
            keep.insert(e.callee.clone());
        }
        if e.callee == selected {
            keep.insert(e.caller.clone());
        }
    }
    let mut dimmed = BTreeSet::new();
    for s in &idx.symbols {
        if !keep.contains(&s.id) {
            dimmed.insert(s.id.clone());
        }
    }
    dimmed
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
    let hovered_id: Option<String> = app.graph_state.as_ref().and_then(|(_, g)| {
        g.hovered_node()
            .and_then(|h| g.node(h))
            .map(|node| node.payload().id.clone())
    });
    if let Some(hovered) = hovered_id {
        egui::Frame::new()
            .fill(theme::SURFACE_E2)
            .inner_margin(egui::Margin::symmetric(
                theme::SPACE_2 as i8,
                theme::SPACE_1 as i8,
            ))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    match idx.symbols.iter().find(|s| s.id == hovered) {
                        Some(s) => {
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
                        }
                        None => {
                            // File-level node: show its path in code face.
                            let path = hovered.strip_prefix("file:").unwrap_or(hovered.as_str());
                            ui.colored_label(
                                theme::TEXT_SECONDARY,
                                egui::RichText::new(path)
                                    .family(egui::FontFamily::Monospace)
                                    .size(theme::FONT_SMALL),
                            );
                        }
                    }
                });
            });
    }

    // Node click: trace picking first, else selection sync. The graph borrow
    // ends before app mutations so the dimming pass can re-borrow.
    let clicked_id: Option<String> = app
        .graph_state
        .as_ref()
        .and_then(|(_, g)| g.selected_nodes().last().copied())
        .and_then(|n| app.graph_state.as_ref().unwrap().1.node(n))
        .map(|node| node.payload().id.clone());
    if let Some(id) = clicked_id {
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
            app.select(Selection::Symbol(sym.id.clone()));
        } else if let Some(path) = id.strip_prefix("file:") {
            app.select(Selection::File(path.to_string()));
        }
    }

    // Dimming: neighborhood highlight around selection, or trace path focus.
    let focus: Option<String> = app.selection.clone().map(|s| match s {
        Selection::Symbol(id) => id,
        Selection::File(p) => format!("file:{p}"),
    });
    let mut dim: BTreeSet<String> = focus
        .as_ref()
        .map(|f| dimmed_nodes(&idx, f))
        .unwrap_or_default();

    if let Some((a, b)) = app.trace.clone() {
        if let Some(path) = shortest_call_path(&idx, &a, &b) {
            // trace mode: highlight only the path
            dim = idx
                .symbols
                .iter()
                .map(|s| s.id.clone())
                .filter(|id| !path.contains(id))
                .collect();
        }
    }

    if let Some((_, g)) = app.graph_state.as_mut() {
        let indices: Vec<_> = g.g().node_indices().collect();
        for idx_in_graph in indices {
            if let Some(node) = g.g_mut().node_weight_mut(idx_in_graph) {
                node.display_mut().dimmed = !dim.is_empty() && dim.contains(&node.payload().id);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use repowalk_core::model::{CallSite, Edge, Symbol};

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
    fn dimming_keeps_only_neighborhood() {
        let idx = idx_chain();
        let dim = dimmed_nodes(&idx, "a.rs:5");
        assert!(!dim.contains("a.rs:5"), "selected stays lit");
        assert!(!dim.contains("a.rs:1"), "caller stays lit");
        assert!(!dim.contains("a.rs:9"), "callee stays lit");
        // a fourth, unconnected symbol is dimmed
        let mut idx2 = idx_chain();
        idx2.symbols.push(sym("d.rs:1", "d"));
        let dim2 = dimmed_nodes(&idx2, "a.rs:5");
        assert!(dim2.contains("d.rs:1"));
    }
}
