//! Text mode: file tree + editable pane + explain card. OWNER of aggregate: main agent; parts: RSK-17/18.
use eframe::egui;
use std::path::PathBuf;

use crate::app::App;

pub mod editor;
pub mod tree;

pub fn ui(ui: &mut egui::Ui, app: &mut App) {
    if app.index.is_none() {
        ui.centered_and_justified(|ui| {
            ui.weak("open a repository to begin — type a path below and press Index");
        });
        open_row(ui, app);
        return;
    }

    // File tree (left) + editor (rest), explain card underneath.
    egui::Panel::left("text-files")
        .resizable(true)
        .default_size(220.0)
        .show(ui, |ui| {
            tree::ui(ui, app);
        });

    editor::ui(ui, app);

    ui.separator();
    egui::ScrollArea::vertical()
        .id_salt("explain-card")
        .max_height(180.0)
        .show(ui, |ui| {
            crate::explain::card(ui, app);
        });

    ui.separator();
    open_row(ui, app);
}

/// Repo path row: type a folder and index it (in-app open-folder, plan §3.1).
fn open_row(ui: &mut egui::Ui, app: &mut App) {
    ui.horizontal(|ui| {
        ui.label("repo:");
        let resp = ui.text_edit_singleline(&mut app.repo_input);
        if (resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)))
            || ui.button("Index").clicked()
        {
            let path = PathBuf::from(app.repo_input.trim());
            if !path.as_os_str().is_empty() {
                app.start_indexing(path);
            }
        }
    });
}
