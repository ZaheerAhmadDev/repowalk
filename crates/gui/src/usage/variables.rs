//! Per-file variable table: kind, type, read/write/use counts, decl line.
//! OWNER: TEXT-2.
//!
//! Data contract: `analysis.variables` (`Variable { id, name, kind, type_name,
//! file, line, reads, writes, first_use, last_use, enclosing }`). A declaration
//! counts as one write in every language; `reads == 0` marks a never-read local.
use eframe::egui;

use crate::ui_kit;

pub fn ui(ui: &mut egui::Ui, app: &mut crate::app::App) {
    let detail = match &app.analysis {
        None => "no analysis yet — press Learn first".to_string(),
        Some(a) => {
            let unused = a.variables.iter().filter(|v| v.reads == 0).count();
            format!(
                "analysis ready: {} variable(s), {unused} never read",
                a.variables.len()
            )
        }
    };
    ui_kit::not_ready(ui, "Variables", "TEXT-2", &detail);
}
