//! Repo map + metrics strip + health report panel. OWNER: RSK-15.
use eframe::egui;

use crate::app::{App, Selection};
use crate::sidebar::{data_row, Meta};
use crate::theme;
use repowalk_core::metrics::Metrics;
use repowalk_core::model::Index;

/// Rows shown per group; the rest collapses to a "+N more" hint.
const ROWS_PER_GROUP: usize = 5;
/// `unresolved_pct` at/above which the health dot goes red.
const UNRESOLVED_WARN: f64 = 0.05;

pub fn ui(ui: &mut egui::Ui, app: &mut App) {
    let Some(idx) = app.index.clone() else {
        return;
    };
    // Cached once per index (poll_progress) — Tarjan SCC every sidebar
    // repaint is not a per-frame cost.
    let Some(m) = app.cached_metrics.clone() else {
        return;
    };

    theme::section_header(ui, "MAP");
    let cap = (ui.available_height() * 0.5).clamp(theme::ROW_HEIGHT * 4.0, 360.0);
    egui::ScrollArea::vertical()
        .id_salt("sidebar-map")
        .max_height(cap)
        .show(ui, |ui| {
            overview(ui, app, &m);
            theme::separator(ui);
            theme::section_header(ui, "Top files");
            top_files(ui, app, &m);
            theme::separator(ui);
            theme::section_header(ui, "Hottest symbols");
            hottest(ui, app, &m);
            theme::separator(ui);
            health(ui, app, &idx, &m);
        });
}

/// LOC callout + language mix + entrypoint links: the "what is this?" block.
fn overview(ui: &mut egui::Ui, app: &mut App, m: &Metrics) {
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(m.total_loc.to_string())
                .size(theme::FONT_HEADING)
                .strong()
                .color(theme::TEXT_PRIMARY),
        );
        ui.label(
            egui::RichText::new("LOC")
                .size(theme::FONT_SMALL)
                .color(theme::TEXT_MUTED),
        );
    });

    if !m.langs.is_empty() {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = theme::SPACE_2;
            for (lang, n) in &m.langs {
                ui.label(
                    egui::RichText::new(format!("{lang}: {n}"))
                        .size(theme::FONT_SMALL)
                        .color(theme::TEXT_SECONDARY),
                );
            }
        });
    }

    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = theme::SPACE_2;
        ui.label(
            egui::RichText::new("Entrypoints")
                .size(theme::FONT_SMALL)
                .strong()
                .color(theme::TEXT_MUTED),
        );
        if m.entrypoints.is_empty() {
            ui.label(
                egui::RichText::new("none")
                    .size(theme::FONT_SMALL)
                    .color(theme::TEXT_DISABLED),
            );
        }
        for e in &m.entrypoints {
            let path = e.clone();
            if ui
                .link(egui::RichText::new(e).size(theme::FONT_SMALL))
                .clicked()
            {
                app.select(Selection::File(path));
            }
        }
    });
}

/// Largest files, clickable into the editor (LOC right-aligned).
fn top_files(ui: &mut egui::Ui, app: &mut App, m: &Metrics) {
    if m.top_files.is_empty() {
        muted(ui, "no files indexed");
        return;
    }
    for (i, (path, lines)) in m.top_files.iter().take(ROWS_PER_GROUP).enumerate() {
        let selected = app.open_file.as_deref() == Some(path.as_str());
        let loc = format!("{lines} LOC");
        let resp = data_row(
            ui,
            selected,
            ("map-top-file", i),
            short_path(path),
            Some(Meta::muted(&loc)),
        );
        if resp.clicked() {
            let path = path.clone();
            app.select(Selection::File(path));
        }
        resp.on_hover_text(path.as_str());
    }
    more(ui, m.top_files.len(), ROWS_PER_GROUP, "larger files");
}

/// Highest fan-in symbols (call-count hotspots), clickable into the editor.
fn hottest(ui: &mut egui::Ui, app: &mut App, m: &Metrics) {
    if m.fan_in.is_empty() {
        muted(ui, "no call edges");
        return;
    }
    for (i, (sym, n)) in m.fan_in.iter().take(ROWS_PER_GROUP).enumerate() {
        let id = sym.clone();
        let selected = app.selection.as_ref() == Some(&Selection::Symbol(id.clone()));
        let callers = format!("{n} caller{}", if *n == 1 { "" } else { "s" });
        let resp = data_row(
            ui,
            selected,
            ("map-hottest", i),
            base_name(sym),
            Some(Meta::muted(&callers)),
        );
        if resp.clicked() {
            app.select(Selection::Symbol(id));
        }
    }
    more(ui, m.fan_in.len(), ROWS_PER_GROUP, "symbols");
}

/// Health report: status dot + cycles ("spaghetti zones") + god-files (plan §7.4).
fn health(ui: &mut egui::Ui, app: &mut App, idx: &Index, m: &Metrics) {
    theme::section_header(ui, "Health");
    let issues =
        m.cycles.len() + m.god_files.len() + usize::from(m.unresolved_pct >= UNRESOLVED_WARN);
    let ok = issues == 0;

    ui.horizontal(|ui| {
        let (dot, _) = ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
        ui.painter().circle_filled(
            dot.center(),
            4.0,
            if ok { theme::SUCCESS } else { theme::DANGER },
        );
        let headline = if ok {
            "healthy".to_string()
        } else {
            format!("{issues} issue{}", if issues == 1 { "" } else { "s" })
        };
        ui.label(
            egui::RichText::new(headline)
                .size(theme::FONT_SMALL)
                .strong()
                .color(if ok { theme::SUCCESS } else { theme::DANGER }),
        );
    });
    ui.add(
        egui::Label::new(
            egui::RichText::new(format!(
                "{} cycles · {} god-files · {:.0}% unresolved",
                m.cycles.len(),
                m.god_files.len(),
                m.unresolved_pct * 100.0
            ))
            .size(theme::FONT_SMALL)
            .color(theme::TEXT_MUTED),
        )
        .truncate(),
    );

    for (i, cycle) in m.cycles.iter().take(ROWS_PER_GROUP).enumerate() {
        let Some(target) = cycle.first().cloned() else {
            continue;
        };
        let label = cycle
            .iter()
            .map(|s| base_name(s))
            .collect::<Vec<_>>()
            .join(" ↔ ");
        let size = format!("{} symbols", cycle.len());
        let resp = data_row(
            ui,
            false,
            ("map-cycle", i),
            egui::RichText::new(label).color(theme::DANGER),
            Some(Meta::tinted(&size, theme::DANGER)),
        );
        if resp.clicked() {
            app.select(Selection::Symbol(target));
        }
        resp.on_hover_text(cycle.join(" ↔ "));
    }
    more(ui, m.cycles.len(), ROWS_PER_GROUP, "cycles");

    for (i, god) in m.god_files.iter().take(ROWS_PER_GROUP).enumerate() {
        let loc = format!("{} LOC", file_lines(idx, god));
        let selected = app.open_file.as_deref() == Some(god.as_str());
        let resp = data_row(
            ui,
            selected,
            ("map-god-file", i),
            short_path(god),
            Some(Meta::tinted(&loc, theme::WARNING)),
        );
        if resp.clicked() {
            let path = god.clone();
            app.select(Selection::File(path));
        }
    }
    more(ui, m.god_files.len(), ROWS_PER_GROUP, "god-files");
}

// ── Small helpers ───────────────────────────────────────────────────────────

fn muted(ui: &mut egui::Ui, text: &str) {
    ui.label(
        egui::RichText::new(text)
            .size(theme::FONT_SMALL)
            .color(theme::TEXT_MUTED),
    );
}

/// "+N more …" hint when a group is truncated (progressive disclosure).
fn more(ui: &mut egui::Ui, total: usize, shown: usize, what: &str) {
    if total > shown {
        muted(ui, &format!("+{} more {what}", total - shown));
    }
}

/// `crates/gui/src/sidebar/mod.rs` → `sidebar/mod.rs` (last two segments).
fn short_path(path: &str) -> String {
    let mut parts: Vec<&str> = path.rsplit('/').take(2).collect();
    parts.reverse();
    parts.join("/")
}

/// `src/main.rs:42` → `main.rs:42` (symbol ids keep their line number).
fn base_name(id: &str) -> String {
    match id.rsplit_once(':') {
        Some((file, line)) => format!("{}:{line}", short_path(file)),
        None => short_path(id),
    }
}

fn file_lines(idx: &Index, path: &str) -> usize {
    idx.files
        .iter()
        .find(|f| f.path == path)
        .map(|f| f.lines)
        .unwrap_or(0)
}
