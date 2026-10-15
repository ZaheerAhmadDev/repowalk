//! Guided tour panel (reading order, clickable into text mode). OWNER: RSK-16.
use eframe::egui;

use crate::app::{App, Selection};
use crate::sidebar::data_row;
use crate::theme;
use repowalk_core::tour;

/// Files offered by the tour; Enter walks the same order (see `App`).
const TOUR_LEN: usize = 20;

pub fn ui(ui: &mut egui::Ui, app: &mut App) {
    let Some(idx) = app.index.clone() else {
        return;
    };
    let items = tour::tour(&idx, TOUR_LEN);

    theme::section_header(ui, "Tour — start here");
    if items.is_empty() {
        ui.label(
            egui::RichText::new("nothing to tour")
                .size(theme::FONT_SMALL)
                .color(theme::TEXT_MUTED),
        );
        return;
    }

    let cap = (ui.available_height() * 0.45).clamp(theme::ROW_HEIGHT * 3.0, 260.0);
    egui::ScrollArea::vertical()
        .id_salt("sidebar-tour")
        .max_height(cap)
        .show(ui, |ui| {
            for (i, item) in items.iter().enumerate() {
                let selected = app.open_file.as_deref() == Some(item.path.as_str());
                let name = format!("{}. {}", i + 1, item.path);
                let resp = data_row(ui, selected, ("tour-item", i), name, None);
                if resp.clicked() {
                    let path = item.path.clone();
                    app.select(Selection::File(path));
                }
                // "Why this file" line, muted small so it reads as secondary text.
                ui.horizontal(|ui| {
                    ui.add_space(theme::SPACE_2);
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(&item.why)
                                .size(theme::FONT_SMALL)
                                .color(theme::TEXT_MUTED),
                        )
                        .truncate(),
                    );
                });
            }
        });
}
