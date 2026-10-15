//! Repository health panel (v2 F6). OWNER: DSN-3.
//!
//! v1 `metrics::compute` already answers files/LOC/cycles; the v2 aggregates
//! (complexity per module, god files, hotspot roll-ups) come from
//! `metrics.rs`/`complexity.rs` — read them, do not recompute in the GUI.
use eframe::egui;

use crate::ui_kit;

pub fn ui(ui: &mut egui::Ui, app: &mut crate::app::App) {
    let detail = match &app.index {
        None => "no repository loaded".to_string(),
        Some(idx) => {
            let m = repowalk_core::metrics::compute(idx);
            format!(
                "v1 metrics: {} file(s) · {} LOC · {} import cycle(s)",
                idx.files.len(),
                m.total_loc,
                m.cycles.len()
            )
        }
    };
    ui_kit::not_ready(ui, "Repository health", "DSN-3", &detail);
}
