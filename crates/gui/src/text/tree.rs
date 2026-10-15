//! File tree pane: directories grouped as collapsible nodes (module color dot),
//! entrypoint markers ⟶, full-row click targets → `Selection::File`. OWNER: RSK-17.
use eframe::egui;
use std::collections::BTreeMap;

use crate::app::{App, Selection};
use crate::theme;
use repowalk_core::model::FileEntry;

/// One indexed file, already split for rendering.
struct FileRow {
    name: String,
    path: String,
    entrypoint: bool,
    lines: usize,
}

/// Directory node: files at this level plus nested subdirectories (sorted).
struct DirNode {
    full: String,
    files: Vec<FileRow>,
    subdirs: BTreeMap<String, DirNode>,
}

impl DirNode {
    fn new(full: &str) -> Self {
        Self {
            full: full.to_string(),
            files: Vec::new(),
            subdirs: BTreeMap::new(),
        }
    }

    /// Last path component — nesting is already expressed by indentation.
    fn name(&self) -> &str {
        match self.full.rsplit_once('/') {
            Some((_, last)) => last,
            None => &self.full,
        }
    }
}

pub fn ui(ui: &mut egui::Ui, app: &mut App) {
    let Some(idx) = app.index.clone() else {
        return;
    };

    theme::section_header(ui, "FILES");

    let root = build_tree(&idx.files);

    egui::ScrollArea::vertical()
        .id_salt("file-tree")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for row in &root.files {
                file_row(ui, app, row);
            }
            for dir in root.subdirs.values() {
                dir_ui(ui, app, dir, 0);
            }
        });
}

/// Group repo-relative paths into a directory tree (deterministic: `BTreeMap`).
fn build_tree(files: &[FileEntry]) -> DirNode {
    let mut root = DirNode::new("");
    for f in files {
        let (dir, name) = match f.path.rsplit_once('/') {
            Some((d, n)) => (d, n),
            None => ("", f.path.as_str()),
        };

        let mut node = &mut root;
        let mut prefix = String::new();
        for comp in dir.split('/').filter(|c| !c.is_empty()) {
            if !prefix.is_empty() {
                prefix.push('/');
            }
            prefix.push_str(comp);
            let full = prefix.clone();
            node = node
                .subdirs
                .entry(comp.to_string())
                .or_insert_with(|| DirNode::new(&full));
        }

        node.files.push(FileRow {
            name: name.to_string(),
            path: f.path.clone(),
            entrypoint: f.is_entrypoint,
            lines: f.lines,
        });
    }
    root.files.sort_by(|a, b| a.name.cmp(&b.name));
    root
}

fn dir_ui(ui: &mut egui::Ui, app: &mut App, dir: &DirNode, depth: usize) {
    // Top level stays open so the first click isn't a hunt; deeper nodes remember
    // their own state and open when they contain the active file.
    let default_open = depth == 0 || contains_open(app, dir);

    egui::collapsing_header::CollapsingState::load_with_default_open(
        ui.ctx(),
        egui::Id::new(("text-tree-dir", dir.full.clone())),
        default_open,
    )
    .show_header(ui, |ui| {
        ui.colored_label(theme::module_color(&dir.full), "●");
        ui.label(
            egui::RichText::new(dir.name())
                .strong()
                .color(theme::TEXT_PRIMARY),
        );
    })
    .body(|ui| {
        for row in &dir.files {
            file_row(ui, app, row);
        }
        for sub in dir.subdirs.values() {
            dir_ui(ui, app, sub, depth + 1);
        }
    });
}

fn contains_open(app: &App, dir: &DirNode) -> bool {
    app.open_file
        .as_deref()
        .is_some_and(|open| open.starts_with(&format!("{}/", dir.full)))
}

/// Full-row file target: entrypoint marker, name, right-aligned line count.
fn file_row(ui: &mut egui::Ui, app: &mut App, row: &FileRow) {
    let selected = app.open_file.as_deref() == Some(row.path.as_str());
    let path = row.path.clone();

    let resp = theme::list_row(ui, selected, ("text-tree-file", path.clone()), |ui| {
        // Transparent marker keeps names aligned; entrypoints read in accent.
        let marker = if row.entrypoint {
            theme::ACCENT
        } else {
            egui::Color32::TRANSPARENT
        };
        ui.colored_label(marker, "⟶");
        ui.label(egui::RichText::new(&row.name).color(if selected {
            theme::TEXT_PRIMARY
        } else {
            theme::TEXT_SECONDARY
        }));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.colored_label(
                theme::TEXT_MUTED,
                egui::RichText::new(row.lines.to_string()).size(theme::FONT_SMALL),
            );
        });
    });

    if resp.clicked() {
        app.select(Selection::File(path));
    }
}
