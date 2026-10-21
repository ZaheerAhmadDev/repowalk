//! Compact Flow panel: A→B pickers mirroring the graph trace picker, a
//! transport mirror, and the current-step readout. OWNER: TL-GRAPH (GRAPH-1).
use eframe::egui;
use repowalk_core::model::Index;

use crate::app::{App, TracePick};
use crate::flow::{self, playback, render};
use crate::sidebar::{data_row, Meta};
use crate::theme;

/// Fuzzy hits offered while an endpoint slot is armed.
const MAX_PICK_HITS: usize = 8;

pub fn ui(ui: &mut egui::Ui, app: &mut App) {
    let Some(idx) = app.index.clone() else {
        return;
    };
    theme::section_header(ui, "Flow");

    // A / B rows — click to arm, then pick from the hits below (graph mode's
    // equivalent picks nodes on the canvas).
    pick_row(ui, app, &idx, "A", TracePick::PickA);
    pick_row(ui, app, &idx, "B", TracePick::PickB);
    if !matches!(app.flow.pick, TracePick::None) {
        pick_hits(ui, app, &idx);
    }

    ui.add_space(theme::SPACE_2);

    // Transport mirror + current-step readout.
    let seq = flow::sequence(&idx, &app.flow);
    let steps = seq.step_count();
    let (step, _) = playback::step_pos(app.flow.playback.playhead, steps);
    ui.horizontal(|ui| {
        let glyph = if app.flow.playback.playing {
            "⏸"
        } else {
            "▶"
        };
        let play = ui
            .add_enabled(steps > 0, egui::Button::new(glyph).frame(false).small())
            .on_hover_text("Play / pause the flow");
        if play.clicked() {
            app.flow.playback.toggle_play(steps);
        }
        let text = if steps > 0 {
            let detail = seq.default_detail(step).unwrap_or_default();
            format!(
                "{}/{} · {}",
                step + 1,
                steps,
                render::display_name(&idx, &detail)
            )
        } else {
            "no steps — pick A & B".to_string()
        };
        ui.label(
            egui::RichText::new(text)
                .size(theme::FONT_SMALL)
                .color(theme::TEXT_SECONDARY),
        );
    });
}

/// One endpoint row: current pick (or the "pick…" nudge), click to arm.
fn pick_row(ui: &mut egui::Ui, app: &mut App, idx: &Index, slot: &str, pick: TracePick) {
    let armed = app.flow.pick == pick;
    let value = if slot == "A" {
        app.flow.trace_a.clone()
    } else {
        app.flow.trace_b.clone()
    };
    let (name, meta) = if value.is_empty() {
        (
            egui::RichText::new(format!("pick {slot}…")).color(theme::TEXT_MUTED),
            None,
        )
    } else {
        (
            egui::RichText::new(render::display_name(idx, &value)).strong(),
            Some(Meta::muted(render::file_of(&value))),
        )
    };
    let resp = data_row(ui, armed, ("flow-endpoint", slot), name, meta);
    if resp.clicked() {
        app.flow.pick = if armed { TracePick::None } else { pick };
        app.flow.pick_query.clear();
    }
}

/// Mini fuzzy picker shown while a slot is armed; a hit fills the slot.
fn pick_hits(ui: &mut egui::Ui, app: &mut App, idx: &Index) {
    let slot = match app.flow.pick {
        TracePick::PickA => "A",
        TracePick::PickB => "B",
        TracePick::None => return,
    };
    egui::TextEdit::singleline(&mut app.flow.pick_query)
        .id(egui::Id::new(("flow-pick-box", slot)))
        .hint_text(format!("fuzzy search symbols for {slot}…"))
        .desired_width(ui.available_width())
        .show(ui);

    let hits = flow::picker_hits(idx, &app.flow.pick_query, MAX_PICK_HITS);
    if hits.is_empty() {
        ui.label(
            egui::RichText::new(if app.flow.pick_query.trim().is_empty() {
                "no functions in this repository"
            } else {
                "no symbols match"
            })
            .size(theme::FONT_SMALL)
            .color(theme::TEXT_MUTED),
        );
        return;
    }
    for (target, path) in &hits {
        let name = idx
            .symbols
            .iter()
            .find(|s| s.id == *target)
            .map(|s| s.name.clone())
            .unwrap_or_else(|| target.clone());
        let resp = data_row(
            ui,
            false,
            ("flow-pick-hit", target),
            name,
            Some(Meta::muted(path)),
        );
        if resp.clicked() {
            crate::flow::assign_endpoint(app, target.clone());
        }
    }
}
