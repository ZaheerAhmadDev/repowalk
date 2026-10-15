//! Text mode: file tree (left) + editable pane (rest) + explain strip (bottom).
//! OWNER of aggregate: main agent; parts: RSK-17/18.
use eframe::egui;
use std::path::PathBuf;

use crate::app::{App, Selection};
use crate::theme;

pub mod editor;
pub mod tree;

/// Empty-state card metrics — the content is fixed, so a pre-measured height is
/// enough to center it without a second layout pass.
const CARD_WIDTH: f32 = 440.0;
const CARD_HEIGHT: f32 = 152.0;

pub fn ui(ui: &mut egui::Ui, app: &mut App) {
    if app.index.is_none() {
        open_repository(ui, app);
        return;
    }

    // Left: file tree. Full height, hairline edge (panel separator) to the editor.
    egui::Panel::left("text-files")
        .resizable(true)
        .default_size(240.0)
        .min_size(180.0)
        .max_size(theme::SIDEBAR_MAX)
        .show(ui, |ui| {
            egui::Frame::new()
                .inner_margin(egui::Margin::symmetric(
                    theme::SPACE_3 as i8,
                    theme::SPACE_3 as i8,
                ))
                .show(ui, |ui| tree::ui(ui, app));
        });

    // Bottom: explain strip. Auto-expands when a symbol lands, collapses on demand.
    explain_strip(ui, app);

    // Rest: editor.
    editor::ui(ui, app);
}

/// Centered "open a repository" card — the only place a repo path can be typed
/// before an index exists (once indexed, Open lives in the toolbar).
fn open_repository(ui: &mut egui::Ui, app: &mut App) {
    ui.centered_and_justified(|ui| {
        ui.add_space(((ui.available_height() - CARD_HEIGHT) * 0.5).max(theme::SPACE_6));
        ui.vertical_centered(|ui| {
            ui.allocate_ui(egui::vec2(CARD_WIDTH, 0.0), |ui| {
                egui::Frame::new()
                    .fill(theme::SURFACE_E2)
                    .stroke(egui::Stroke::new(1.0, theme::BORDER_SUBTLE))
                    .corner_radius(egui::CornerRadius::same(theme::CORNER_MD))
                    .inner_margin(egui::Margin::symmetric(
                        theme::SPACE_6 as i8,
                        theme::SPACE_6 as i8,
                    ))
                    .show(ui, |ui| {
                        ui.set_min_width(CARD_WIDTH - 2.0 * theme::SPACE_6 - 2.0);
                        ui.label(
                            egui::RichText::new("Open a repository")
                                .size(theme::FONT_HEADING)
                                .strong()
                                .color(theme::TEXT_PRIMARY),
                        );
                        ui.add_space(theme::SPACE_2);
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(
                                    "Point repowalk at a local checkout — it will index files, \
                                     symbols, and call edges.",
                                )
                                .size(theme::FONT_SMALL)
                                .color(theme::TEXT_MUTED),
                            )
                            .wrap(),
                        );
                        ui.add_space(theme::SPACE_4);
                        ui.horizontal(|ui| {
                            let resp = egui::TextEdit::singleline(&mut app.repo_input)
                                .hint_text("/path/to/repo")
                                .desired_width(300.0)
                                .show(ui);
                            let submitted = resp.response.lost_focus()
                                && ui.input(|i| i.key_pressed(egui::Key::Enter));
                            let ready = !app.repo_input.trim().is_empty();
                            if ui.add_enabled(ready, egui::Button::new("Index")).clicked()
                                || (submitted && ready)
                            {
                                let path = PathBuf::from(app.repo_input.trim());
                                app.start_indexing(path);
                            }
                        });
                    });
            });
        });
    });
}

/// Bottom explain strip: `CollapsingHeader`-style panel that opens on its own when
/// a symbol is selected and stays put once the user drags it shut.
fn explain_strip(ui: &mut egui::Ui, app: &mut App) {
    let ctx = ui.ctx().clone();
    let open_id = egui::Id::new("text-explain-open");
    let closed_for_id = egui::Id::new("text-explain-closed-for");

    // Default open so the "select a symbol" hint is discoverable on first run.
    let mut open = ctx
        .data_mut(|d| d.get_temp::<bool>(open_id))
        .unwrap_or(true);
    let closed_for: Option<String> = ctx
        .data_mut(|d| d.get_temp::<Option<String>>(closed_for_id))
        .flatten();

    let symbol: Option<String> = match &app.selection {
        Some(Selection::Symbol(id)) => Some(id.clone()),
        _ => None,
    };
    if let Some(id) = &symbol {
        if closed_for.as_deref() != Some(id.as_str()) {
            open = true;
        }
    }

    egui::Panel::bottom("text-explain")
        .resizable(true)
        .default_size(160.0)
        .min_size(88.0)
        .max_size(200.0)
        .show_collapsible(ui, &mut open, |ui| crate::explain::card(ui, app));

    let closed_for = if open { None } else { symbol };
    ctx.data_mut(|d| {
        d.insert_temp(open_id, open);
        d.insert_temp(closed_for_id, closed_for);
    });
}
