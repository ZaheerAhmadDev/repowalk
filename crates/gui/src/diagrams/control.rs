//! Control-flow diagram view (CFG per function). OWNER: GRAPH-2.
//!
//! Contract to render (frozen in `crates/core/src/analysis.rs`): one
//! `FunctionCfg` per function with `CfgNode { kind, label, line }` and
//! `CfgEdge { kind, from, to }`; `CfgEdgeKind::Back` marks loop back-edges and
//! `cyclomatic`/`max_nesting` are already computed — never count decisions here.
use eframe::egui;

use crate::app::{App, Selection};
use crate::ui_kit;

pub fn ui(ui: &mut egui::Ui, app: &mut App) {
    let detail = match &app.analysis {
        None => "no analysis yet — press Learn (or run `repowalk learn`) first".to_string(),
        Some(a) => {
            let selection = match &app.selection {
                Some(Selection::Symbol(id)) => format!(" · selected {id}"),
                _ => " · select a function to open its CFG".to_string(),
            };
            format!(
                "analysis ready: {} function CFG(s){selection}",
                a.cfgs.len()
            )
        }
    };
    ui_kit::not_ready(ui, "Control-flow diagrams", "GRAPH-2", &detail);
}
