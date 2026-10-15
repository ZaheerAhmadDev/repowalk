//! Explain card (inspector strip): docstring, signature, file:line, callers/callees,
//! nested items — every link drives the shared `Selection`. OWNER: RSK-21.
use eframe::egui;

use crate::app::{App, Selection};
use crate::theme;
use repowalk_core::model::{EdgeKind, Index, SymbolKind};

/// Width of the fixed label column in the two-column sections.
const LABEL_WIDTH: f32 = 80.0;

pub fn card(ui: &mut egui::Ui, app: &mut App) {
    egui::ScrollArea::vertical()
        .id_salt("explain-card")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.add_space(theme::SPACE_2);
            theme::section_header(ui, "EXPLAIN");

            let Some(idx) = app.index.clone() else {
                return;
            };
            let Some(Selection::Symbol(id)) = app.selection.clone() else {
                hint(
                    ui,
                    "Select a symbol — in the graph, search, or through a link below — \
                     to see its doc, signature, and callers.",
                );
                return;
            };
            let Some(sym) = idx.symbols.iter().find(|s| s.id == id).cloned() else {
                hint(ui, &format!("Symbol not in index: {id}"));
                return;
            };

            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(&sym.name)
                        .size(theme::FONT_HEADING)
                        .strong()
                        .color(theme::TEXT_PRIMARY),
                );
                ui.label(
                    egui::RichText::new(kind_label(sym.kind))
                        .size(theme::FONT_SMALL)
                        .color(theme::TEXT_MUTED),
                );
            });

            if !sym.doc.is_empty() {
                let summary = sym
                    .doc
                    .lines()
                    .next()
                    .unwrap_or_default()
                    .trim()
                    .to_string();
                if !summary.is_empty() {
                    ui.add_space(theme::SPACE_1);
                    ui.add(
                        egui::Label::new(egui::RichText::new(summary).color(theme::TEXT_SECONDARY))
                            .wrap(),
                    );
                }
            }

            if !sym.signature.is_empty() {
                ui.add_space(theme::SPACE_2);
                egui::Frame::new()
                    .fill(theme::SURFACE_E2)
                    .corner_radius(egui::CornerRadius::same(theme::CORNER_SM))
                    .inner_margin(egui::Margin::symmetric(
                        theme::SPACE_2 as i8,
                        theme::SPACE_1 as i8,
                    ))
                    .show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(sym.signature.trim())
                                    .monospace()
                                    .color(theme::TEXT_SECONDARY),
                            )
                            .wrap(),
                        );
                    });
            }

            ui.add_space(theme::SPACE_2);
            ui.horizontal(|ui| {
                let file = sym.file.clone();
                let line = sym.start_line;
                let loc = format!("{file}:{line}");
                if ui
                    .link(egui::RichText::new(loc).monospace().color(theme::ACCENT))
                    .clicked()
                {
                    app.select(Selection::File(file));
                    app.jump_line = Some(line);
                }
                ui.label(
                    egui::RichText::new(format!("· module {}", App::module_of(&sym.file)))
                        .size(theme::FONT_SMALL)
                        .color(theme::TEXT_MUTED),
                );
            });

            section(ui, "called by", |ui| callers(ui, app, &idx, &sym.id));
            section(ui, "calls", |ui| callees(ui, app, &idx, &sym.id));

            let nested: Vec<_> = idx
                .symbols
                .iter()
                .filter(|s| s.container.as_deref() == Some(sym.name.as_str()))
                .cloned()
                .collect();
            if !nested.is_empty() {
                section(ui, "nested", |ui| {
                    for n in nested {
                        if ui
                            .link(format!("{} ({}:{})", n.name, n.file, n.start_line))
                            .clicked()
                        {
                            app.select(Selection::Symbol(n.id));
                        }
                    }
                });
            }

            ui.add_space(theme::SPACE_3);
        });
}

fn hint(ui: &mut egui::Ui, text: &str) {
    ui.label(
        egui::RichText::new(text)
            .size(theme::FONT_SMALL)
            .color(theme::TEXT_MUTED),
    );
}

/// Two-column row: muted small label, then content that wraps in the rest.
fn section(ui: &mut egui::Ui, label: &str, add_contents: impl FnOnce(&mut egui::Ui)) {
    ui.add_space(theme::SPACE_2);
    ui.horizontal(|ui| {
        ui.allocate_ui_with_layout(
            egui::vec2(LABEL_WIDTH, ui.spacing().interact_size.y),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.label(
                    egui::RichText::new(label)
                        .size(theme::FONT_SMALL)
                        .strong()
                        .color(theme::TEXT_MUTED),
                );
            },
        );
        ui.horizontal_wrapped(add_contents);
    });
}

fn callers(ui: &mut egui::Ui, app: &mut App, idx: &Index, symbol_id: &str) {
    let mut callers: Vec<(String, usize)> = idx
        .edges
        .iter()
        .filter(|e| e.kind == EdgeKind::Call && e.callee == symbol_id)
        .map(|e| (e.caller.clone(), e.line))
        .collect();
    callers.sort();
    callers.dedup();

    if callers.is_empty() {
        none(ui);
        return;
    }

    for (caller, line) in callers {
        // caller is a symbol id (`path:line`) or a bare file path.
        let resolved = idx.symbols.iter().find(|s| s.id == caller);
        let label = match resolved {
            Some(s) => format!("{} ({}:{})", s.name, s.file, s.start_line),
            None => caller.clone(),
        };
        if ui.link(label).clicked() {
            // Jump to the definition when resolvable, else to the call site.
            let (file, target) = match resolved {
                Some(s) => (s.file.clone(), s.start_line),
                None => (
                    caller
                        .rsplit_once(':')
                        .map(|(f, _)| f.to_string())
                        .unwrap_or_else(|| caller.clone()),
                    line,
                ),
            };
            app.select(Selection::File(file));
            app.jump_line = Some(target);
        }
    }
}

fn callees(ui: &mut egui::Ui, app: &mut App, idx: &Index, symbol_id: &str) {
    let mut callees: Vec<(String, usize)> = idx
        .edges
        .iter()
        .filter(|e| e.kind == EdgeKind::Call && e.caller == symbol_id)
        .map(|e| (e.callee.clone(), e.line))
        .collect();
    callees.sort();
    callees.dedup();

    if callees.is_empty() {
        none(ui);
        return;
    }

    for (callee, _line) in callees {
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
                // Unresolved: muted raw name + danger-tinted `?` marker.
                ui.label(
                    egui::RichText::new(callee)
                        .size(theme::FONT_SMALL)
                        .color(theme::TEXT_MUTED),
                );
                ui.colored_label(
                    theme::DANGER,
                    egui::RichText::new("?").strong().size(theme::FONT_SMALL),
                )
                .on_hover_text("unresolved call — not in the index");
            }
        }
    }
}

fn none(ui: &mut egui::Ui) {
    ui.label(
        egui::RichText::new("none")
            .size(theme::FONT_SMALL)
            .color(theme::TEXT_DISABLED),
    );
}

fn kind_label(kind: SymbolKind) -> &'static str {
    match kind {
        SymbolKind::Function => "fn",
        SymbolKind::Method => "method",
        SymbolKind::Struct => "struct",
        SymbolKind::Enum => "enum",
        SymbolKind::Trait => "trait",
        SymbolKind::Mod => "mod",
        SymbolKind::Class => "class",
    }
}
