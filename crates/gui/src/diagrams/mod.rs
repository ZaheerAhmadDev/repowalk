//! Diagrams mode — control flow, types, modules (v2 F2/F3).
//!
//! OWNER of this file: integrator (mode + tab plumbing is pre-wired so the
//! diagram cards never touch it). Panels: GRAPH-2 fills `control.rs`, GRAPH-3
//! fills `types.rs` and `modules.rs`.
use eframe::egui;

use crate::theme;
use crate::ui_kit;

mod control;
mod modules;
mod types;

/// Which diagram the Diagrams mode is showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DiagramsTab {
    #[default]
    Control,
    Types,
    Modules,
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
        ],
    ) {
        app.diagrams.tab = tab;
    }
    theme::separator(ui);
    match app.diagrams.tab {
        DiagramsTab::Control => control::ui(ui, app),
        DiagramsTab::Types => types::ui(ui, app),
        DiagramsTab::Modules => modules::ui(ui, app),
    }
}
