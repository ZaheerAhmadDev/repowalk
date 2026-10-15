//! Module-structure diagram view. OWNER: GRAPH-3.
//!
//! Reads `analysis.modules` / `analysis.module_edges`. Those are populated by
//! `core::modules::build` through the learn pipeline (card CORE-9) — an empty
//! list here means the pipeline has not filled them yet, which this view says
//! out loud rather than drawing an empty frame.
use eframe::egui;

use crate::ui_kit;

pub fn ui(ui: &mut egui::Ui, app: &mut crate::app::App) {
    let detail = match &app.analysis {
        None => "no analysis yet — press Learn first".to_string(),
        Some(a) if a.modules.is_empty() => {
            "analysis has no module tree yet — learn() does not populate\
             Analysis.modules (card CORE-9)"
                .to_string()
        }
        Some(a) => format!(
            "analysis ready: {} module node(s), {} module edge(s)",
            a.modules.len(),
            a.module_edges.len()
        ),
    };
    ui_kit::not_ready(ui, "Module diagrams", "GRAPH-3", &detail);
}
