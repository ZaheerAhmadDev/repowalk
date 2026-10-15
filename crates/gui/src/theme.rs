//! Central design-token system. Every color, spacing, and type choice in the
//! GUI comes from here — hardcoded hexes outside this module are a defect.
//! Tokens follow an HCI brief: 4pt spacing grid, cool-neutral 5-layer dark
//! elevation, WCAG-checked contrast. Syntax colors come from the proven
//! `egui_code_editor::ColorTheme` (AYU_DARK) — not redefined here.
use eframe::egui::{self, Color32, CornerRadius, FontFamily, FontId, Margin, Stroke, TextStyle};

// ── Elevation (dark mode: higher elevation = slightly lighter) ──────────────
pub const BG_BASE: Color32 = Color32::from_rgb(0x0F, 0x11, 0x15);
pub const SURFACE_E0: Color32 = Color32::from_rgb(0x14, 0x16, 0x1B);
pub const SURFACE_E1: Color32 = Color32::from_rgb(0x18, 0x1B, 0x21);
pub const SURFACE_E2: Color32 = Color32::from_rgb(0x1D, 0x21, 0x28);
pub const SURFACE_E3: Color32 = Color32::from_rgb(0x23, 0x28, 0x30);
pub const SURFACE_HOVER: Color32 = Color32::from_rgb(0x2A, 0x30, 0x3A);
pub const SURFACE_SELECTED: Color32 = Color32::from_rgb(0x1E, 0x3A, 0x5F);

// ── Borders ─────────────────────────────────────────────────────────────────
pub const BORDER_SUBTLE: Color32 = Color32::from_rgb(0x26, 0x2B, 0x33);
pub const BORDER_STRONG: Color32 = Color32::from_rgb(0x34, 0x3B, 0x47);

// ── Text ────────────────────────────────────────────────────────────────────
pub const TEXT_PRIMARY: Color32 = Color32::from_rgb(0xE6, 0xE9, 0xEF);
pub const TEXT_SECONDARY: Color32 = Color32::from_rgb(0xA8, 0xB0, 0xBD);
pub const TEXT_MUTED: Color32 = Color32::from_rgb(0x8A, 0x93, 0xA3);
pub const TEXT_DISABLED: Color32 = Color32::from_rgb(0x5A, 0x62, 0x70);

// ── Accent & semantics ──────────────────────────────────────────────────────
pub const ACCENT: Color32 = Color32::from_rgb(0x4C, 0x8D, 0xFF);
pub const ACCENT_PRESSED: Color32 = Color32::from_rgb(0x3A, 0x76, 0xDB);
pub const SUCCESS: Color32 = Color32::from_rgb(0x3F, 0xB9, 0x50);
pub const WARNING: Color32 = Color32::from_rgb(0xE3, 0xB3, 0x41);
pub const DANGER: Color32 = Color32::from_rgb(0xF8, 0x51, 0x49);
pub const INFO: Color32 = Color32::from_rgb(0x58, 0xA6, 0xFF);

// ── Module palette (deterministic 8-color, plan §4.4) ──────────────────────
pub const MODULE_PALETTE: [Color32; 8] = [
    Color32::from_rgb(0x61, 0xAF, 0xEF),
    Color32::from_rgb(0x98, 0xC3, 0x79),
    Color32::from_rgb(0xE5, 0xC0, 0x7B),
    Color32::from_rgb(0xE0, 0x6C, 0x75),
    Color32::from_rgb(0xC6, 0x78, 0xDD),
    Color32::from_rgb(0x56, 0xB6, 0xC2),
    Color32::from_rgb(0xD1, 0x9A, 0x66),
    Color32::from_rgb(0xAB, 0xB2, 0xBF),
];

// ── Spacing (4pt base, 8pt rhythm) ─────────────────────────────────────────
pub const SPACE_1: f32 = 4.0;
pub const SPACE_2: f32 = 8.0;
pub const SPACE_3: f32 = 12.0;
pub const SPACE_4: f32 = 16.0;
pub const SPACE_6: f32 = 24.0;
pub const SPACE_8: f32 = 32.0;

// ── Fixed metrics ───────────────────────────────────────────────────────────
pub const ROW_HEIGHT: f32 = 24.0;
pub const TOOLBAR_HEIGHT: f32 = 36.0;
pub const STATUS_BAR_HEIGHT: f32 = 26.0;
pub const SIDEBAR_DEFAULT: f32 = 272.0;
pub const SIDEBAR_MIN: f32 = 180.0;
pub const SIDEBAR_MAX: f32 = 480.0;
pub const TREE_INDENT: f32 = 14.0;
pub const CORNER_SM: u8 = 3;
pub const CORNER_MD: u8 = 4;

// ── Type scale ──────────────────────────────────────────────────────────────
pub const FONT_SMALL: f32 = 11.0;
pub const FONT_BODY: f32 = 13.0;
pub const FONT_BUTTON: f32 = 13.0;
pub const FONT_HEADING: f32 = 17.0;
pub const FONT_CODE: f32 = 13.0;

// ── Motion (100ms grid, HCI-derived) ────────────────────────────────────────
// Animation is never the only carrier of information — static fallbacks stay
// readable (WCAG 2.3.3): the stage strip, edge labels, and selection states
// must all remain legible when durations are honored at zero / suppressed by
// a reduced-motion preference.
// `allow(dead_code)`: tokens are pinned for dynamic views (flow playback,
// edge pulses) landing in follow-up tasks; consumers must reference these
// names rather than hand-roll durations.

/// Hover/selection transitions.
#[allow(dead_code)]
pub const TRANSITION_MS: u64 = 200;
/// Edge-pulse loop.
#[allow(dead_code)]
pub const PULSE_MS: u64 = 600;
/// Flow stage dwell per step.
#[allow(dead_code)]
pub const STEP_DWELL_MS: u64 = 800;

/// Smooth 0→1 easing for playhead motion: t in [0,1] -> eased in [0,1].
/// Clamped smoothstep; panics-free, allocation-free.
#[allow(dead_code)]
pub fn ease_in_out(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Alpha wash for the active flow stage: full at progress 0.5, softer at edges.
/// Clamped to [0,1]; panics-free, allocation-free.
#[allow(dead_code)]
pub fn step_alpha(progress: f32) -> f32 {
    let p = progress.clamp(0.0, 1.0);
    (0.15 + 0.25 * (1.0 - (p * 2.0 - 1.0).abs())).clamp(0.0, 1.0)
}

/// Deterministic module → palette color (stable across runs; plan §4.4).
pub fn module_color(module: &str) -> Color32 {
    let mut h: u64 = 5381;
    for b in module.bytes() {
        h = h.wrapping_mul(33).wrapping_add(b as u64);
    }
    MODULE_PALETTE[(h % MODULE_PALETTE.len() as u64) as usize]
}

/// Apply the full token set to the egui context. Called once from `App::new`.
pub fn apply(ctx: &egui::Context) {
    ctx.set_theme(egui::Theme::Dark);

    let mut style = (*ctx.style_of(egui::Theme::Dark)).clone();

    style.text_styles = [
        (
            TextStyle::Small,
            FontId::new(FONT_SMALL, FontFamily::Proportional),
        ),
        (
            TextStyle::Body,
            FontId::new(FONT_BODY, FontFamily::Proportional),
        ),
        (
            TextStyle::Button,
            FontId::new(FONT_BUTTON, FontFamily::Proportional),
        ),
        (
            TextStyle::Heading,
            FontId::new(FONT_HEADING, FontFamily::Proportional),
        ),
        (
            TextStyle::Monospace,
            FontId::new(FONT_CODE, FontFamily::Monospace),
        ),
    ]
    .into();

    style.spacing.item_spacing = egui::vec2(SPACE_2, SPACE_2);
    style.spacing.button_padding = egui::vec2(SPACE_3, SPACE_1 + 1.0);
    style.spacing.menu_margin = Margin::symmetric(SPACE_2 as i8, SPACE_2 as i8);
    style.spacing.indent = TREE_INDENT;
    style.spacing.scroll.bar_width = 8.0;
    style.spacing.scroll.bar_outer_margin = 2.0;
    style.spacing.scroll.bar_inner_margin = 2.0;

    let v = &mut style.visuals;
    v.dark_mode = true;

    v.panel_fill = SURFACE_E1;
    v.window_fill = SURFACE_E3;
    v.extreme_bg_color = SURFACE_E2;
    v.faint_bg_color = SURFACE_E0;
    v.code_bg_color = SURFACE_E2;

    v.override_text_color = Some(TEXT_PRIMARY);
    v.weak_text_color = Some(TEXT_MUTED);
    v.warn_fg_color = WARNING;
    v.error_fg_color = DANGER;

    let corner = CornerRadius::same(CORNER_SM);
    let border = Stroke::new(1.0, BORDER_SUBTLE);

    v.widgets.noninteractive.bg_fill = SURFACE_E1;
    v.widgets.noninteractive.weak_bg_fill = SURFACE_E1;
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, TEXT_SECONDARY);
    v.widgets.noninteractive.bg_stroke = border;
    v.widgets.noninteractive.corner_radius = corner;

    v.widgets.inactive.bg_fill = SURFACE_E2;
    v.widgets.inactive.weak_bg_fill = SURFACE_E2;
    v.widgets.inactive.fg_stroke = Stroke::new(1.0, TEXT_SECONDARY);
    v.widgets.inactive.bg_stroke = border;
    v.widgets.inactive.corner_radius = corner;

    v.widgets.hovered.bg_fill = SURFACE_HOVER;
    v.widgets.hovered.weak_bg_fill = SURFACE_HOVER;
    v.widgets.hovered.fg_stroke = Stroke::new(1.0, TEXT_PRIMARY);
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, BORDER_STRONG);
    v.widgets.hovered.corner_radius = corner;

    v.widgets.active.bg_fill = ACCENT_PRESSED;
    v.widgets.active.weak_bg_fill = ACCENT_PRESSED;
    v.widgets.active.fg_stroke = Stroke::new(1.0, Color32::WHITE);
    v.widgets.active.bg_stroke = Stroke::new(1.0, ACCENT);
    v.widgets.active.corner_radius = corner;

    v.widgets.open.bg_fill = SURFACE_E3;
    v.widgets.open.weak_bg_fill = SURFACE_E3;
    v.widgets.open.fg_stroke = Stroke::new(1.0, TEXT_PRIMARY);
    v.widgets.open.bg_stroke = Stroke::new(1.0, BORDER_STRONG);
    v.widgets.open.corner_radius = corner;

    v.selection.bg_fill = SURFACE_SELECTED;
    v.selection.stroke = Stroke::new(1.0, ACCENT);
    v.slider_trailing_fill = true;
    v.striped = true;
    v.hyperlink_color = ACCENT;

    v.window_corner_radius = CornerRadius::same(CORNER_MD);
    v.menu_corner_radius = CornerRadius::same(CORNER_MD);
    v.window_stroke = Stroke::new(1.0, BORDER_STRONG);
    v.popup_shadow = egui::epaint::Shadow::NONE;
    v.window_highlight_topmost = false;

    // Scroll bars: solid, thin, derived from widget visuals (egui 0.36 ScrollStyle).
    style.spacing.scroll.floating = false;
    style.spacing.scroll.foreground_color = true;

    ctx.set_global_style(style);
}

/// Alpha-blended accent wash (toggle-on fills, selected-row tint).
pub fn accent_subtle(alpha: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(ACCENT.r(), ACCENT.g(), ACCENT.b(), alpha)
}

/// Muted neutral for dimmed/de-emphasized graph elements. Single source so the
/// "receded" state is one token, not scattered `from_rgba_unmultiplied` calls.
pub const DIM: Color32 = Color32::from_rgb(90, 95, 105);

/// `DIM` at a given alpha — for dimmed node fills, edges, and halos.
pub fn dim_alpha(alpha: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(DIM.r(), DIM.g(), DIM.b(), alpha)
}

/// Any theme color at a given alpha (edges at 55%, halos, washes).
pub fn with_alpha(c: Color32, alpha: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), alpha)
}

/// Small uppercase section header ("MAP", "HEALTH", …). Gestalt: common region
/// via consistent styling, not heavy boxes.
pub fn section_header(ui: &mut egui::Ui, text: &str) {
    ui.add_space(SPACE_1);
    ui.colored_label(
        TEXT_MUTED,
        egui::RichText::new(text.to_uppercase())
            .size(FONT_SMALL)
            .strong(),
    );
    ui.add_space(SPACE_1);
}

/// Hairline separator with breathing room.
pub fn separator(ui: &mut egui::Ui) {
    ui.add_space(SPACE_1);
    ui.separator();
    ui.add_space(SPACE_1);
}

/// Paint a full-width row background for the given rect (hover / selection).
pub fn paint_row_bg(ui: &egui::Ui, rect: egui::Rect, selected: bool, hovered: bool) {
    if !selected && !hovered {
        return;
    }
    let fill = if selected {
        SURFACE_SELECTED
    } else {
        SURFACE_HOVER
    };
    ui.painter()
        .rect_filled(rect, CornerRadius::same(CORNER_SM), fill);
}

/// A clickable full-width list row with hover/selection visuals and
/// horizontally laid-out contents. Returns the click response.
pub fn list_row(
    ui: &mut egui::Ui,
    selected: bool,
    id_salt: impl std::hash::Hash + std::fmt::Debug,
    add_contents: impl FnOnce(&mut egui::Ui),
) -> egui::Response {
    let width = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(width, ROW_HEIGHT), egui::Sense::click());
    paint_row_bg(ui, rect, selected, resp.hovered());

    let inner = rect.shrink2(egui::vec2(SPACE_2, 0.0));
    ui.scope_builder(
        egui::UiBuilder::new()
            .id_salt(id_salt)
            .max_rect(inner)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
        |ui| {
            ui.spacing_mut().item_spacing.x = SPACE_1;
            add_contents(ui);
        },
    );
    resp
}
