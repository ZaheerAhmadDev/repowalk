//! Flow visuals: transport row, two free-floating interactive canvases (the
//! A→B trace chain and the entrypoint wave fan), and the per-step detail panel.
//!
//! B5-03: cards/chips can be dragged anywhere, the canvas pans by dragging
//! empty space and zooms about the cursor, and a Reset control (plus
//! double-click on empty canvas) restores the deterministic chain/row layout.
//! A press released within the click tolerance keeps its click meaning
//! (seek / inspect), so drag and click coexist. Building on the shared
//! [`crate::diagrams::interact`] primitive (B5-02: Layer/CanvasState/frame),
//! whose state persists in egui temp memory per Flow sub-tab. Playback and the
//! Feature tab are untouched. Animation only ever adds emphasis — every card
//! is fully legible while paused (WCAG 2.3.3, DSN-1).
//! OWNER: TL-GRAPH (GRAPH-1 / B5-03 interactivity).
use eframe::egui;
use repowalk_core::model::{Confidence, Index, SymbolKind};
use std::collections::BTreeMap;

use crate::app::App;
use crate::diagrams::interact;
use crate::flow::{playback, FlowState, FlowTab};
use crate::theme;

/// Stage-card geometry (canvas units, at scale 1). The deterministic chain is
/// the initial layout.
const CARD_W: f32 = 148.0;
const CARD_H: f32 = 54.0;
/// Connector gap between two cards — where the pulse travels.
const LINK_W: f32 = 30.0;
/// Left/top margin of the deterministic chain/row layout (canvas units).
const CANVAS_MARGIN: f32 = 6.0;
/// Wave chip height (canvas units).
const CHIP_H: f32 = 22.0;
/// Width reserved for the `L{i}` layer label column (canvas units).
const LAYER_LABEL_W: f32 = 34.0;
/// Horizontal gap between wave chips (canvas units).
const CHIP_GAP: f32 = 6.0;
/// Vertical pitch between wave chip rows (canvas units).
const CHIP_ROW_PITCH: f32 = 28.0;
/// Extra vertical space between wave layers (canvas units).
const LAYER_GAP: f32 = 10.0;

// ── Layout math (pure; unit-tested below) ───────────────────────────────────

/// Deterministic chain layout: card `i`'s top-left at
/// `(CANVAS_MARGIN + i·(CARD_W + LINK_W), CANVAS_MARGIN)`.
pub fn chain_pos(i: usize) -> egui::Pos2 {
    egui::pos2(CANVAS_MARGIN + i as f32 * (CARD_W + LINK_W), CANVAS_MARGIN)
}

/// Wrap chip widths left→right into rows within `max_w` (a chip wider than
/// `max_w` gets a row of its own; empty input ⇒ empty output). Row advance is
/// [`CHIP_ROW_PITCH`]. Returns each chip's row-relative x offset and the y of
/// its row.
pub fn wrap_positions(widths: &[f32], max_w: f32) -> Vec<egui::Pos2> {
    let mut out = Vec::with_capacity(widths.len());
    let mut x = 0.0;
    let mut y = 0.0;
    for &w in widths {
        let w = w.max(1.0);
        if x > 0.0 && (x + w > max_w || w > max_w) {
            x = 0.0;
            y += CHIP_ROW_PITCH;
        }
        out.push(egui::pos2(x, y));
        x += w + CHIP_GAP;
    }
    out
}

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

// ── Shared canvas plumbing (B5-03, atop diagrams::interact) ────────────────

/// egui-id of the persisted canvas state (layer + per-item offsets) per view.
fn canvas_id(tab: FlowTab) -> egui::Id {
    let name = match tab {
        FlowTab::Trace => "trace",
        FlowTab::Waves => "waves",
        FlowTab::Feature => "feature",
    };
    egui::Id::new("repowalk_flow_canvas").with(name)
}

/// egui-id of the hovered-item memo (drives hover-dependent repaints).
fn hover_memo_id(tab: FlowTab) -> egui::Id {
    canvas_id(tab).with("hover")
}

/// Hover tooltip content: symbol id + intent hint (the text the old widgets
/// showed); wave chips strip their `L{i}:` prefix back to the plain id.
fn tooltip_for(tab: FlowTab, key: &str) -> String {
    match tab {
        FlowTab::Trace => format!("{key}\nclick to seek here"),
        _ => {
            let id = key.split_once(':').map(|(_, id)| id).unwrap_or(key);
            format!("{id}\nclick to inspect")
        }
    }
}

/// Shared canvas interaction handling, used verbatim by both flow canvases.
/// All pan / zoom / drag / click semantics live in [`interact::frame`]; this
/// adds flow-mode chrome: the interaction hint, a `reset view` button
/// (double-click on empty canvas does the same), drag cursors, and exactly one
/// tooltip describing the hovered item.
///
/// Items are `(key, base_rect)` pairs in draw order; the rect is in canvas
/// space. `interact::frame` applies per-item offsets and the layer transform
/// for picking, so picking order == paint order.
fn canvas_input(
    ui: &mut egui::Ui,
    tab: FlowTab,
    items: &[(String, egui::Rect)],
) -> (interact::CanvasState, egui::Rect, Option<String>) {
    let id = canvas_id(tab);
    let memo = hover_memo_id(tab);
    let ctx = ui.ctx().clone();
    let mut reset = false;

    // Toolbar: interaction hint (left) + Reset (right).
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("drag items anywhere · drag empty space to pan · scroll to zoom")
                .size(theme::FONT_SMALL)
                .color(theme::TEXT_MUTED),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .small_button("reset view")
                .on_hover_text("Restore the deterministic layout, pan and zoom")
                .clicked()
            {
                reset = true;
            }
        });
    });

    let height = (ui.available_height() * 0.6).clamp(160.0, 340.0);
    let size = egui::vec2(ui.available_width().max(120.0), height);
    let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::click_and_drag());

    // Pan / zoom / per-item drag / click — the shared diagrams primitive.
    let mut state = interact::CanvasState::load(&ctx, id);
    if reset {
        state.reset();
    }
    let (hover_key, actions) = interact::frame(&resp, &mut state, items);
    if actions.double_clicked_empty {
        state.reset(); // double-click on empty canvas = reset, same as the button
    }
    let clicked = actions.clicked;

    // Cursor + tooltip feedback; a hover change needs a repaint to show.
    let last_hover: Option<String> = ctx
        .data_mut(|d| d.get_temp::<Option<String>>(memo))
        .flatten();
    if hover_key != last_hover {
        ctx.data_mut(|d| d.insert_temp(memo, hover_key.clone()));
        ctx.request_repaint();
    }
    if resp.is_pointer_button_down_on() {
        ctx.set_cursor_icon(if state.is_dragging() {
            egui::CursorIcon::Grabbing
        } else {
            egui::CursorIcon::Grab
        });
    } else if hover_key.is_some() {
        ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if let Some(key) = &hover_key {
        resp.show_tooltip_text(tooltip_for(tab, key));
    }

    state.store(&ctx, id);
    (state, rect, clicked)
}

// ── Stage canvas (A→B trace) ────────────────────────────────────────────────

/// Free-floating storyboard of the A→B path: card `i` starts at
/// [`chain_pos`] + user offset, a pulse travels each connector into the next
/// step, and connectors follow the dragged cards. Cards drag, the canvas pans/
/// zooms; a click within the click tolerance seeks. Returns the clicked step.
pub fn stage_strip(
    ui: &mut egui::Ui,
    idx: &Index,
    path: &[String],
    step: usize,
    progress: f32,
    pulse: f32,
    tab: FlowTab,
) -> Option<usize> {
    if path.is_empty() {
        return None;
    }

    // Base layout: the deterministic chain, in canvas space, in paint order.
    let items: Vec<(String, egui::Rect)> = path
        .iter()
        .enumerate()
        .map(|(i, id)| {
            (
                id.clone(),
                egui::Rect::from_min_size(chain_pos(i), egui::vec2(CARD_W, CARD_H)),
            )
        })
        .collect();

    let (state, rect, clicked) = canvas_input(ui, tab, &items);
    let layer = state.layer;
    let scale = layer.scale;
    let painter = ui.painter().with_clip_rect(rect);
    // Apply offsets + transform with the shared primitive, once per card.
    let placed: Vec<egui::Rect> = items
        .iter()
        .map(|(k, base)| interact::placed_rect(*base, state.node_offset(k), layer))
        .collect();
    // `placed` is widget-relative (as the driver picked it); map to screen.
    let screen_rect =
        |local: egui::Rect| egui::Rect::from_min_size(rect.min + local.min.to_vec2(), local.size());

    // Connectors first so cards draw over them; each follows the dragged cards
    // (center-to-center, so a moved chain stays readable).
    for i in 0..placed.len().saturating_sub(1) {
        let a = screen_rect(placed[i]).center();
        let b = screen_rect(placed[i + 1]).center();
        let done = i < step;
        let track = if done {
            theme::with_alpha(theme::ACCENT, 120)
        } else {
            theme::BORDER_SUBTLE
        };
        painter.line_segment([a, b], egui::Stroke::new(1.0, track));
        if i == step {
            let dot = a + (b - a) * pulse;
            // Soft halo + trail so the dot reads as motion, not a bead.
            painter.line_segment([dot, b], egui::Stroke::new(1.0, theme::accent_subtle(90)));
            painter.circle_filled(dot, 4.5, theme::with_alpha(theme::ACCENT, 60));
            painter.circle_filled(dot, 2.5, theme::ACCENT);
        }
    }

    // Cards on top, same index order as the base layout (paint == pick order).
    for (i, id) in path.iter().enumerate() {
        draw_chain_card(
            &painter,
            ui,
            screen_rect(placed[i]),
            scale,
            CardVisual {
                idx,
                id,
                active: i == step,
                done: i < step,
                progress,
            },
        );
    }

    clicked.and_then(|key| path.iter().position(|id| id == &key))
}

/// Draw payload shared by canvas cards / chips: the step visual state, the
/// symbol identity, and which stage flag applies (keeps the draw functions
/// under clippy's argument budget).
struct CardVisual<'a> {
    idx: &'a Index,
    id: &'a str,
    /// The card the playhead is on.
    active: bool,
    /// Cards/chips the playhead has passed.
    done: bool,
    /// Playhead position inside the active step (for the transient wash).
    progress: f32,
}

/// One step card on the canvas: module color spine, symbol name, file basename.
/// Same visuals as the old chain (active wash, done vs upcoming fills), scaled
/// by the canvas zoom. Captions live in a scoped child `Ui` so truncate and
/// colors keep working; it clips to the card rect.
fn draw_chain_card(
    painter: &egui::Painter,
    ui: &mut egui::Ui,
    rect: egui::Rect,
    scale: f32,
    v: CardVisual<'_>,
) {
    let active = v.active;
    let done = v.done;
    let progress = v.progress;
    let color = theme::module_color(&App::module_of(file_of(v.id)));

    let fill = if active {
        theme::with_alpha(theme::ACCENT, (theme::step_alpha(progress) * 255.0) as u8)
    } else if done {
        theme::SURFACE_E2
    } else {
        theme::SURFACE_E1
    };
    let corner = egui::CornerRadius::same(theme::CORNER_MD);
    painter.rect_filled(rect, corner, fill);
    painter.rect_stroke(
        rect,
        corner,
        egui::Stroke::new(1.0, if active { color } else { theme::BORDER_SUBTLE }),
        egui::StrokeKind::Middle,
    );
    // Module color spine down the left edge.
    let spine_w = (3.0 * scale).max(0.6);
    let spine = egui::Rect::from_min_max(
        egui::pos2(rect.min.x + 1.0, rect.min.y + 1.0),
        egui::pos2(rect.min.x + 1.0 + spine_w, rect.max.y - 1.0),
    );
    painter.rect_filled(
        spine,
        egui::CornerRadius::ZERO,
        if done || active {
            color
        } else {
            theme::with_alpha(color, 120)
        },
    );

    if scale < 0.55 {
        return; // too small to read captions; the box still communicates
    }
    let name_color = if done {
        theme::TEXT_SECONDARY
    } else {
        theme::TEXT_PRIMARY
    };
    let inner = rect.shrink2(egui::vec2(theme::SPACE_3 + 3.0, theme::SPACE_2));
    ui.scope_builder(
        egui::UiBuilder::new()
            .id_salt(("flow-card", v.id))
            .max_rect(inner)
            .layout(egui::Layout::top_down(egui::Align::Min)),
        |ui| {
            ui.add(
                egui::Label::new(
                    egui::RichText::new(display_name(v.idx, v.id))
                        .size(theme::FONT_BODY * scale)
                        .strong()
                        .color(name_color),
                )
                .truncate(),
            );
            ui.add(
                egui::Label::new(
                    egui::RichText::new(basename(file_of(v.id)))
                        .size(theme::FONT_SMALL * scale)
                        .color(theme::TEXT_MUTED),
                )
                .truncate(),
            );
        },
    );
}

// ── Wave canvas (entrypoint layers) ─────────────────────────────────────────

/// Entrypoint BFS layers as draggable chip rows on a free-floating canvas.
/// Layers fill in one at a time; unrevealed layers stay visible-but-dim. Chips
/// drag individually; the canvas pans/zooms; a click within the click
/// tolerance inspects. Returns the clicked chip as (layer, symbol id).
pub fn wave_view(
    ui: &mut egui::Ui,
    idx: &Index,
    layers: &[Vec<String>],
    step: usize,
    progress: f32,
    tab: FlowTab,
) -> Option<(usize, String)> {
    // Deterministic wrapped layout: chip widths at scale 1 (plus dot and
    // padding), rows per layer bounded by the canvas width, layers stacked.
    let max_w = (ui.available_width() - CANVAS_MARGIN - CANVAS_MARGIN - LAYER_LABEL_W).max(60.0);
    let mut key_of: BTreeMap<String, (usize, String)> = BTreeMap::new();
    let mut items: Vec<(String, egui::Rect)> = Vec::new();
    let mut layer_tops: Vec<f32> = Vec::with_capacity(layers.len());
    let mut layer_y = CANVAS_MARGIN;
    for (li, layer) in layers.iter().enumerate() {
        layer_tops.push(layer_y);
        let widths: Vec<f32> = layer
            .iter()
            .map(|id| {
                let galley = ui.painter().layout_no_wrap(
                    display_name(idx, id),
                    egui::FontId::new(theme::FONT_SMALL, egui::FontFamily::Proportional),
                    theme::TEXT_PRIMARY,
                );
                galley.size().x + 26.0
            })
            .collect();
        let positions = wrap_positions(&widths, max_w);
        for (j, id) in layer.iter().enumerate() {
            let key = format!("L{li}:{id}");
            key_of.insert(key.clone(), (li, id.clone()));
            let p = positions
                .get(j)
                .copied()
                .unwrap_or(egui::pos2(0.0, CHIP_ROW_PITCH * widths.len() as f32));
            let rect = egui::Rect::from_min_size(
                egui::pos2(CANVAS_MARGIN + LAYER_LABEL_W + p.x, layer_y + p.y),
                egui::vec2(widths[j], CHIP_H),
            );
            items.push((key, rect));
        }
        let last_row = positions
            .last()
            .map(|p| p.y)
            .unwrap_or((widths.len().saturating_sub(1) as f32) * CHIP_ROW_PITCH);
        layer_y += last_row + CHIP_H + LAYER_GAP;
    }

    let (state, rect, clicked) = canvas_input(ui, tab, &items);
    let layer_transform = state.layer;
    let painter = ui.painter().with_clip_rect(rect);
    // Screen rect of one placed chip (offsets + transform applied once).
    let placed: Vec<egui::Rect> = items
        .iter()
        .map(|(k, base)| interact::placed_rect(*base, state.node_offset(k), layer_transform))
        .collect();
    let screen_rect =
        |local: egui::Rect| egui::Rect::from_min_size(rect.min + local.min.to_vec2(), local.size());

    // Layer labels (`L{i}`) track each layer's first row exactly like before.
    for (li, layer) in layers.iter().enumerate() {
        if layer.is_empty() {
            continue;
        }
        let label =
            layer_transform.to_screen(egui::pos2(CANVAS_MARGIN, layer_tops[li] + CHIP_H / 2.0));
        painter.text(
            rect.min + label.to_vec2(),
            egui::Align2::LEFT_CENTER,
            format!("L{li}"),
            egui::FontId::new(
                theme::FONT_SMALL * layer_transform.scale,
                egui::FontFamily::Proportional,
            ),
            if li == step {
                theme::ACCENT
            } else {
                theme::TEXT_MUTED
            },
        );
    }

    // Chips on top (draw order = item build order = layer then row order).
    for (i, (key, _)) in items.iter().enumerate() {
        let Some((li, id)) = key_of.get(key) else {
            continue;
        };
        draw_wave_chip(
            &painter,
            ui,
            screen_rect(placed[i]),
            layer_transform.scale,
            CardVisual {
                idx,
                id,
                active: *li == step,
                done: *li < step,
                progress,
            },
        );
    }

    clicked.and_then(|key| key_of.get(&key).cloned())
}

/// One wave chip on the canvas: module dot + symbol name. Active-layer chips
/// carry the step wash; unrevealed chips render dim but fully legible (same
/// fills/colors as the old row layout, scaled by the canvas zoom). For chips
/// "done" reads as "revealed in an earlier layer".
fn draw_wave_chip(
    painter: &egui::Painter,
    ui: &mut egui::Ui,
    rect: egui::Rect,
    scale: f32,
    v: CardVisual<'_>,
) {
    let active = v.active;
    let revealed = v.active || v.done;
    let progress = v.progress;
    let id = v.id;
    let color = theme::module_color(&App::module_of(file_of(id)));
    let fill = if active {
        theme::with_alpha(theme::ACCENT, (theme::step_alpha(progress) * 255.0) as u8)
    } else if revealed {
        theme::SURFACE_E2
    } else {
        theme::SURFACE_E1
    };
    let corner = egui::CornerRadius::same(theme::CORNER_SM);
    painter.rect_filled(rect, corner, fill);
    painter.rect_stroke(
        rect,
        corner,
        egui::Stroke::new(1.0, if active { color } else { theme::BORDER_SUBTLE }),
        egui::StrokeKind::Middle,
    );
    painter.circle_filled(
        egui::pos2(rect.min.x + 9.0 * scale, rect.center().y),
        3.0 * scale,
        if revealed { color } else { theme::DIM },
    );
    if scale < 0.55 {
        return; // unreadable glyphs; the chip and dot still communicate
    }
    let name_color = if revealed {
        theme::TEXT_PRIMARY
    } else {
        theme::TEXT_DISABLED
    };
    let inner = rect.shrink2(egui::vec2(13.0 * scale, 4.0 * scale));
    ui.scope_builder(
        egui::UiBuilder::new()
            .id_salt(("flow-chip", v.id))
            .max_rect(inner)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
        |ui| {
            ui.add(
                egui::Label::new(
                    egui::RichText::new(display_name(v.idx, v.id))
                        .size(theme::FONT_SMALL * scale)
                        .color(name_color),
                )
                .truncate(),
            );
        },
    );
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

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::pos2;

    #[test]
    fn chain_layout_is_the_deterministic_storyboard() {
        let first = chain_pos(0);
        assert_eq!(first.x, CANVAS_MARGIN);
        assert_eq!(first.y, CANVAS_MARGIN);
        let next = chain_pos(3);
        let prev = chain_pos(2);
        assert!(
            ((next.x - prev.x) - (CARD_W + LINK_W)).abs() < f32::EPSILON,
            "cards advance one pitch each"
        );
        assert!((next.y - prev.y).abs() < f32::EPSILON, "one row");
    }

    #[test]
    fn wrap_positions_fills_rows_and_never_squeezes_a_wide_chip() {
        // 20-unit rows: 3 and 3 on row one, then the 8-wide chip wraps.
        let pos = wrap_positions(&[3.0, 3.0, 8.0, 2.0], 20.0);
        assert_eq!(pos.len(), 4);
        assert_eq!(pos[0], pos2(0.0, 0.0));
        assert_eq!(pos[1], pos2(3.0 + CHIP_GAP, 0.0));
        assert_eq!(pos[2], pos2(0.0, CHIP_ROW_PITCH), "third chip wraps");
        assert_eq!(pos[3], pos2(8.0 + CHIP_GAP, CHIP_ROW_PITCH));
        // A chip wider than the max gets its own row, placed at x = 0.
        let lone = wrap_positions(&[2.0, 20.0], 10.0);
        assert_eq!(lone[1], pos2(0.0, CHIP_ROW_PITCH));
        assert_eq!(wrap_positions(&[], 10.0).len(), 0);
    }

    #[test]
    fn tooltip_content_differs_by_tab_and_strips_the_layer_prefix() {
        assert_eq!(
            tooltip_for(FlowTab::Trace, "src/a.rs:1"),
            "src/a.rs:1\nclick to seek here"
        );
        assert_eq!(
            tooltip_for(FlowTab::Waves, "L2:src/a.rs:1"),
            "src/a.rs:1\nclick to inspect",
        );
    }

    #[test]
    fn canvas_ids_partition_by_flow_sub_tab() {
        let t = canvas_id(FlowTab::Trace);
        let w = canvas_id(FlowTab::Waves);
        let f = canvas_id(FlowTab::Feature);
        assert_ne!(t, w);
        assert_ne!(w, f);
        assert_ne!(t, f);
        // The hover memo is a separate key per view too.
        assert_ne!(hover_memo_id(FlowTab::Trace), t);
        assert_ne!(
            hover_memo_id(FlowTab::Waves),
            hover_memo_id(FlowTab::Feature)
        );
    }
}
