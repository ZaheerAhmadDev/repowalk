//! Graph mode: egui_graphs adapter + legend + interaction. OWNER of aggregate: main; parts: RSK-19/20.
pub mod interaction;
pub mod render;

use crate::app::App;
use eframe::egui;

pub fn ui(ui: &mut egui::Ui, app: &mut App) {
    render::legend(ui, app);
    render::ui(ui, app);
    interaction::handle(ui, app);
}
