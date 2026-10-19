//! Explorer: the sidebar's real directory tree for the VS Code-like shell.
//! OWNER: integrator (card B5-04). The tree is built purely from `Index.files`
//! (real names, deterministic), directories collapse and remember their state,
//! and clicking a file opens it through the one selection path.

use eframe::egui;
use std::collections::BTreeMap;

use crate::app::{App, Selection};
use crate::theme;
use repowalk_core::model::FileEntry;

/// A directory under the repo root.
pub struct DirNode {
    /// Full path from the repo root (empty for the root itself).
    pub full: String,
    pub name: String,
    pub files: Vec<FileRow>,
    pub dirs: BTreeMap<String, DirNode>,
}

/// One indexed file, pre-split for rendering.
pub struct FileRow {
    pub name: String,
    pub path: String,
    pub lang: String,
    pub lines: usize,
    pub entrypoint: bool,
}

/// Group repo-relative paths into a tree: deterministic (`BTreeMap` everywhere,
/// children sorted by name).
pub fn build(files: &[FileEntry]) -> DirNode {
    let mut root = DirNode {
        full: String::new(),
        name: String::new(),
        files: Vec::new(),
        dirs: BTreeMap::new(),
    };
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
                .dirs
                .entry(comp.to_string())
                .or_insert_with(|| DirNode {
                    full,
                    name: comp.to_string(),
                    files: Vec::new(),
                    dirs: BTreeMap::new(),
                });
        }
        node.files.push(FileRow {
            name: name.to_string(),
            path: f.path.clone(),
            lang: f.lang.clone(),
            lines: f.lines,
            entrypoint: f.is_entrypoint,
        });
    }
    root.files.sort_by(|a, b| a.name.cmp(&b.name));
    root
}

/// Deep count of the files under `dir` (the muted number on the dir row).
pub fn count_files(dir: &DirNode) -> usize {
    dir.files.len() + dir.dirs.values().map(count_files).sum::<usize>()
}

/// The entry point: header, repo row, then the rows.
pub fn ui(ui: &mut egui::Ui, app: &mut App) {
    let Some(idx) = app.index.clone() else {
        return;
    };

    theme::section_header(ui, "EXPLORER");
    let name = app
        .repo
        .as_ref()
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "repository".into());
    ui.label(
        egui::RichText::new(format!("▾ {name}"))
            .size(theme::FONT_SMALL)
            .color(theme::TEXT_SECONDARY),
    );

    let root = build(&idx.files);
    egui::ScrollArea::vertical()
        .id_salt("explorer-tree")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for row in &root.files {
                file_ui(ui, app, row, 0);
            }
            for dir in root.dirs.values() {
                dir_ui(ui, app, dir, 1);
            }
        });
}

fn dir_ui(ui: &mut egui::Ui, app: &mut App, dir: &DirNode, depth: usize) {
    // Top level starts open so the first click is not a hunt; deeper nodes are
    // closed until opened, and one that holds the active file opens itself.
    let default_open = depth <= 1 || contains_open(app, dir);

    egui::collapsing_header::CollapsingState::load_with_default_open(
        ui.ctx(),
        egui::Id::new(("explorer-dir", dir.full.clone())),
        default_open,
    )
    .show_header(ui, |ui| {
        ui.add_space((depth as f32 - 1.0) * 12.0);
        ui.colored_label(theme::module_color(&dir.full), "▸");
        ui.label(
            egui::RichText::new(&dir.name)
                .strong()
                .color(theme::TEXT_PRIMARY),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.colored_label(
                theme::TEXT_MUTED,
                egui::RichText::new(count_files(dir).to_string()).size(theme::FONT_SMALL),
            );
        });
    })
    .body(|ui| {
        for row in &dir.files {
            file_ui(ui, app, row, depth);
        }
        for sub in dir.dirs.values() {
            dir_ui(ui, app, sub, depth + 1);
        }
    });
}

fn contains_open(app: &App, dir: &DirNode) -> bool {
    app.open_file
        .as_deref()
        .is_some_and(|open| open.starts_with(&format!("{}/", dir.full)))
}

fn file_ui(ui: &mut egui::Ui, app: &mut App, row: &FileRow, depth: usize) {
    let path = row.path.clone();
    // Root-level rows (depth 0) must not pull the cursor UP by 12px — a
    // negative add_space advanced/overlapped rows. Clamp at zero.
    ui.add_space((((depth as f32) - 1.0).max(0.0)) * 12.0);
    let loc = row.lines.to_string();
    let resp = crate::sidebar::data_row(
        ui,
        app.open_file.as_deref() == Some(path.as_str()),
        ("explorer-file", path.clone()),
        row.name.clone(),
        Some(crate::sidebar::Meta::muted(&loc)),
    );
    let resp = resp.on_hover_text(format!("{path} · {}", row.lang));
    if resp.clicked() {
        app.select(Selection::File(path));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fe(path: &str, lines: usize, entry: bool) -> FileEntry {
        FileEntry {
            path: path.into(),
            lang: "rust".into(),
            lines,
            mtime: 0,
            is_entrypoint: entry,
        }
    }

    #[test]
    fn the_tree_uses_real_directory_and_file_names() {
        let root = build(&[
            fe("src/main.rs", 3, true),
            fe("README.md", 1, false),
            fe("src/diagrams/types.rs", 12, false),
        ]);
        let src = root.dirs.get("src").unwrap();
        let dia = src.dirs.get("diagrams").unwrap();
        assert_eq!(dia.name, "diagrams");
        assert!(src
            .files
            .iter()
            .any(|f| f.name == "main.rs" && f.entrypoint));
        assert!(root.files.iter().any(|f| f.name == "README.md"));
        assert_eq!(dia.files[0].name, "types.rs");
        assert_eq!(dia.files[0].lines, 12);
    }

    #[test]
    fn empty_directories_are_not_invented() {
        let root = build(&[fe("src/main.rs", 1, false)]);
        // src exists (it holds main.rs); nothing else is fabricated.
        assert_eq!(root.dirs.len(), 1);
        assert!(root.dirs.get("src").unwrap().dirs.is_empty());
        let root2 = build(&[fe("README.md", 1, false)]);
        assert!(root2.dirs.is_empty());
    }

    #[test]
    fn deep_counts_roll_up() {
        let root = build(&[fe("src/main.rs", 1, false), fe("src/gui/a.rs", 1, false)]);
        let src = root.dirs.get("src").unwrap();
        assert_eq!(count_files(src), 2);
    }
}
