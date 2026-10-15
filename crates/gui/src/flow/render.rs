//! Flow visuals: transport row, stage strip with pulsing connectors, wave
//! rows, and the per-step detail panel. Animation only ever adds emphasis —
//! every card and row is fully legible while paused (WCAG 2.3.3, DSN-1).
//! OWNER: TL-GRAPH (GRAPH-1).
use eframe::egui;
use repowalk_core::model::{Confidence, Index, SymbolKind};

use crate::app::App;
use crate::flow::{playback, FlowState};
use crate::theme;

/// Stage-card geometry (px). Fixed so the strip reads as a storyboard.
const CARD_W: f32 = 148.0;
const CARD_H: f32 = 54.0;
/// Connector gap between two cards — where the pulse travels.
const LINK_W: f32 = 30.0;

// ── Transport ───────────────────────────────────────────────────────────────

/// Play/pause, step fwd/back, speed, loop and scrub — one fixed row above the
/// canvas, disabled while the sequence is empty.
pub fn transport(ui: &mut egui::Ui, flow: &mut FlowState, steps: usize) {
    ui.horizontal(|ui| {
        let play = if flow.playback.playing {
            "⏸ pause"
        } else {
            "▶ play"
        };
        if ui
            .add_enabled(steps > 0, egui::Button::new(play))
            .on_hover_text("Play / pause the flow (plays from the scrub position)")
            .clicked()
        {
            flow.playback.toggle_play(steps);
        }
        if ui
            .add_enabled(steps > 0, egui::Button::new("⏮"))
            .on_hover_text("Step back")
            .clicked()
        {
            flow.playback.step_back(steps);
        }
        if ui
            .add_enabled(steps > 0, egui::Button::new("⏭"))
            .on_hover_text("Step forward")
            .clicked()
        {
            flow.playback.step_forward(steps);
        }
        if ui
            .button(format!("{}x", flow.playback.speed))
            .on_hover_text("Playback speed (click to cycle)")
            .clicked()
        {
            flow.playback.cycle_speed();
        }
        ui.toggle_value(&mut flow.playback.looping, "loop")
            .on_hover_text("Restart from the beginning after the last step");

        // Scrub: a [0,1] slider mapped onto the sequence duration.
        let total = playback::total_secs(steps);
        let mut frac = if total > 0.0 {
            (flow.playback.playhead / total).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let scrub = ui
            .add_enabled(
                steps > 0,
                egui::Slider::new(&mut frac, 0.0..=1.0).show_value(false),
            )
            .on_hover_text("Scrub the playhead");
        if scrub.changed() {
            flow.playback.playhead = frac * total;
        }

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let (step, _) = playback::step_pos(flow.playback.playhead, steps);
            let text = if steps > 0 {
                format!("step {}/{}", step + 1, steps)
            } else {
                "no steps".to_string()
            };
            ui.label(
                egui::RichText::new(text)
                    .size(theme::FONT_SMALL)
                    .color(theme::TEXT_MUTED),
            );
        });
    });
}

// ── Stage strip (A→B trace) ─────────────────────────────────────────────────

/// Horizontal storyboard: one card per step with a pulse traveling the
/// connector into the next step. Returns the clicked step (seek).
pub fn stage_strip(
    ui: &mut egui::Ui,
    idx: &Index,
    path: &[String],
    step: usize,
    progress: f32,
    pulse: f32,
) -> Option<usize> {
    let mut clicked = None;
    egui::ScrollArea::horizontal()
        .id_salt("flow-stage-strip")
        .auto_shrink([false, true])
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                for (i, id) in path.iter().enumerate() {
                    if i > 0 {
                        link(ui, i - 1, step, pulse);
                    }
                    if card(ui, idx, id, i, step, progress).clicked() {
                        clicked = Some(i);
                    }
                }
            });
        });
    clicked
}

/// One step card: module color spine, symbol name, file basename. The active
/// card carries the `theme::step_alpha` wash; done vs upcoming differ in
/// static fills so the story reads with zero motion.
fn card(
    ui: &mut egui::Ui,
    idx: &Index,
    id: &str,
    i: usize,
    step: usize,
    progress: f32,
) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(CARD_W, CARD_H), egui::Sense::click());
    let active = i == step;
    let done = i < step;
    let color = theme::module_color(&App::module_of(file_of(id)));

    let fill = if active {
        theme::with_alpha(theme::ACCENT, (theme::step_alpha(progress) * 255.0) as u8)
    } else if done {
        theme::SURFACE_E2
    } else {
        theme::SURFACE_E1
    };
    let corner = egui::CornerRadius::same(theme::CORNER_MD);
    ui.painter().rect_filled(rect, corner, fill);
    ui.painter().rect_stroke(
        rect,
        corner,
        egui::Stroke::new(1.0, if active { color } else { theme::BORDER_SUBTLE }),
        egui::StrokeKind::Middle,
    );
    // Module color spine down the left edge.
    let spine = egui::Rect::from_min_max(
        egui::pos2(rect.min.x + 1.0, rect.min.y + 1.0),
        egui::pos2(rect.min.x + 4.0, rect.max.y - 1.0),
    );
    ui.painter().rect_filled(
        spine,
        egui::CornerRadius::ZERO,
        if done || active {
            color
        } else {
            theme::with_alpha(color, 120)
        },
    );

    let name_color = if done {
        theme::TEXT_SECONDARY
    } else {
        theme::TEXT_PRIMARY
    };
    let inner = rect.shrink2(egui::vec2(theme::SPACE_3 + 3.0, theme::SPACE_2));
    ui.scope_builder(
        egui::UiBuilder::new()
            .id_salt(("flow-card", i))
            .max_rect(inner)
            .layout(egui::Layout::top_down(egui::Align::Min)),
        |ui| {
            ui.add(
                egui::Label::new(
                    egui::RichText::new(display_name(idx, id))
                        .size(theme::FONT_BODY)
                        .strong()
                        .color(name_color),
                )
                .truncate(),
            );
            ui.add(
                egui::Label::new(
                    egui::RichText::new(basename(file_of(id)))
                        .size(theme::FONT_SMALL)
                        .color(theme::TEXT_MUTED),
                )
                .truncate(),
            );
        },
    );
    resp.on_hover_text(format!("{id}\nclick to seek here"))
}

/// Connector between cards `i` and `i+1`. The segment after the active card
/// carries a pulse dot looping every `theme::PULSE_MS`, eased smoothstep; when
/// paused the dot freezes at the eased step progress (still, not hidden).
fn link(ui: &mut egui::Ui, segment: usize, step: usize, pulse: f32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(LINK_W, CARD_H), egui::Sense::hover());
    let y = rect.center().y;
    let x0 = rect.left() + 2.0;
    let x1 = rect.right() - 2.0;
    let active = segment == step;
    let done = segment < step;
    let track = if done {
        theme::with_alpha(theme::ACCENT, 120)
    } else {
        theme::BORDER_SUBTLE
    };
    ui.painter()
        .hline(x0..=x1, y, egui::Stroke::new(1.0, track));
    if active {
        let px = x0 + (x1 - x0) * pulse;
        // Soft halo + trail so the dot reads as motion, not a bead.
        ui.painter()
            .hline(px..=x1, y, egui::Stroke::new(1.0, theme::accent_subtle(90)));
        ui.painter()
            .circle_filled(egui::pos2(px, y), 4.5, theme::with_alpha(theme::ACCENT, 60));
        ui.painter()
            .circle_filled(egui::pos2(px, y), 2.5, theme::ACCENT);
    }
}

// ── Wave view (entrypoint layers) ───────────────────────────────────────────

/// BFS layers as rows that fill in one layer at a time. Unrevealed layers stay
/// visible-but-dim, so the whole fan is readable while paused. Returns the
/// clicked chip as (layer, symbol id).
pub fn wave_view(
    ui: &mut egui::Ui,
    idx: &Index,
    layers: &[Vec<String>],
    step: usize,
    progress: f32,
) -> Option<(usize, String)> {
    let mut clicked = None;
    for (li, layer) in layers.iter().enumerate() {
        let revealed = li <= step;
        let active = li == step;
        ui.horizontal(|ui| {
            ui.allocate_ui_with_layout(
                egui::vec2(40.0, theme::ROW_HEIGHT),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    ui.label(
                        egui::RichText::new(format!("L{li}"))
                            .size(theme::FONT_SMALL)
                            .strong()
                            .color(if active {
                                theme::ACCENT
                            } else {
                                theme::TEXT_MUTED
                            }),
                    );
                },
            );
            ui.horizontal_wrapped(|ui| {
                for id in layer {
                    if chip(ui, idx, id, revealed, active, progress).clicked() {
                        clicked = Some((li, id.clone()));
                    }
                }
            });
        });
    }
    clicked
}

/// One wave chip: module dot + symbol name. Active-layer chips carry the
/// step wash; unrevealed chips render dim but fully legible.
fn chip(
    ui: &mut egui::Ui,
    idx: &Index,
    id: &str,
    revealed: bool,
    active: bool,
    progress: f32,
) -> egui::Response {
    let name = display_name(idx, id);
    let color = theme::module_color(&App::module_of(file_of(id)));
    let font = egui::FontId::new(theme::FONT_SMALL, egui::FontFamily::Proportional);
    // Unrevealed chips stay dim but fully legible (static fallback first).
    let name_color = if revealed {
        theme::TEXT_PRIMARY
    } else {
        theme::TEXT_DISABLED
    };
    let galley = ui.painter().layout_no_wrap(name, font, name_color);
    let size = galley.size() + egui::vec2(26.0, 8.0);
    let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::click());

    let fill = if active {
        theme::with_alpha(theme::ACCENT, (theme::step_alpha(progress) * 255.0) as u8)
    } else if revealed {
        theme::SURFACE_E2
    } else {
        theme::SURFACE_E1
    };
    let corner = egui::CornerRadius::same(theme::CORNER_SM);
    ui.painter().rect_filled(rect, corner, fill);
    ui.painter().rect_stroke(
        rect,
        corner,
        egui::Stroke::new(1.0, if active { color } else { theme::BORDER_SUBTLE }),
        egui::StrokeKind::Middle,
    );
    ui.painter().circle_filled(
        egui::pos2(rect.left() + 9.0, rect.center().y),
        3.0,
        if revealed { color } else { theme::DIM },
    );
    ui.painter().galley(
        egui::pos2(rect.left() + 15.0, rect.center().y - galley.size().y / 2.0),
        galley,
        theme::TEXT_PRIMARY,
    );
    resp.on_hover_text(format!("{id}\nclick to inspect"))
}

// ── Step detail ─────────────────────────────────────────────────────────────

/// Detail card for one step: name, full signature, file:line, callers/callees
/// from the index, the incoming edge's confidence, and the Open source jump.
/// Returns Some(symbol id) when Open source is clicked.
pub fn step_detail(
    ui: &mut egui::Ui,
    idx: &Index,
    id: &str,
    incoming: Option<(String, Confidence)>,
) -> Option<String> {
    let mut open = None;
    egui::Frame::new()
        .fill(theme::SURFACE_E0)
        .stroke(egui::Stroke::new(1.0, theme::BORDER_SUBTLE))
        .corner_radius(egui::CornerRadius::same(theme::CORNER_MD))
        .inner_margin(egui::Margin::symmetric(
            theme::SPACE_3 as i8,
            theme::SPACE_3 as i8,
        ))
        .show(ui, |ui| {
            let sym = idx.symbols.iter().find(|s| s.id == id);
            let name = sym
                .map(|s| s.name.clone())
                .unwrap_or_else(|| id.to_string());
            let file = sym
                .map(|s| s.file.clone())
                .unwrap_or_else(|| file_of(id).to_string());
            let line = sym.map(|s| s.start_line).unwrap_or_else(|| line_of(id));
            let color = theme::module_color(&App::module_of(&file));

            ui.horizontal(|ui| {
                ui.painter().circle_filled(
                    ui.cursor().min + egui::vec2(4.0, theme::FONT_BODY / 2.0 + 2.0),
                    4.0,
                    color,
                );
                ui.label(egui::RichText::new(name).size(theme::FONT_HEADING).strong());
                if let Some(k) = sym.map(|s| s.kind) {
                    ui.weak(kind_label(k));
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button("Open source").clicked() {
                        open = Some(id.to_string());
                    }
                });
            });

            if let Some(s) = sym.filter(|s| !s.signature.is_empty()) {
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(&s.signature)
                            .monospace()
                            .size(theme::FONT_CODE)
                            .color(theme::INFO),
                    )
                    .wrap(),
                );
            }
            ui.label(
                egui::RichText::new(format!("{file}:{line}"))
                    .size(theme::FONT_SMALL)
                    .color(theme::TEXT_MUTED),
            );

            if let Some((caller, conf)) = incoming {
                ui.horizontal(|ui| {
                    ui.weak("incoming call");
                    ui.label(
                        egui::RichText::new(confidence_label(conf))
                            .size(theme::FONT_SMALL)
                            .strong()
                            .color(confidence_color(conf)),
                    );
                    ui.weak(format!("from {caller}"));
                });
            }

            let (callers, callees) = neighbours(idx, id);
            neighbour_row(ui, "callers", &callers);
            neighbour_row(ui, "callees", &callees);
        });
    open
}

/// One muted `label: a, b, c (+N more)` row, elided when long.
fn neighbour_row(ui: &mut egui::Ui, label: &str, names: &[String]) {
    let text = if names.len() > 4 {
        format!(
            "{label}: {} (+{} more)",
            names[..4].join(", "),
            names.len() - 4
        )
    } else if names.is_empty() {
        format!("{label}: —")
    } else {
        format!("{label}: {}", names.join(", "))
    };
    ui.add(
        egui::Label::new(
            egui::RichText::new(text)
                .size(theme::FONT_SMALL)
                .color(theme::TEXT_SECONDARY),
        )
        .truncate(),
    );
}

// ── Shared helpers ──────────────────────────────────────────────────────────

/// Direct call neighbours of `id`, deduped and display-named.
fn neighbours(idx: &Index, id: &str) -> (Vec<String>, Vec<String>) {
    let mut callers = Vec::new();
    let mut callees = Vec::new();
    for e in &idx.edges {
        if e.callee == id {
            callers.push(display_name(idx, &e.caller));
        }
        if e.caller == id {
            callees.push(display_name(idx, &e.callee));
        }
    }
    callers.sort();
    callers.dedup();
    callees.sort();
    callees.dedup();
    (callers, callees)
}

/// Symbol id (`path:start_line`) → file path.
pub fn file_of(id: &str) -> &str {
    id.rsplit_once(':').map(|(f, _)| f).unwrap_or(id)
}

/// Line component of a symbol id (0 when unparsable).
pub fn line_of(id: &str) -> usize {
    id.rsplit_once(':')
        .and_then(|(_, l)| l.parse().ok())
        .unwrap_or(0)
}

/// Human name for a symbol id: indexed name, or the raw id when unresolved.
pub fn display_name(idx: &Index, id: &str) -> String {
    idx.symbols
        .iter()
        .find(|s| s.id == id)
        .map(|s| s.name.clone())
        .unwrap_or_else(|| id.to_string())
}

fn basename(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
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

fn confidence_label(c: Confidence) -> &'static str {
    match c {
        Confidence::Exact => "exact",
        Confidence::NameOnly => "name-only",
        Confidence::Ambiguous => "ambiguous",
        Confidence::Unresolved => "unresolved",
    }
}

fn confidence_color(c: Confidence) -> egui::Color32 {
    match c {
        Confidence::Exact => theme::SUCCESS,
        Confidence::NameOnly => theme::INFO,
        Confidence::Ambiguous => theme::WARNING,
        Confidence::Unresolved => theme::DANGER,
    }
}
