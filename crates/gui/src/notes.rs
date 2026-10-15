//! Notes panel (v2 F11): pin a reading note to a file or symbol. OWNER: TEXT-4.
//!
//! Notes are local to the repo (no network, no LLM); the store lands with this
//! card. Until then this panel states the situation instead of rendering an
//! empty list that looks like "no notes exist".
use eframe::egui;

use crate::ui_kit;

pub fn ui(ui: &mut egui::Ui, app: &mut crate::app::App) {
    let detail = match &app.index {
        None => "no repository loaded".to_string(),
        Some(idx) => format!(
            "{} file(s) can be annotated once the note store lands",
            idx.files.len()
        ),
    };
    ui_kit::not_ready(ui, "Notes", "TEXT-4", &detail);
}
