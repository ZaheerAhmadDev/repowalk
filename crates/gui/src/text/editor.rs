//! Editable syntax-highlighted pane (egui_code_editor), line numbers, Ctrl+S, jump/scroll. OWNER: RSK-18.
use eframe::egui;
use egui_code_editor::{CodeEditor, ColorTheme, Syntax};

use crate::app::App;

pub fn ui(ui: &mut egui::Ui, app: &mut App) {
    let Some(path) = app.open_file.clone() else {
        ui.centered_and_justified(|ui| ui.weak("select a file from the tree"));
        return;
    };
    let Some(saved) = app.editor_buffers.get(&path).cloned() else {
        ui.centered_and_justified(|ui| ui.weak("select a file from the tree"));
        return;
    };

    // Header runs before the buffer is checked out for editing.
    ui.horizontal(|ui| {
        ui.strong(&path);
        if app.dirty_edits.contains(&path) {
            ui.colored_label(egui::Color32::from_rgb(229, 192, 123), "● unsaved");
        }
        if ui.button("save (Ctrl+S)").clicked() {
            app.save_open_file();
        }
    });

    let mut text = saved.clone();
    let lang = editor_language(&path);
    let line_height = 14.0 * 1.35; // egui monospace row height approximation
    let jump = app.jump_line.take();

    egui::ScrollArea::vertical()
        .id_salt(("editor", path.clone()))
        .auto_shrink([false, false])
        .show(ui, |ui| {
            if let Some(line) = jump {
                // Approximate scroll target: line height × (line-1), top-aligned.
                let top = ui.cursor().top();
                let y = top + (line.saturating_sub(1)) as f32 * line_height;
                ui.scroll_to_rect(
                    egui::Rect::from_min_size(
                        egui::Pos2::new(ui.cursor().left(), y),
                        egui::vec2(1.0, line_height),
                    ),
                    Some(egui::Align::Min),
                );
            }
            CodeEditor::default()
                .id_source(format!("editor-{path}"))
                .with_rows(24)
                .with_fontsize(14.0)
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

/// egui_code_editor syntax for a repo-relative path (rust or python).
fn editor_language(path: &str) -> Syntax {
    if path.ends_with(".py") {
        Syntax::python()
    } else {
        Syntax::rust()
    }
}
