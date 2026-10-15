//! Usage panels — variables and references (v2 F4/F7).
//!
//! OWNER of this file: integrator (tab plumbing pre-wired). Panels: TEXT-2 fills
//! `variables.rs`, TEXT-3 fills `refs.rs`.
use eframe::egui;

use crate::theme;
use crate::ui_kit;

mod refs;
mod variables;

/// Which usage panel the sidebar is showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum UsageTab {
    #[default]
    Variables,
    Refs,
}

/// Per-panel view state.
#[derive(Default)]
pub struct UsageState {
    pub tab: UsageTab,
}

pub fn ui(ui: &mut egui::Ui, app: &mut crate::app::App) {
    if let Some(tab) = ui_kit::tab_strip(
        ui,
        app.usage.tab,
        &[
            (UsageTab::Variables, "Variables"),
            (UsageTab::Refs, "References"),
        ],
    ) {
        app.usage.tab = tab;
    }
    theme::separator(ui);
    match app.usage.tab {
        UsageTab::Variables => variables::ui(ui, app),
        UsageTab::Refs => refs::ui(ui, app),
    }
}
