//! Editable syntax-highlighted pane: header bar (path · dirty · save),
//! egui_code_editor with line numbers, Ctrl+S, jump-to-line scroll. OWNER: RSK-18.
use eframe::egui;
use egui_code_editor::{CodeEditor, ColorTheme, Syntax};

use crate::app::App;
use crate::theme;

/// Line box used to turn a 1-based line number into a scroll offset.
const LINE_HEIGHT: f32 = theme::FONT_CODE * 1.55;
/// Approximate height of the single empty-state hint line.
const HINT_HEIGHT: f32 = 20.0;

pub fn ui(ui: &mut egui::Ui, app: &mut App) {
    let Some(path) = app.open_file.clone() else {
        empty_state(ui);
        return;
    };
    let Some(saved) = app.editor_buffers.get(&path).cloned() else {
        empty_state(ui);
        return;
    };

    header(ui, app, &path, app.dirty_edits.contains(&path));

    let mut text = saved.clone();
    let lang = editor_language(&path);
    let jump = app.jump_line.take();

    egui::ScrollArea::vertical()
        .id_salt(("editor", path.clone()))
        .auto_shrink([false, false])
        .show(ui, |ui| {
            if let Some(line) = jump {
                // Approximate scroll target: line height × (line-1), top-aligned.
                let top = ui.cursor().top();
                let y = top + (line.saturating_sub(1)) as f32 * LINE_HEIGHT;
                ui.scroll_to_rect(
                    egui::Rect::from_min_size(
                        egui::Pos2::new(ui.cursor().left(), y),
                        egui::vec2(1.0, LINE_HEIGHT),
                    ),
                    Some(egui::Align::Min),
                );
            }
            CodeEditor::default()
                .id_source(format!("editor-{path}"))
                .with_rows(24)
                .with_fontsize(theme::FONT_CODE)
                .with_theme(ColorTheme::AYU_DARK)
                .with_numlines(true)
                .with_wrap(false)
                .show(ui, &mut text, &lang);
        });

    // Store the edited buffer back; dirty iff it drifted from the saved copy.
    if text != saved {
        app.editor_buffers.insert(path.clone(), text);
        app.dirty_edits.insert(path);
    }
}

/// Header bar: path, unsaved marker, save action — then a hairline to the code.
fn header(ui: &mut egui::Ui, app: &mut App, path: &str, dirty: bool) {
    egui::Frame::new()
        .fill(theme::SURFACE_E1)
        .inner_margin(egui::Margin::symmetric(
            theme::SPACE_3 as i8,
            theme::SPACE_2 as i8,
        ))
        .show(ui, |ui| {
            ui.set_min_height(theme::ROW_HEIGHT);
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(path)
                        .strong()
                        .color(theme::TEXT_PRIMARY),
                );
                if dirty {
                    ui.colored_label(theme::WARNING, "●");
                    ui.label(
                        egui::RichText::new("unsaved")
                            .size(theme::FONT_SMALL)
                            .color(theme::WARNING),
                    );
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let save = egui::Button::new("Save (Ctrl+S)")
                        .min_size(egui::vec2(0.0, theme::ROW_HEIGHT));
                    if ui.add(save).clicked() {
                        app.save_open_file();
                    }
                });
            });
        });
    hairline(ui);
}

fn hairline(ui: &mut egui::Ui) {
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 1.0), egui::Sense::hover());
    ui.painter()
        .rect_filled(rect, egui::CornerRadius::same(0), theme::BORDER_SUBTLE);
}

fn empty_state(ui: &mut egui::Ui) {
    ui.centered_and_justified(|ui| {
        ui.add_space(((ui.available_height() - HINT_HEIGHT) * 0.5).max(theme::SPACE_8));
        ui.vertical_centered(|ui| {
            ui.label(
                egui::RichText::new("Select a file from the tree, or press Ctrl+K")
                    .size(theme::FONT_BODY)
                    .color(theme::TEXT_MUTED),
            );
        });
    });
}

/// egui_code_editor syntax for a repo-relative path (rust or python).
fn editor_language(path: &str) -> Syntax {
    if path.ends_with(".py") {
        Syntax::python()
    } else {
        Syntax::rust()
    }
}
