//! Fuzzy search box + results + sortable symbol list. OWNER: RSK-16.
use eframe::egui;

use crate::app::{App, Selection, SortBy};
use crate::sidebar::{data_row, Meta};
use crate::theme;
use repowalk_core::metrics;
use repowalk_core::model::Index;
use repowalk_core::search;

/// Fuzzy hits shown while a query is typed.
const MAX_HITS: usize = 20;
/// Symbol rows shown in the browsable list.
const MAX_SYMBOLS: usize = 50;

pub fn ui(ui: &mut egui::Ui, app: &mut App) {
    let Some(idx) = app.index.clone() else {
        return;
    };

    theme::section_header(ui, "Search");
    let box_resp = egui::TextEdit::singleline(&mut app.query)
        .id(egui::Id::new("search-box"))
        .hint_text("fuzzy search — press / to focus")
        .desired_width(ui.available_width())
        .show(ui);
    if app.search_focus {
        app.search_focus = false;
        box_resp.response.request_focus();
    }

    if app.query.trim().is_empty() {
        symbols(ui, app, &idx);
    } else {
        results(ui, app, &idx);
    }
}

/// Fuzzy hits: bold name left, muted `path:line` right.
fn results(ui: &mut egui::Ui, app: &mut App, idx: &Index) {
    let hits = search::search(idx, &app.query, MAX_HITS);
    if hits.is_empty() {
        ui.add_space(theme::SPACE_2);
        muted(ui, &format!("no matches for “{}”", app.query.trim()));
        return;
    }

    let cap = ui.available_height().max(theme::ROW_HEIGHT * 3.0);
    egui::ScrollArea::vertical()
        .id_salt("search-results")
        .max_height(cap)
        .show(ui, |ui| {
            for (i, hit) in hits.iter().enumerate() {
                let sym = idx.symbols.iter().find(|s| s.id == hit.target);
                let (name, sel, loc) = match &sym {
                    Some(s) => (
                        egui::RichText::new(&s.name).strong(),
                        Selection::Symbol(hit.target.clone()),
                        Some(format!("{}:{}", hit.path, hit.line)),
                    ),
                    None => (
                        egui::RichText::new(&hit.path).strong(),
                        Selection::File(hit.target.clone()),
                        None,
                    ),
                };
                let selected = app.selection.as_ref() == Some(&sel) || i == app.list_cursor;
                let meta = loc.as_deref().map(Meta::muted);
                let resp = data_row(ui, selected, ("search-hit", i), name, meta);
                if resp.clicked() {
                    app.select(sel);
                }
            }
        });
}

/// Symbol browser: sort control on top, fan-in badges on the right. The j/k
/// cursor row (`app.list_cursor`) stays highlighted like the selection.
fn symbols(ui: &mut egui::Ui, app: &mut App, idx: &Index) {
    theme::section_header(ui, "Symbols");
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(format!("{} symbols", idx.symbols.len()))
                .size(theme::FONT_SMALL)
                .color(theme::TEXT_MUTED),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let label = match app.sort {
                SortBy::FanIn => "fan-in",
                SortBy::Loc => "LOC",
                SortBy::Name => "name",
            };
            egui::ComboBox::from_id_salt("symbol-sort")
                .selected_text(
                    egui::RichText::new(label)
                        .size(theme::FONT_SMALL)
                        .color(theme::TEXT_SECONDARY),
                )
                .width(88.0)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut app.sort, SortBy::FanIn, "fan-in");
                    ui.selectable_value(&mut app.sort, SortBy::Loc, "LOC");
                    ui.selectable_value(&mut app.sort, SortBy::Name, "name");
                });
        });
    });

    let m = metrics::compute(idx);
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
        SortBy::Name => rows.sort_by_key(|(id, _)| symbol_name(idx, id)),
        SortBy::Loc => rows.sort_by(|a, b| {
            file_lines(idx, file_of(&a.0))
                .cmp(&file_lines(idx, file_of(&b.0)))
                .then_with(|| a.0.cmp(&b.0))
        }),
    }

    if rows.is_empty() {
        muted(ui, "no symbols indexed");
        return;
    }

    let cap = ui.available_height().max(theme::ROW_HEIGHT * 3.0);
    egui::ScrollArea::vertical()
        .id_salt("symbol-list")
        .max_height(cap)
        .show(ui, |ui| {
            for (i, (id, fan)) in rows.iter().take(MAX_SYMBOLS).enumerate() {
                let name = symbol_name(idx, id);
                let selected = app.selection.as_ref() == Some(&Selection::Symbol(id.clone()))
                    || i == app.list_cursor;
                let fan_text = fan.to_string();
                let meta = if *fan > 0 {
                    Meta::badge(&fan_text)
                } else {
                    Meta::muted("0")
                };
                let resp = data_row(ui, selected, ("symbol-row", i), name, Some(meta));
                if resp.clicked() {
                    let id = id.clone();
                    app.select(Selection::Symbol(id));
                }
            }
        });
}

// ── Symbol-list helpers ─────────────────────────────────────────────────────

fn muted(ui: &mut egui::Ui, text: &str) {
    ui.label(
        egui::RichText::new(text)
            .size(theme::FONT_SMALL)
            .color(theme::TEXT_MUTED),
    );
}

fn symbol_name(idx: &Index, id: &str) -> String {
    idx.symbols
        .iter()
        .find(|s| s.id == id)
        .map(|s| s.name.clone())
        .unwrap_or_else(|| id.to_string())
}

fn file_of(id: &str) -> &str {
    id.rsplit_once(':').map(|(f, _)| f).unwrap_or(id)
}

fn file_lines(idx: &Index, path: &str) -> usize {
    idx.files
        .iter()
        .find(|f| f.path == path)
        .map(|f| f.lines)
        .unwrap_or(0)
}
