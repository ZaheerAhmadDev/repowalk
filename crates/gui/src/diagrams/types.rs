//! Type-structure diagram view. OWNER: GRAPH-3.
//!
//! Read `analysis.types` / `analysis.type_edges` — never re-derive edges; the
//! core `types::build` already drops dangling endpoints and duplicate edges.
use eframe::egui;

use crate::ui_kit;

pub fn ui(ui: &mut egui::Ui, app: &mut crate::app::App) {
    let detail = match &app.analysis {
        None => "no analysis yet — press Learn first".to_string(),
        Some(a) => format!(
            "analysis ready: {} type(s), {} type edge(s) (inherit/impl/field)",
            a.types.len(),
            a.type_edges.len()
        ),
    };
    ui_kit::not_ready(ui, "Type diagrams", "GRAPH-3", &detail);
}
