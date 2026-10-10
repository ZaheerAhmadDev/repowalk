//! Graph mode: egui_graphs adapter + legend + interaction. OWNER of aggregate: main; parts: RSK-19/20.
pub mod interaction;
pub mod render;

pub fn ui(ui: &mut egui::Ui, app: &mut crate::app::App) {
    render::ui(ui, app);
    interaction::handle(ui, app);
}
use eframe::egui;
