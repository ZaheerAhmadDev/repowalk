//! Shared view primitives for panels.
//! OWNER: integrator — pre-wired so parallel GUI cards never edit each other's
//! files (each card fills its own panel module; the chrome lives here).
use eframe::egui;

use crate::theme;

/// The hint a panel shows when the index is loaded but the analysis artifact is
/// not. One string for every caller, so the wording cannot drift — and so the
/// wording can be asserted without a UI (see the test below).
pub const NO_ANALYSIS_HINT: &str = "no analysis yet — press Learn first";

/// Centered empty-state card: what this view is, and why it has nothing to show.
///
/// Every panel renders this when it cannot show its data (no analysis yet, an
/// index-only open, a filter that matched nothing), so the app never shows a
/// blank pane. `hint` must name the thing that is actually missing ("no analysis
/// yet — press Learn first"); it must **not** claim the view is unbuilt, because
/// every v2 view has shipped.
pub fn empty_state(ui: &mut egui::Ui, title: &str, hint: &str) {
    ui.add_space(theme::SPACE_8);
    ui.vertical_centered(|ui| {
        ui.label(
            egui::RichText::new(title)
                .size(theme::FONT_HEADING)
                .color(theme::TEXT_SECONDARY),
        );
        if !hint.is_empty() {
            ui.add_space(theme::SPACE_3);
            ui.label(
                egui::RichText::new(hint)
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Regression (PROD-7): an empty state may say what is missing, but it must
    /// never tell the reader the view itself is unbuilt — every v2 view shipped,
    /// and a scaffold line reads as "not done" to anyone who just opened a repo
    /// that only indexed.
    #[test]
    fn the_no_analysis_hint_never_claims_the_view_is_unbuilt() {
        let hint = NO_ANALYSIS_HINT.to_lowercase();
        assert!(!hint.contains("card"), "{hint}");
        assert!(!hint.contains("lands"), "{hint}");
        assert!(!hint.contains("not ready"), "{hint}");
        assert!(hint.contains("learn"), "{hint}");
    }
}
