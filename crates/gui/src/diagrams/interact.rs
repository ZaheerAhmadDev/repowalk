//! Reusable pan / zoom / node-drag layer for painter-canvas diagrams. OWNER:
//! B5-02 (GUI/diagrams) — B5-05 reuses this primitive for the Directory panel.
//!
//! Model: a pure [`Layer`] transform `screen = pan + canvas * scale`, plus
//! per-node drag offsets in canvas coordinates. Panels keep their deterministic
//! `layout(...)` functions as the canonical initial state; interaction is
//! applied strictly on top, never absorbed into the layout math.
//!
//! Zoom gesture — documented choice: the DEFAULT is the plain mouse wheel, i.e.
//! the wheel zooms about the cursor with no modifier. Rationale: these canvases
//! deliberately replaced their ScrollArea with this layer (see the wiring in
//! [`control`]/[`types`]/[`modules`]), so the wheel is free and panning is the
//! mouse-drag. The modifier variant is Ctrl+wheel (and pinch on trackpads),
//! which egui folds into `InputState::zoom_delta` and which this layer applies
//! identically — egui never sends both in one frame, so there is no double
//! zoom.
//!
//! Movement-threshold detection: a click without movement must stay a
//! click/select. egui's `click_and_drag` sense already postpones its click vs
//! drag decision until the pointer moves beyond its click distance or is held
//! past its click duration; on top of that, this layer refuses to apply any
//! node/pan movement until the total drag delta crosses [`CLICK_TOLERANCE`]. so
//! a long-press-then-nudge cannot creep a node off its spot.
//!
//! Persistence: state ([`CanvasState`]) lives in egui's memory under a
//! per-panel `egui::Id` (the control-flow canvas additionally keys by the
//! selected symbol), so transforms and node offsets survive tab switches within
//! one session and die with the app — nothing on disk, no schema, no IO.
//!
//! All coordinate math is in the pure functions below plus one `pub struct`
//! each for the layer and the state, so it is unit-testable without a UI (the
//! existing repo pattern).

use std::collections::BTreeMap;

use eframe::egui;
use egui::{Pos2, Rect, Vec2};

use crate::theme;

// ── Tuning knobs ────────────────────────────────────────────────────────────

/// Zoom clamp (screen px per canvas px). Wid enough to read to the per-node
/// texts; small enough that a stray wheel burst can't blow up the canvas.
pub const MIN_SCALE: f32 = 0.35;
pub const MAX_SCALE: f32 = 3.0;

/// One mouse-wheel notch is about this many px of scrolled delta (egui sends
/// ~50px per notch by default); used only for the plain-wheel zoom feel.
const WHEEL_TICK_PX: f32 = 50.0;
/// Per-notch multiplicative zoom step: one notch in = ×(1+WHEEL_STEP).
pub const WHEEL_STEP: f32 = 0.14;

/// Screen px a pointer must travel before drag movement starts being applied
/// (the second half of the movement-threshold detection).
pub const CLICK_TOLERANCE: f32 = 6.0;

// ── Pure layer math ─────────────────────────────────────────────────────────

/// The transform of one painted layer: `screen(canvas-local) = pan + p * scale`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Layer {
    pub scale: f32,
    pub pan: Vec2,
}

impl Default for Layer {
    fn default() -> Self {
        Layer {
            scale: 1.0,
            pan: Vec2::ZERO,
        }
    }
}

impl Layer {
    /// Clamp a raw scale onto [`MIN_SCALE`]/[`MAX_SCALE`] (and refuse NaN).
    pub fn clamp_scale(scale: f32) -> f32 {
        if scale.is_finite() {
            scale.clamp(MIN_SCALE, MAX_SCALE)
        } else {
            MIN_SCALE
        }
    }

    /// Move the layer by a screen-space delta (the pan gesture).
    pub fn pan_by(&self, screen_delta: Vec2) -> Layer {
        Layer {
            scale: self.scale,
            pan: self.pan + screen_delta,
        }
    }

    /// Zoom about a screen-space anchor (usually the cursor): whatever canvas
    /// point is under the anchor stays put while `scale` changes.
    pub fn zoom_at(&self, anchor: Vec2, factor: f32) -> Layer {
        if !factor.is_finite() || factor <= 0.0 || factor == 1.0 {
            return *self;
        }
        let scale = Self::clamp_scale(self.scale * factor);
        if scale == self.scale {
            return *self; // clamped or unchanged: the anchor must not move
        }
        let ratio = scale / self.scale;
        Layer {
            scale,
            pan: Vec2::new(
                anchor.x - (anchor.x - self.pan.x) * ratio,
                anchor.y - (anchor.y - self.pan.y) * ratio,
            ),
        }
    }

    /// Canvas point → screen (canvas-local) point.
    pub fn to_screen(self, canvas: Pos2) -> Pos2 {
        Pos2::new(
            self.pan.x + canvas.x * self.scale,
            self.pan.y + canvas.y * self.scale,
        )
    }

    /// Screen (canvas-local) point → canvas point (the inverse transform; the
    /// drag hit-test runs in canvas space through the placed rects).
    pub fn to_canvas(self, screen: Vec2) -> Pos2 {
        Pos2::new(
            (screen.x - self.pan.x) / self.scale,
            (screen.y - self.pan.y) / self.scale,
        )
    }

    /// Scale a canvas-space size into screen space.
    pub fn scale_size(self, canvas: Vec2) -> Vec2 {
        canvas * self.scale
    }
}

/// Screen rect of one node: its persisted drag offset (canvas coordinates)
/// applied to the base layout rect, then the layer transform. Deterministic
/// and pure — this is the only place node positions and the transform meet.
pub fn placed_rect(base: Rect, offset: Vec2, layer: Layer) -> Rect {
    Rect::from_min_size(
        layer.to_screen(base.translate(offset).min),
        layer.scale_size(base.size()),
    )
}

/// Which placed rect contains the point, if any — first match wins, so the
/// iteration order (paint order) is the picking order.
pub fn hit_index(pt: Pos2, placed: &[Rect]) -> Option<usize> {
    placed.iter().position(|r| r.contains(pt))
}

/// Screen drag delta → canvas-space delta for node offsets (nodes are stored
/// in canvas coordinates, so they survive pan/zoom).
pub fn canvas_delta(screen_delta: Vec2, scale: f32) -> Vec2 {
    Vec2::new(
        screen_delta.x / scale.max(f32::EPSILON),
        screen_delta.y / scale.max(f32::EPSILON),
    )
}

/// Did the total drag of a gesture leave the click zone? (Point validity is
/// the caller's: a zero vector is simply "not moved".)
pub fn beyond_tolerance(total_delta: Vec2, tolerance: f32) -> bool {
    total_delta.length_sq() > tolerance * tolerance
}

/// The zoom factor for one frame of plain-wheel input (the documented default
/// zoom gesture). Up (positive y) zooms in, down zooms out, `0` is no-op.
pub fn wheel_factor(scroll_delta_y: f32) -> f32 {
    if !scroll_delta_y.is_finite() || scroll_delta_y == 0.0 {
        1.0
    } else {
        (1.0 + WHEEL_STEP).powf(scroll_delta_y / WHEEL_TICK_PX)
    }
}

// ── Persistence + the per-frame interaction driver ──────────────────────────

/// One canvas's interaction state: the layer transform plus every node's drag
/// offset. Persisted in egui temp memory (see the module docs).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CanvasState {
    pub layer: Layer,
    /// node id → drag offset in canvas coordinates.
    offsets: BTreeMap<String, Vec2>,
    /// group key → shared offset in canvas coordinates. Members of a group are
    /// dragged together — one gesture writes the group offset and every
    /// member's placed rect includes it (e.g. a Types-diagram module moving
    /// with its header and all of its boxes).
    group_offsets: BTreeMap<String, Vec2>,
    /// What a drag in progress moves: an individual node or a whole group.
    dragging: Option<DragTarget>,
}

/// What the pointer currently grabs.
#[derive(Clone, Debug, PartialEq)]
pub enum DragTarget {
    Node(String),
    Group(String),
}

/// What the drivers ask the panel to react to this frame.
#[derive(Debug, Default)]
pub struct CanvasActions {
    /// A click that landed on the node with this id (egui's decided click,
    /// i.e. movement-threshold aware).
    pub clicked: Option<String>,
    /// A double-click on empty canvas — the reset gesture.
    pub double_clicked_empty: bool,
}

impl CanvasState {
    pub fn load(ctx: &egui::Context, id: egui::Id) -> CanvasState {
        ctx.data_mut(|d| d.get_temp::<CanvasState>(id).unwrap_or_default())
    }

    pub fn store(&self, ctx: &egui::Context, id: egui::Id) {
        ctx.data_mut(|d| d.insert_temp(id, self.clone()));
    }

    /// The persisted drag offset of one node (canvas coordinates); `ZERO` when
    /// the node has never been moved.
    pub fn node_offset(&self, node_id: &str) -> Vec2 {
        self.offsets.get(node_id).copied().unwrap_or(Vec2::ZERO)
    }

    /// The persisted shared offset of one group (canvas coordinates); `ZERO`
    /// when the group has never been moved.
    pub fn group_offset(&self, group: &str) -> Vec2 {
        self.group_offsets.get(group).copied().unwrap_or(Vec2::ZERO)
    }

    /// Node drag offsets are private so that only the driver can write them;
    /// readers can query membership with this.
    pub fn is_dragging(&self) -> bool {
        self.dragging.is_some()
    }

    /// Back to the canonical layout: layer identity and every offset.
    pub fn reset(&mut self) {
        self.layer = Layer::default();
        self.offsets.clear();
        self.group_offsets.clear();
        self.dragging = None;
    }
}

/// Run one frame of pan / zoom / node-drag / hover over a painter-canvas
/// response and hand the panel back the hover id plus the pending actions.
///
/// `items` is the *base* layout: stable node ids paired with their canonical
/// canvas-space rects, in the panel's paint order (picking order for
/// [`hit_index`]). Offsets and the layer transform are applied here, so
/// hover-picking agrees exactly with what is on screen.
///
/// Group-aware panels call [`frame_with`] instead; this plain variant keeps
/// per-node drags only.
pub fn frame(
    resp: &egui::Response,
    state: &mut CanvasState,
    items: &[(String, Rect)],
) -> (Option<String>, CanvasActions) {
    frame_with(resp, state, items, &BTreeMap::new())
}

/// The group-aware variant. `member_of` maps an item id to a group key:
///
/// * **Placement** — every member's placed position is its own offset *plus*
///   `[CanvasState::group_offset]` of its group, so hover-picking inside this
///   driver stays in exact lockstep with what painting shows.
/// * **Dragging** — a press that starts on a member grabs the *whole group*:
///   the drag delta lands on the shared offset and every member moves together
///   (a module name travelling with all of its type boxes). Nodes absent from
///   the map keep the individual-drag behaviour of [`frame`].
pub fn frame_with(
    resp: &egui::Response,
    state: &mut CanvasState,
    items: &[(String, Rect)],
    member_of: &BTreeMap<String, String>,
) -> (Option<String>, CanvasActions) {
    let mut actions = CanvasActions::default();
    let to_local = |p: Pos2| p - resp.rect.min.to_vec2();

    // Offsets are pure reads here (writes happen below); the map keeps every
    // downstream hit-test on the same table the painter uses.
    let total = |state: &CanvasState, id: &str| -> Vec2 {
        let mut d = state.node_offset(id);
        if let Some(group) = member_of.get(id) {
            d += state.group_offset(group);
        }
        d
    };

    // Hit-testing runs in canvas space (through the inverse transform), so it
    // is exactly the mirrored arithmetic of what painting does with
    // [`placed_rect`]; same iteration order, same picking order.
    let placed: Vec<Rect> = items
        .iter()
        .map(|(id, base)| base.translate(total(state, id)))
        .collect();

    let hovered = resp
        .hover_pos()
        .map(&to_local)
        .map(|q| state.layer.to_canvas(q.to_vec2()))
        .and_then(|canvas_pt| hit_index(canvas_pt, &placed).map(|i| items[i].0.clone()));

    // Zoom. The default gesture is the plain wheel (see module docs); Ctrl+wheel
    // and pinch arrive as egui's own zoom delta — modifier variant, same effect.
    if resp.hovered() {
        let (zoom_delta, scroll_y) = resp
            .ctx
            .input(|i| (i.zoom_delta(), i.smooth_scroll_delta.y));
        let factor = if zoom_delta != 1.0 {
            zoom_delta
        } else {
            wheel_factor(scroll_y)
        };
        if factor != 1.0 {
            if let Some(anchor) = resp.hover_pos().map(|p| p - resp.rect.min.to_vec2()) {
                state.layer = state.layer.zoom_at(anchor.to_vec2(), factor);
            }
        }
    }

    // Drag classification: a member grabs its whole group, any other node
    // grabs itself; a press on empty canvas pans the whole layer.
    if resp.drag_started_by(egui::PointerButton::Primary) {
        state.dragging = None;
        if let Some(press) = resp
            .interact_pointer_pos()
            .map(&to_local)
            .map(|q| state.layer.to_canvas(q.to_vec2()))
            .and_then(|canvas_pt| hit_index(canvas_pt, &placed))
        {
            let id = &items[press].0;
            state.dragging = Some(match member_of.get(id) {
                Some(group) => DragTarget::Group(group.clone()),
                None => DragTarget::Node(id.clone()),
            });
        }
    }

    if resp.dragged_by(egui::PointerButton::Primary) {
        let total_delta = resp.total_drag_delta().unwrap_or(Vec2::ZERO);
        if beyond_tolerance(total_delta, CLICK_TOLERANCE) {
            let delta = resp.drag_delta();
            match state.dragging.clone() {
                Some(DragTarget::Node(id)) => {
                    let moved = state.node_offset(&id) + canvas_delta(delta, state.layer.scale);
                    state.offsets.insert(id, moved);
                }
                Some(DragTarget::Group(group)) => {
                    let moved = state.group_offset(&group) + canvas_delta(delta, state.layer.scale);
                    state.group_offsets.insert(group, moved);
                }
                None => {
                    state.layer = state.layer.pan_by(delta);
                }
            }
        }
    }

    // Click semantics stay untouched: egui only reports a click once its own
    // threshold decision has (sub-threshold movement = click, not drag).
    if resp.clicked() {
        actions.clicked = hovered.clone();
    }
    // Double-click on empty canvas restores the canonical layout.
    if resp.double_clicked() && hovered.is_none() {
        actions.double_clicked_empty = true;
    }

    // The gesture is over (or was never a drag): forget the grab.
    if !resp.is_pointer_button_down_on() {
        state.dragging = None;
    }

    (hovered, actions)
}

// ── Canvas-side hover tags ───────────────────────────────────────────────────

/// A small near-cursor hint tag for painter canvases — the in-canvas stand-in
/// for egui's tooltip (which needs a widget response; these canvases paint
/// with the painter). One `title` line plus muted detail lines, drawn beside
/// the cursor and flipped/clamped so it stays inside its clip rect. Painting
/// only: no memory, no interaction, nothing to persist.
pub fn hover_tag(painter: &egui::Painter, anchor: Pos2, title: &str, lines: &[String]) {
    if title.is_empty() {
        return;
    }
    let title_font = egui::FontId::proportional(theme::FONT_BODY);
    let line_font = egui::FontId::proportional(theme::FONT_SMALL);
    let title_gal = painter.layout_no_wrap(title.to_owned(), title_font, theme::TEXT_PRIMARY);
    let mut width = title_gal.size().x;
    let mut height = title_gal.size().y + theme::SPACE_2;
    let mut line_galleys = Vec::with_capacity(lines.len());
    for line in lines {
        let gal = painter.layout_no_wrap(line.clone(), line_font.clone(), theme::TEXT_SECONDARY);
        width = width.max(gal.size().x);
        height += gal.size().y + 2.0;
        line_galleys.push(gal);
    }

    const PAD: f32 = 6.0;
    const LEAD: Vec2 = Vec2::new(14.0, 16.0);
    let size = Vec2::new(width + 2.0 * PAD, height + 2.0 * PAD);
    let clip = painter.clip_rect();
    let mut min = anchor + LEAD;
    if min.x + size.x > clip.max.x {
        min.x = anchor.x - LEAD.x + 2.0 - size.x;
    }
    if min.x < clip.min.x + PAD {
        min.x = clip.min.x + PAD;
    }
    if min.y + size.y > clip.max.y {
        min.y = anchor.y - LEAD.y + 2.0 - size.y;
    }
    if min.y < clip.min.y + PAD {
        min.y = clip.min.y + PAD;
    }

    let rect = Rect::from_min_size(min, size);
    painter.rect_filled(
        rect,
        egui::CornerRadius::same(theme::CORNER_SM),
        theme::SURFACE_E3,
    );
    painter.rect_stroke(
        rect,
        egui::CornerRadius::same(theme::CORNER_SM),
        egui::Stroke::new(1.0, theme::BORDER_SUBTLE),
        egui::StrokeKind::Middle,
    );

    let mut y = min.y + PAD;
    painter.galley(
        Pos2::new(min.x + PAD, y),
        title_gal.clone(),
        theme::TEXT_PRIMARY,
    );
    y += title_gal.size().y + theme::SPACE_2;
    for gal in &line_galleys {
        painter.galley(
            Pos2::new(min.x + PAD, y),
            gal.clone(),
            theme::TEXT_SECONDARY,
        );
        y += gal.size().y + 2.0;
    }
}

// ── Tests (pure math; no UI, no window) ─────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::vec2;

    #[test]
    fn to_screen_and_to_canvas_round_trip() {
        let layer = Layer {
            scale: 1.7,
            pan: Vec2::new(40.0, -25.0),
        };
        for p in [Pos2::ZERO, Pos2::new(120.0, 60.0), Pos2::new(-80.0, 310.0)] {
            let on_screen = layer.to_screen(p);
            let back = layer.to_canvas(on_screen.to_vec2());
            assert!((back.x - p.x).abs() < 1e-4);
            assert!((back.y - p.y).abs() < 1e-4);
        }
    }

    #[test]
    fn identity_layer_is_translation_only_at_scale_one() {
        let layer = Layer::default();
        let p = Pos2::new(37.0, 41.0);
        assert_eq!(layer.to_screen(p), p);
        assert_eq!(
            placed_rect(
                Rect::from_min_size(p, Vec2::new(10.0, 5.0)),
                Vec2::ZERO,
                layer
            ),
            Rect::from_min_size(p, Vec2::new(10.0, 5.0))
        );
    }

    #[test]
    fn zoom_keeps_the_point_under_the_anchor_still() {
        let layer = Layer::default();
        let anchor = Vec2::new(220.0, 130.0);
        let point_under_anchor = layer.to_canvas(anchor);
        let zoomed = layer.zoom_at(anchor, 1.5);
        let after = zoomed.to_screen(point_under_anchor).to_vec2();
        assert!((after.x - anchor.x).abs() < 1e-3, "{after:?} vs {anchor:?}");
        assert!((after.y - anchor.y).abs() < 1e-3);
        // origin shifts only by pan
        assert_eq!(zoomed.to_screen(Pos2::ZERO).to_vec2(), zoomed.pan);
        // Zooming back out returns the exact identity transform.
        let round = zoomed.zoom_at(anchor, 1.0 / 1.5);
        assert_eq!(round, layer);
    }

    #[test]
    fn zoom_clamps_and_ignores_invalid_factors() {
        let layer = Layer::default();
        let in_limit = layer.zoom_at(Vec2::ZERO, 1e6);
        assert_eq!(in_limit.scale, MAX_SCALE);
        let out_limit = layer.zoom_at(Vec2::ZERO, 1e-6);
        assert_eq!(out_limit.scale, MIN_SCALE);
        assert_eq!(
            layer.zoom_at(Vec2::ZERO, 0.0),
            layer,
            "zero factor is a no-op"
        );
        assert_eq!(layer.zoom_at(Vec2::ZERO, f32::NAN), layer);
        assert_eq!(layer.zoom_at(Vec2::ZERO, -2.0), layer);
        // A layer already sitting on the clamp must not move the anchor at all.
        let at_max = Layer {
            scale: MAX_SCALE,
            pan: Vec2::ZERO,
        };
        assert_eq!(at_max.zoom_at(Vec2::new(30.0, 40.0), 100.0), at_max);
        let at_min = Layer {
            scale: MIN_SCALE,
            pan: Vec2::ZERO,
        };
        assert_eq!(
            at_min.zoom_at(Vec2::new(30.0, 40.0), 0.001),
            at_min,
            "cannot zoom below the clamp: no anchor movement either"
        );
    }

    #[test]
    fn pan_moves_the_layer_screen_space_is_screen_delta() {
        let layer = Layer::default().pan_by(Vec2::new(15.0, -7.0));
        assert_eq!(layer.scale, 1.0, "pan never zooms");
        assert_eq!(layer.pan, Vec2::new(15.0, -7.0));
    }

    #[test]
    fn placed_rect_applies_offset_then_scale() {
        let base = Rect::from_min_size(Pos2::new(100.0, 200.0), Vec2::new(10.0, 20.0));
        let layer = Layer {
            scale: 2.0,
            pan: Vec2::new(50.0, 60.0),
        };
        let placed = placed_rect(base, Vec2::new(5.0, 3.0), layer);
        // offset 5,3 then scale ×2: (105,203) → (210,406) then +pan.
        assert_eq!(placed.min, Pos2::new(210.0 + 50.0, 406.0 + 60.0));
        assert_eq!(placed.size(), Vec2::new(20.0, 40.0));
    }

    #[test]
    fn hit_index_picks_the_first_rect_containing_the_point() {
        let r0 = Rect::from_min_size(Pos2::ZERO, Vec2::new(10.0, 10.0));
        let r1 = Rect::from_min_size(Pos2::new(20.0, 0.0), Vec2::new(10.0, 10.0));
        let overlapping = Rect::from_min_size(Pos2::new(5.0, 0.0), Vec2::new(10.0, 10.0));
        assert_eq!(
            hit_index(Pos2::new(5.0, 5.0), &[r0, overlapping, r1]),
            Some(0),
            "first match wins: paint order = z order = picking order"
        );
        assert_eq!(hit_index(Pos2::new(25.0, 5.0), &[r0, r1]), Some(1));
        assert_eq!(
            hit_index(Pos2::new(15.0, 5.0), &[r0, r1, overlapping]),
            Some(2)
        );
        assert_eq!(hit_index(Pos2::new(999.0, 999.0), &[r0, r1]), None);
        let far_rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(1000.0, 800.0));
        assert_eq!(hit_index(far_rect.min, &[]), None);
    }

    #[test]
    fn canvas_delta_follows_the_current_zoom() {
        assert_eq!(
            canvas_delta(Vec2::new(40.0, 20.0), 2.0),
            Vec2::new(20.0, 10.0)
        );
        assert_eq!(
            canvas_delta(Vec2::new(30.0, 15.0), 1.0),
            Vec2::new(30.0, 15.0)
        );
        // At very small scale a screen drag becomes a big canvas drag.
        let sharp = canvas_delta(Vec2::new(1.0, 1.0), MIN_SCALE);
        assert!(sharp.x > 1.0 && sharp.y > 1.0);
    }

    #[test]
    fn beyond_tolerance_is_2d() {
        assert!(!beyond_tolerance(Vec2::ZERO, CLICK_TOLERANCE));
        assert!(
            !beyond_tolerance(Vec2::new(5.0, 0.0), 6.0),
            "5px is inside the 6px click zone"
        );
        assert!(
            beyond_tolerance(Vec2::new(5.0, 5.0), 6.0),
            "~7.07px is outside"
        );
        assert!(
            !beyond_tolerance(Vec2::new(6.0, 0.0), 6.0),
            "exactly on tolerance still counts as a click"
        );
        assert!(beyond_tolerance(Vec2::new(0.0, 7.0), 6.0));
    }

    #[test]
    fn wheel_factor_scales_with_input_and_is_symmetric() {
        assert_eq!(wheel_factor(0.0), 1.0);
        let tick = wheel_factor(WHEEL_TICK_PX);
        assert!((tick - (1.0 + WHEEL_STEP)).abs() < 1e-6, "one notch in");
        assert!(
            (wheel_factor(-WHEEL_TICK_PX) - 1.0 / tick).abs() < 1e-6,
            "one notch out"
        );
        assert!(
            (wheel_factor(WHEEL_TICK_PX * 2.0) - tick * tick).abs() < 1e-5,
            "composes"
        );
        assert!((wheel_factor(12.5) - (1.0 + WHEEL_STEP).powf(12.5 / WHEEL_TICK_PX)).abs() < 1e-5);
    }

    #[test]
    fn state_round_trips_through_egui_memory() {
        // The pattern used by every wired panel: load → mutate → store → load.
        // It is exercised here against the real `egui::IdTypeMap` (no window needed).
        let id = egui::Id::new("diagrams-interact-test");
        let ctx = egui::Context::default();
        let state = (0..3)
            .map(|i| (format!("node{i}"), vec2(1.0 + i as f32, 2.0)))
            .collect::<BTreeMap<_, _>>();
        let canvas = CanvasState {
            layer: Layer {
                scale: 1.5,
                pan: vec2(9.0, -3.0),
            },
            offsets: state.clone(),
            group_offsets: BTreeMap::from([("grp".to_owned(), vec2(4.0, 5.0))]),
            dragging: None,
        };
        canvas.store(&ctx, id);
        let loaded = CanvasState::load(&ctx, id);
        assert_eq!(loaded, canvas);
        assert_eq!(loaded.node_offset("node0"), vec2(1.0, 2.0));
        assert_eq!(
            loaded.node_offset("never"),
            Vec2::ZERO,
            "unmoved nodes read as zero"
        );
        assert_eq!(
            loaded.group_offset("grp"),
            vec2(4.0, 5.0),
            "group offsets persist beside node offsets"
        );
        assert_eq!(loaded.group_offset("never"), Vec2::ZERO);
        // A different id is untouched state (per-panel separation).
        assert_eq!(
            CanvasState {
                layer: Layer::default(),
                offsets: BTreeMap::new(),
                group_offsets: BTreeMap::new(),
                dragging: None,
            },
            CanvasState::load(&ctx, egui::Id::new("diagrams-interact-test-other"))
        );
    }

    #[test]
    fn reset_returns_the_canonical_layout_but_keeps_nothing_stale() {
        let mut state = CanvasState {
            layer: Layer {
                scale: 2.5,
                pan: vec2(80.0, 40.0),
            },
            offsets: BTreeMap::new(),
            group_offsets: BTreeMap::new(),
            dragging: None,
        };
        state.offsets.insert("n1".into(), vec2(12.0, 6.0));
        state.group_offsets.insert("g1".into(), vec2(3.0, 3.0));
        state.dragging = Some(DragTarget::Group("g1".into()));
        state.reset();
        assert_eq!(state.layer, Layer::default());
        assert!(state.offsets.is_empty(), "no stale offsets survive a reset");
        assert!(
            state.group_offsets.is_empty(),
            "no stale group offsets survive a reset"
        );
        assert!(!state.is_dragging());
    }
}
