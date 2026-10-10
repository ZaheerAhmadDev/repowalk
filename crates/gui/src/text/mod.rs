//! Text mode: file tree + editable pane. OWNER of aggregate: main agent; parts: RSK-17/18.
pub mod editor;
pub mod tree;

pub fn ui(ui: &mut egui::Ui, app: &mut crate::app::App) {
    ui.horizontal(|ui| {
        tree::ui(ui, app);
        editor::ui(ui, app);
    });
}
use eframe::egui;
