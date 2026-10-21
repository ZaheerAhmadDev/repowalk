//! File tree pane: directories grouped as collapsible nodes (tinted folder
//! glyphs), files carrying colored type chips, entrypoint markers ⟶, full-row
//! click targets → `Selection::File`. OWNER: RSK-17.
use eframe::egui;
use std::collections::BTreeMap;

use crate::app::{App, Selection};
use crate::theme;
use repowalk_core::model::FileEntry;

use super::FilesDock;

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

    files_header(ui, app);

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

/// The FILES heading, and the view's drag story:
/// * **docked** — the label is a grab handle: dragging it beyond the click
///   threshold pops the tree out as a floating panel at the cursor;
/// * **floating** — the title bar already drags the window natively, so this
///   row carries the re-dock buttons instead.
///
/// The tree keeps its collapse memory through all of it (egui memory keyed on
/// the dir paths, not the container).
fn files_header(ui: &mut egui::Ui, app: &mut App) {
    ui.add_space(theme::SPACE_1);
    ui.horizontal(|ui| {
        let docked = app.files_dock != FilesDock::Float;
        if docked {
            let resp = ui.add(
                egui::Label::new(
                    egui::RichText::new("FILES")
                        .size(theme::FONT_SMALL)
                        .strong()
                        .color(theme::TEXT_MUTED),
                )
                .sense(egui::Sense::click_and_drag()),
            );
            let resp =
                resp.on_hover_text("drag to pull the file tree out — it floats, then re-docks");
            let pulled = resp.dragged_by(egui::PointerButton::Primary)
                && crate::diagrams::interact::beyond_tolerance(
                    resp.total_drag_delta().unwrap_or(egui::Vec2::ZERO),
                    crate::diagrams::interact::CLICK_TOLERANCE,
                );
            if pulled {
                app.files_dock = FilesDock::Float;
                app.files_float_pos = ui
                    .input(|i| i.pointer.hover_pos())
                    .map(|p| p - egui::vec2(60.0, 8.0));
            }
        } else {
            ui.label(
                egui::RichText::new("FILES")
                    .size(theme::FONT_SMALL)
                    .strong()
                    .color(theme::TEXT_MUTED),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .button("dock right")
                    .on_hover_text("put the tree into the right side panel")
                    .clicked()
                {
                    app.files_dock = FilesDock::Right;
                    app.files_float_pos = None;
                }
                if ui
                    .button("dock left")
                    .on_hover_text("put the tree back into the left side panel")
                    .clicked()
                {
                    app.files_dock = FilesDock::Left;
                    app.files_float_pos = None;
                }
            });
        }
    });
    ui.add_space(theme::SPACE_1);
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

    let state = egui::collapsing_header::CollapsingState::load_with_default_open(
        ui.ctx(),
        egui::Id::new(("text-tree-dir", dir.full.clone())),
        default_open,
    );
    // Folders are shapes, files are type chips: the open/closed glyph tells you
    // what a row *is* before you read its name.
    let open = state.is_open();
    state
        .show_header(ui, |ui| {
            ui.label(
                crate::icons::glyph(
                    if open {
                        crate::icons::FOLDER_OPENED
                    } else {
                        crate::icons::FOLDER
                    },
                    14.0,
                )
                .color(theme::module_color(&dir.full)),
            );
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

/// Full-row file target: entrypoint marker, type chip, name, right-aligned
/// line count.
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
        crate::icons::file_mark(ui, &row.path, 14.0);
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
