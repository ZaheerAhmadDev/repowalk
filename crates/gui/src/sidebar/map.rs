//! Repo map + metrics strip + health report panel. OWNER: RSK-15.
use eframe::egui;

use crate::app::{App, Selection};
use repowalk_core::metrics;

pub fn ui(ui: &mut egui::Ui, app: &mut App) {
    let Some(idx) = app.index.clone() else {
        return;
    };
    let m = metrics::compute(&idx);

    ui.heading("Map");
    egui::ScrollArea::vertical().id_salt("sidebar-map").show(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            for (lang, n) in &m.langs {
                ui.label(format!("{lang}: {n}"));
            }
        });
        ui.label(format!("{} LOC", m.total_loc));
        ui.horizontal_wrapped(|ui| {
            ui.weak("entrypoints:");
            for e in &m.entrypoints {
                let path = e.clone();
                if ui.link(format!("⟶ {e}")).clicked() {
                    app.select(Selection::File(path));
                }
            }
            if m.entrypoints.is_empty() {
                ui.weak("none");
            }
        });

        ui.separator();
        ui.strong("Top files");
        for (path, lines) in m.top_files.iter().take(5) {
            let p = path.clone();
            if ui.selectable_label(app.open_file.as_deref() == Some(path.as_str()), format!("{path} · {lines}")).clicked() {
                app.select(Selection::File(p));
            }
        }

        ui.separator();
        ui.strong("Hottest symbols");
        if m.fan_in.is_empty() {
            ui.weak("no call edges");
        }
        for (sym, n) in m.fan_in.iter().take(5) {
            let id = sym.clone();
            if ui.selectable_label(app.selection.as_ref() == Some(&Selection::Symbol(id.clone())), format!("{sym} ← {n}")).clicked() {
                app.select(Selection::Symbol(id));
            }
        }

        ui.separator();
        health(ui, app, &m);
    });
}

/// Health report: cycles ("spaghetti zones"), god-files, unresolved % (plan §7.4).
fn health(ui: &mut egui::Ui, app: &mut App, m: &metrics::Metrics) {
    ui.strong("Health");
    let ok = m.cycles.is_empty() && m.god_files.is_empty() && m.unresolved_pct < 0.05;
    let dot = if ok {
        egui::Color32::from_rgb(152, 195, 121)
    } else {
        egui::Color32::from_rgb(224, 108, 117)
    };
    ui.horizontal(|ui| {
        ui.colored_label(dot, "●");
        ui.label(format!(
            "{} cycles · {} god-files · {:.0}% unresolved",
            m.cycles.len(),
            m.god_files.len(),
            m.unresolved_pct * 100.0
        ));
    });
    for cycle in &m.cycles {
        ui.colored_label(
            egui::Color32::from_rgb(224, 108, 117),
            format!("spaghetti: {}", cycle.join(" ↔ ")),
        );
        let ids: Vec<Selection> = cycle.iter().map(|s| Selection::Symbol(s.clone())).collect();
        let _ = ids;
    }
    for god in &m.god_files {
        let p = god.clone();
        if ui.link(format!("god-file: {god}")).clicked() {
            app.select(Selection::File(p));
        }
    }
}
