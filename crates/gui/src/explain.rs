//! Explain card: docstring, signature, file:line, callers/callees, clickable → sync. OWNER: RSK-21.
use eframe::egui;

use crate::app::{App, Selection};
use repowalk_core::model::{EdgeKind, Index};

pub fn card(ui: &mut egui::Ui, app: &mut App) {
    let Some(idx) = app.index.clone() else {
        return;
    };
    let Some(Selection::Symbol(id)) = app.selection.clone() else {
        ui.weak("select a symbol to see its explain card");
        return;
    };
    let Some(sym) = idx.symbols.iter().find(|s| s.id == id).cloned() else {
        ui.weak(format!("symbol not in index: {id}"));
        return;
    };

    ui.heading(format!("Explain · {}", sym.name));
    if !sym.doc.is_empty() {
        let summary = sym.doc.lines().next().unwrap_or_default();
        ui.label(summary.trim().to_string());
    }
    if !sym.signature.is_empty() {
        ui.monospace(sym.signature.clone());
    }
    ui.weak(format!("{}:{}", sym.file, sym.start_line));
    ui.weak(format!("module role: {}", App::module_of(&sym.file)));

    ui.horizontal_wrapped(|ui| {
        ui.label("called by:");
        callers_ui(ui, app, &idx, &sym.id);
    });
    ui.horizontal_wrapped(|ui| {
        ui.label("calls:");
        callees_ui(ui, app, &idx, &sym.id);
    });

    let nested: Vec<_> = idx
        .symbols
        .iter()
        .filter(|s| s.container.as_deref() == Some(sym.name.as_str()))
        .cloned()
        .collect();
    if !nested.is_empty() {
        ui.horizontal_wrapped(|ui| {
            ui.label("nested:");
            for n in nested {
                let id = n.id.clone();
                if ui
                    .link(format!("{} ({}:{})", n.name, n.file, n.start_line))
                    .clicked()
                {
                    app.select(Selection::Symbol(id));
                }
            }
        });
    }
}

fn callers_ui(ui: &mut egui::Ui, app: &mut App, idx: &Index, symbol_id: &str) {
    let mut callers: Vec<(String, usize)> = idx
        .edges
        .iter()
        .filter(|e| e.kind == EdgeKind::Call && e.callee == symbol_id)
        .map(|e| (e.caller.clone(), e.line))
        .collect();
    callers.sort();
    callers.dedup();
    if callers.is_empty() {
        ui.weak("none");
        return;
    }
    for (caller, line) in callers {
        // caller is a symbol id (path:line) or a bare file path.
        let (file, at) = match caller.rsplit_once(':') {
            Some((f, l)) => (f.to_string(), l.parse::<usize>().unwrap_or(0)),
            None => (caller.clone(), 0),
        };
        if ui.link(format!("{file}:{at}")).clicked() {
            // jump to the caller's definition line when resolvable, else the call site
            let target = idx
                .symbols
                .iter()
                .find(|s| s.id == caller)
                .map(|s| s.start_line)
                .unwrap_or(line);
            app.select(Selection::File(file));
            app.jump_line = Some(target);
        }
    }
}

fn callees_ui(ui: &mut egui::Ui, app: &mut App, idx: &Index, symbol_id: &str) {
    let mut callees: Vec<(String, usize)> = idx
        .edges
        .iter()
        .filter(|e| e.kind == EdgeKind::Call && e.caller == symbol_id)
        .map(|e| (e.callee.clone(), e.line))
        .collect();
    callees.sort();
    callees.dedup();
    if callees.is_empty() {
        ui.weak("none");
        return;
    }
    for (callee, _line) in callees {
        // resolved callee = symbol id; unresolved keeps the raw name
        match idx.symbols.iter().find(|s| s.id == callee) {
            Some(s) => {
                let id = s.id.clone();
                if ui
                    .link(format!("{} ({}:{})", s.name, s.file, s.start_line))
                    .clicked()
                {
                    app.select(Selection::Symbol(id));
                }
            }
            None => {
                ui.weak(format!("{callee} ?"));
            }
        }
    }
}
