//! Graph mode: egui_graphs adapter + legend + interaction. OWNER of aggregate: main; parts: RSK-19/20.
pub mod interaction;
pub mod render;

use crate::app::App;
use eframe::egui;

pub fn ui(ui: &mut egui::Ui, app: &mut App) {
    // Fixed strips first (legend, then trace/hover chrome), then the
    // BG_BASE canvas frame — chrome never scrolls with the graph.
    render::legend(ui, app);
    interaction::handle(ui, app);
    render::ui(ui, app);
}
