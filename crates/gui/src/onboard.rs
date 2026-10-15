//! "Repo 101" onboarding census (v2 F1). OWNER: DSN-4.
//!
//! Shown after `learn()` completes: what the codebase is (language mix, LOC,
//! symbols), how it is shaped (module tree), where the risk is (hotspots, unused
//! variables, cycles) and where to start reading (entrypoints first). Read the
//! cached `Index` + `Analysis`; never re-analyze here.
use eframe::egui;

use crate::ui_kit;

pub fn ui(ui: &mut egui::Ui, app: &mut crate::app::App) {
    let detail = match (&app.index, &app.analysis) {
        (Some(idx), Some(a)) => format!(
            "{} file(s) · {} symbol(s) · {} variable(s) · {} cfg(s) · {} type(s) · {} entry point(s)",
            idx.files.len(),
            idx.symbols.len(),
            a.variables.len(),
            a.cfgs.len(),
            a.types.len(),
            idx.files.iter().filter(|f| f.is_entrypoint).count()
        ),
        (Some(idx), None) => format!("{} file(s) indexed; analysis still running", idx.files.len()),
        _ => "no repository loaded".to_string(),
    };
    ui_kit::not_ready(ui, "Repo 101", "DSN-4", &detail);
}
