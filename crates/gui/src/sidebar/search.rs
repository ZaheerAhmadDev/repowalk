//! Fuzzy search box + results + sortable symbol list. OWNER: RSK-16.
use eframe::egui;

use crate::app::{App, Selection, SortBy};
use repowalk_core::metrics;
use repowalk_core::search;

pub fn ui(ui: &mut egui::Ui, app: &mut App) {
    let Some(idx) = app.index.clone() else {
        return;
    };

    ui.heading("Search");
    let search_id = egui::Id::new("search-box");
    let box_resp = egui::TextEdit::singleline(&mut app.query)
        .id(search_id)
        .hint_text("fuzzy search — press / to focus")
        .desired_width(ui.available_width())
        .show(ui);
    if app.search_focus {
        app.search_focus = false;
        box_resp.response.request_focus();
    }

    if !app.query.trim().is_empty() {
        egui::ScrollArea::vertical().id_salt("search-results").show(ui, |ui| {
            for h in search::search(&idx, &app.query, 10) {
                let name = idx
                    .symbols
                    .iter()
                    .find(|s| s.id == h.target)
                    .map(|s| s.name.clone())
                    .unwrap_or_else(|| h.target.clone());
                let selected = app.selection.as_ref() == Some(&Selection::Symbol(h.target.clone()));
                let target = h.target.clone();
                let label = format!("{name} · {}:{}", h.path, h.line);
                if ui.selectable_label(selected, label).clicked() {
                    app.select(Selection::Symbol(target));
                }
            }
        });
        return;
    }

    ui.separator();
    ui.horizontal(|ui| {
        ui.weak("symbols:");
        egui::ComboBox::from_id_salt("symbol-sort")
            .selected_text(match app.sort {
                SortBy::FanIn => "fan-in",
                SortBy::Loc => "LOC",
                SortBy::Name => "name",
            })
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut app.sort, SortBy::FanIn, "fan-in");
                ui.selectable_value(&mut app.sort, SortBy::Loc, "LOC");
                ui.selectable_value(&mut app.sort, SortBy::Name, "name");
            });
    });

    // Sortable symbol list (plan §3.2 pick 5).
    let m = metrics::compute(&idx);
    let loc_of: std::collections::HashMap<&str, usize> = idx
        .symbols
        .iter()
        .map(|s| (s.file.as_str(), file_lines(&idx, &s.file)))
        .collect();
    let _ = &loc_of;

    let mut rows: Vec<(String, usize)> = idx
        .symbols
        .iter()
        .map(|s| {
            let fan = m
                .fan_in
                .iter()
                .find(|(id, _)| *id == s.id)
                .map(|(_, n)| *n)
                .unwrap_or(0);
            (s.id.clone(), fan)
        })
        .collect();
    match app.sort {
        SortBy::FanIn => rows.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0))),
        SortBy::Name => rows.sort_by_key(|(id, _)| symbol_name(&idx, id)),
        SortBy::Loc => rows.sort_by(|a, b| {
            file_lines(&idx, file_of(&a.0))
                .cmp(&file_lines(&idx, file_of(&b.0)))
                .then_with(|| a.0.cmp(&b.0))
        }),
    }

    egui::ScrollArea::vertical().id_salt("symbol-list").show(ui, |ui| {
        for (id, fan) in rows.iter().take(50) {
            let name = symbol_name(&idx, id);
            let selected = app.selection.as_ref() == Some(&Selection::Symbol(id.clone()));
            let idc = id.clone();
            let label = if *fan > 0 { format!("{name} ← {fan}") } else { name };
            if ui.selectable_label(selected, label).clicked() {
                app.select(Selection::Symbol(idc));
            }
        }
    });
}

fn symbol_name(idx: &repowalk_core::model::Index, id: &str) -> String {
    idx.symbols
        .iter()
        .find(|s| s.id == id)
        .map(|s| s.name.clone())
        .unwrap_or_else(|| id.to_string())
}

fn file_of(id: &str) -> &str {
    id.rsplit_once(':').map(|(f, _)| f).unwrap_or(id)
}

fn file_lines(idx: &repowalk_core::model::Index, path: &str) -> usize {
    idx.files
        .iter()
        .find(|f| f.path == path)
        .map(|f| f.lines)
        .unwrap_or(0)
}
