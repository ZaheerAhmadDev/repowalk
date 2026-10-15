//! Shared view primitives for panels that a card still owns.
//! OWNER: integrator — pre-wired so parallel GUI cards never edit each other's
//! files (each card fills its own panel module; the chrome lives here).
use eframe::egui;

use crate::theme;

/// Centered "not built yet, and here is who owns it" placeholder.
///
/// Every panel renders this until its card lands, so the app never shows a blank
/// pane and the human always knows which card fills it. `detail` should carry a
/// live fact (how much data the analysis already holds) so a broken data path is
/// visible rather than hidden behind the placeholder.
pub fn not_ready(ui: &mut egui::Ui, title: &str, card: &str, detail: &str) {
    ui.add_space(theme::SPACE_8);
    ui.vertical_centered(|ui| {
        ui.label(
            egui::RichText::new(title)
                .size(theme::FONT_HEADING)
                .color(theme::TEXT_SECONDARY),
        );
        ui.add_space(theme::SPACE_2);
        ui.label(
            egui::RichText::new(format!("this view lands with card {card}"))
                .size(theme::FONT_SMALL)
                .color(theme::TEXT_MUTED),
        );
        if !detail.is_empty() {
            ui.add_space(theme::SPACE_3);
            ui.label(
                egui::RichText::new(detail)
                    .size(theme::FONT_SMALL)
                    .color(theme::TEXT_PRIMARY),
            );
        }
    });
}

/// A horizontal strip of mutually exclusive view tabs.
///
/// Returns the tab that was clicked, if any. One widget for every mode and
/// panel, so switching views always feels the same.
pub fn tab_strip<T: PartialEq + Copy>(
    ui: &mut egui::Ui,
    current: T,
    tabs: &[(T, &str)],
) -> Option<T> {
    let mut picked = None;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = theme::SPACE_3;
        for (tab, label) in tabs {
            if ui.selectable_label(*tab == current, *label).clicked() {
                picked = Some(*tab);
            }
        }
    });
    picked
}
