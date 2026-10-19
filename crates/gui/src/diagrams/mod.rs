//! Diagrams mode — control flow, types, modules, directory (v2 F2/F3 + B5).
//!
//! OWNER of this file: integrator (mode + tab plumbing is pre-wired so the
//! diagram cards never touch it). Panels: GRAPH-2 fills `control.rs`, GRAPH-3
//! fills `types.rs` and `modules.rs`; `interact.rs` is the shared pan/zoom/
//! node-drag layer (B5-02); `directory.rs` is the B5-05 panel.
use eframe::egui;

use crate::theme;
use crate::ui_kit;

mod control;
mod directory;
mod files;
mod modules;
mod types;

pub mod interact;

/// Which diagram the Diagrams mode is showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DiagramsTab {
    #[default]
    Control,
    Types,
    Modules,
    /// B5-05: the real directory tree, interactive.
    Directory,
    /// The whole code base as a file-level links graph (post-B5 user pass):
    /// one mini-view card per real file, arrows = file imports.
    Files,
}

/// Per-mode view state (survives frames).
#[derive(Default)]
pub struct DiagramsState {
    pub tab: DiagramsTab,
}

/// Central-pane entry for [`crate::app::Mode::Diagrams`].
pub fn ui(ui: &mut egui::Ui, app: &mut crate::app::App) {
    if let Some(tab) = ui_kit::tab_strip(
        ui,
        app.diagrams.tab,
        &[
            (DiagramsTab::Control, "Control flow"),
            (DiagramsTab::Types, "Types"),
            (DiagramsTab::Modules, "Modules"),
            (DiagramsTab::Directory, "Directory"),
            (DiagramsTab::Files, "File links"),
        ],
    ) {
        app.diagrams.tab = tab;
    }
    theme::separator(ui);
    match app.diagrams.tab {
        DiagramsTab::Control => control::ui(ui, app),
        DiagramsTab::Types => types::ui(ui, app),
        DiagramsTab::Modules => modules::ui(ui, app),
        DiagramsTab::Directory => directory::ui(ui, app),
        DiagramsTab::Files => files::ui(ui, app),
    }
}
