//! File tree pane (entrypoint markers ⟶, module color dots, click → Selection::File). OWNER: RSK-17.
use eframe::egui;
use std::collections::BTreeMap;

use crate::app::App;

pub fn ui(ui: &mut egui::Ui, app: &mut App) {
    let Some(idx) = app.index.clone() else {
        return;
    };

    // Group repo-relative paths by directory, sorted (deterministic like everything else).
    let mut dirs: BTreeMap<String, Vec<(String, bool)>> = BTreeMap::new();
    for f in &idx.files {
        let (dir, name) = match f.path.rsplit_once('/') {
            Some((d, n)) => (d.to_string(), n.to_string()),
            None => (String::new(), f.path.clone()),
        };
        dirs.entry(dir).or_default().push((name, f.is_entrypoint));
    }

    ui.heading("Files");
    egui::ScrollArea::vertical()
        .id_salt("file-tree")
        .show(ui, |ui| {
            for (dir, mut files) in dirs {
                files.sort();
                let open = app
                    .open_file
                    .as_deref()
                    .is_some_and(|of| of.starts_with(&format!("{dir}/")))
                    || dir.is_empty();
                egui::collapsing_header::CollapsingState::load_with_default_open(
                    ui.ctx(),
                    egui::Id::new(("tree-dir", dir.clone())),
                    open,
                )
                .show_header(ui, |ui: &mut egui::Ui| {
                    let color = App::module_color(&dir);
                    ui.colored_label(color, "●");
                    ui.strong(if dir.is_empty() {
                        ".".to_string()
                    } else {
                        dir.clone()
                    });
                })
                .body(|ui: &mut egui::Ui| {
                    for (name, entrypoint) in files {
                        let path = if dir.is_empty() {
                            name.clone()
                        } else {
                            format!("{dir}/{name}")
                        };
                        let marker = if entrypoint { "⟶ " } else { "   " };
                        let selected = app.open_file.as_deref() == Some(path.as_str());
                        let resp = ui.selectable_label(selected, format!("{marker}{name}"));
                        if resp.clicked() {
                            app.select(crate::app::Selection::File(path));
                        }
                    }
                });
            }
        });
}
