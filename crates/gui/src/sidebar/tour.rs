//! Guided tour panel (reading order, clickable into text mode). OWNER: RSK-16.
use eframe::egui;

use crate::app::{App, Selection};
use repowalk_core::tour;

pub fn ui(ui: &mut egui::Ui, app: &mut App) {
    let Some(idx) = app.index.clone() else {
        return;
    };
    let items = tour::tour(&idx, 20);
    if items.is_empty() {
        return;
    }

    ui.heading("Tour — start here");
    egui::ScrollArea::vertical()
        .id_salt("sidebar-tour")
        .show(ui, |ui| {
            for (i, item) in items.iter().enumerate() {
                let selected = app.open_file.as_deref() == Some(item.path.as_str());
                let path = item.path.clone();
                let label = format!("{}. {}", i + 1, item.path);
                let resp = ui.selectable_label(selected, label);
                if resp.clicked() {
                    app.select(Selection::File(path));
                }
                if resp.hovered() {
                    resp.on_hover_text(item.why.clone());
                }
                ui.weak(format!("     {}", item.why));
            }
        });
}
