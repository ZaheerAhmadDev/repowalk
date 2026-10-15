//! References panel: every use site of the selected symbol, grouped by file.
//! OWNER: TEXT-3.
//!
//! Use `repowalk_core::refs::references_ex` so resolved hits and name-only hits
//! are labelled differently (risk R-20: never inflate confidence). Jump-to goes
//! through `app.select(Selection::Symbol(..))` so one selection drives every pane.
use eframe::egui;

use crate::app::Selection;
use crate::ui_kit;

pub fn ui(ui: &mut egui::Ui, app: &mut crate::app::App) {
    let detail = match &app.analysis {
        None => "no analysis yet — press Learn first".to_string(),
        Some(a) => match &app.selection {
            Some(Selection::Symbol(id)) => {
                let hits = repowalk_core::refs::references(a, id);
                format!(
                    "{} reference(s) to {id} · {} use site(s) in total",
                    hits.len(),
                    a.uses.len()
                )
            }
            _ => format!(
                "{} use site(s) in the analysis — select a symbol to list its uses",
                a.uses.len()
            ),
        },
    };
    ui_kit::not_ready(ui, "References", "TEXT-3", &detail);
}
