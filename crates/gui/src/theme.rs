//! Central design-token system. Every color, spacing, and type choice in the
//! GUI comes from here — hardcoded hexes outside this module are a defect.
//! Tokens follow an HCI brief: 4pt spacing grid, cool-neutral 5-layer dark
//! elevation, WCAG-checked contrast. Syntax colors come from the proven
//! `egui_code_editor::ColorTheme` (AYU_DARK) — not redefined here.
use eframe::egui::{self, Color32, CornerRadius, FontFamily, FontId, Margin, Stroke, TextStyle};

// ── Palette (F13) — the role colours every token below aliases ──────────────
// A [`Palette`] is the single source of truth for one theme. The named `pub
// const`s further down are thin aliases of [`DARK`], so every existing caller
// keeps working byte-for-byte; [`apply_theme`] swaps in [`LIGHT`] at runtime.
//
// Contrast (WCAG 2.1 relative luminance over sRGB, computed in
// `palette::tests`): LIGHT targets AAA body text (>= 7:1) for primary and
// secondary text on the surface, AA (>= 4.5:1) for muted text, and AAA for
// the accent as link text on the background. DARK is unchanged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Palette {
    /// App background behind panels.
    pub background: Color32,
    /// Primary elevated surface (panel fill); the elevation E1 layer.
    pub surface: Color32,
    /// Slightly darker than `surface` (toolbar strips, faint fills): E0.
    pub surface_e0: Color32,
    /// One step up from `surface` (insets, extreme bg): E2.
    pub surface_e2: Color32,
    /// Topmost layer (windows, open menus): E3.
    pub surface_e3: Color32,
    /// Hovered rows / widgets.
    pub surface_hover: Color32,
    /// Selected rows / ranges.
    pub surface_selected: Color32,
    /// Hairline borders (subtle).
    pub border: Color32,
    /// Stronger separators (window/menu outlines, hover strokes).
    pub border_strong: Color32,
    /// Primary body text.
    pub text: Color32,
    /// Secondary text (labels, captions).
    pub text_secondary: Color32,
    /// Muted text (section headers, hints).
    pub text_muted: Color32,
    /// Disabled text.
    pub text_disabled: Color32,
    /// Interactive accent (links, selection, focus).
    pub accent: Color32,
    /// Pressed / active accent.
    pub accent_pressed: Color32,
    /// Semantic: success.
    pub success: Color32,
    /// Semantic: warning.
    pub warning: Color32,
    /// Semantic: danger / error.
    pub danger: Color32,
    /// Semantic: informational.
    pub info: Color32,
    /// Receded neutral for dimmed graph chrome.
    pub dim: Color32,
}

/// The existing dark look, verbatim — the source of truth for the aliases
/// below. Changing any value here changes the dark theme everywhere.
pub const DARK: Palette = Palette {
    background: Color32::from_rgb(0x0F, 0x11, 0x15),
    surface: Color32::from_rgb(0x18, 0x1B, 0x21),
    surface_e0: Color32::from_rgb(0x14, 0x16, 0x1B),
    surface_e2: Color32::from_rgb(0x1D, 0x21, 0x28),
    surface_e3: Color32::from_rgb(0x23, 0x28, 0x30),
    surface_hover: Color32::from_rgb(0x2A, 0x30, 0x3A),
    surface_selected: Color32::from_rgb(0x1E, 0x3A, 0x5F),
    border: Color32::from_rgb(0x26, 0x2B, 0x33),
    border_strong: Color32::from_rgb(0x34, 0x3B, 0x47),
    text: Color32::from_rgb(0xE6, 0xE9, 0xEF),
    text_secondary: Color32::from_rgb(0xA8, 0xB0, 0xBD),
    text_muted: Color32::from_rgb(0x8A, 0x93, 0xA3),
    text_disabled: Color32::from_rgb(0x5A, 0x62, 0x70),
    accent: Color32::from_rgb(0x4C, 0x8D, 0xFF),
    accent_pressed: Color32::from_rgb(0x3A, 0x76, 0xDB),
    success: Color32::from_rgb(0x3F, 0xB9, 0x50),
    warning: Color32::from_rgb(0xE3, 0xB3, 0x41),
    danger: Color32::from_rgb(0xF8, 0x51, 0x49),
    info: Color32::from_rgb(0x58, 0xA6, 0xFF),
    dim: Color32::from_rgb(90, 95, 105),
};

/// The light look (F13). Inverted surfaces, darkened accent/semantics so text
/// stays legible on white; the ratios are asserted in the module tests.
pub const LIGHT: Palette = Palette {
    background: Color32::from_rgb(0xFF, 0xFF, 0xFF),
    surface: Color32::from_rgb(0xF5, 0xF6, 0xF8),
    surface_e0: Color32::from_rgb(0xFB, 0xFB, 0xFC),
    surface_e2: Color32::from_rgb(0xEC, 0xEE, 0xF2),
    surface_e3: Color32::from_rgb(0xFF, 0xFF, 0xFF),
    surface_hover: Color32::from_rgb(0xE3, 0xE7, 0xEE),
    surface_selected: Color32::from_rgb(0xD6, 0xE4, 0xFF),
    border: Color32::from_rgb(0xD6, 0xDA, 0xE0),
    border_strong: Color32::from_rgb(0xB4, 0xBA, 0xC4),
    text: Color32::from_rgb(0x1A, 0x1D, 0x23),
    text_secondary: Color32::from_rgb(0x4A, 0x51, 0x5C),
    text_muted: Color32::from_rgb(0x5B, 0x64, 0x72),
    text_disabled: Color32::from_rgb(0x9A, 0xA1, 0xAC),
    accent: Color32::from_rgb(0x0D, 0x4A, 0xBF),
    accent_pressed: Color32::from_rgb(0x0A, 0x3A, 0x99),
    success: Color32::from_rgb(0x1E, 0x7A, 0x34),
    warning: Color32::from_rgb(0x8A, 0x61, 0x00),
    danger: Color32::from_rgb(0xB3, 0x26, 0x1E),
    info: Color32::from_rgb(0x0B, 0x5F, 0xA5),
    dim: Color32::from_rgb(0x6B, 0x72, 0x80),
};

/// Which palette [`apply_theme`] installs. Defaults to `Dark` so an app that
/// never asks for a mode keeps today's look.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ThemeMode {
    /// The original dark theme (default).
    #[default]
    Dark,
    /// The light theme (F13). Not constructed in-tree until the integrator
    /// wires the toggle into `app.rs` (handoff to PMO); `apply_theme` and the
    /// tests can select it right now.
    #[allow(dead_code)]
    Light,
}

impl ThemeMode {
    /// The palette this mode drives.
    pub fn palette(self) -> &'static Palette {
        match self {
            ThemeMode::Dark => &DARK,
            ThemeMode::Light => &LIGHT,
        }
    }
}

// ── Elevation (dark mode: higher elevation = slightly lighter) ──────────────
// Aliases of [`DARK`]; `apply_theme` reads the active palette instead.
pub const BG_BASE: Color32 = DARK.background;
pub const SURFACE_E0: Color32 = DARK.surface_e0;
pub const SURFACE_E1: Color32 = DARK.surface;
pub const SURFACE_E2: Color32 = DARK.surface_e2;
pub const SURFACE_E3: Color32 = DARK.surface_e3;
pub const SURFACE_HOVER: Color32 = DARK.surface_hover;
pub const SURFACE_SELECTED: Color32 = DARK.surface_selected;

// ── Borders ─────────────────────────────────────────────────────────────────
pub const BORDER_SUBTLE: Color32 = DARK.border;
pub const BORDER_STRONG: Color32 = DARK.border_strong;

// ── Text ────────────────────────────────────────────────────────────────────
pub const TEXT_PRIMARY: Color32 = DARK.text;
pub const TEXT_SECONDARY: Color32 = DARK.text_secondary;
pub const TEXT_MUTED: Color32 = DARK.text_muted;
pub const TEXT_DISABLED: Color32 = DARK.text_disabled;

// ── Accent & semantics ──────────────────────────────────────────────────────
pub const ACCENT: Color32 = DARK.accent;
// `allow(dead_code)`: `apply_theme` now reads `Palette::accent_pressed`; the
// legacy alias stays pinned so existing callers and the token contract hold.
#[allow(dead_code)]
pub const ACCENT_PRESSED: Color32 = DARK.accent_pressed;
pub const SUCCESS: Color32 = DARK.success;
pub const WARNING: Color32 = DARK.warning;
pub const DANGER: Color32 = DARK.danger;
pub const INFO: Color32 = DARK.info;

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
/// Kept as the dark entry point — delegates to [`apply_theme`].
pub fn apply(ctx: &egui::Context) {
    apply_theme(ctx, ThemeMode::Dark);
}

/// Apply the token set for `mode` to the egui context. `ThemeMode::Dark`
/// reproduces the original `apply` byte-for-byte (it reads [`DARK`], whose
/// values are the legacy constants); `ThemeMode::Light` installs [`LIGHT`] with
/// the same spacing, metrics and widget layout, only the colours change.
pub fn apply_theme(ctx: &egui::Context, mode: ThemeMode) {
    let p = mode.palette();
    let theme = match mode {
        ThemeMode::Dark => egui::Theme::Dark,
        ThemeMode::Light => egui::Theme::Light,
    };

    ctx.set_theme(theme);

    let mut style = (*ctx.style_of(theme)).clone();

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
    v.dark_mode = matches!(mode, ThemeMode::Dark);

    v.panel_fill = p.surface;
    v.window_fill = p.surface_e3;
    v.extreme_bg_color = p.surface_e2;
    v.faint_bg_color = p.surface_e0;
    v.code_bg_color = p.surface_e2;

    v.override_text_color = Some(p.text);
    v.weak_text_color = Some(p.text_muted);
    v.warn_fg_color = p.warning;
    v.error_fg_color = p.danger;

    let corner = CornerRadius::same(CORNER_SM);
    let border = Stroke::new(1.0, p.border);

    v.widgets.noninteractive.bg_fill = p.surface;
    v.widgets.noninteractive.weak_bg_fill = p.surface;
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, p.text_secondary);
    v.widgets.noninteractive.bg_stroke = border;
    v.widgets.noninteractive.corner_radius = corner;

    v.widgets.inactive.bg_fill = p.surface_e2;
    v.widgets.inactive.weak_bg_fill = p.surface_e2;
    v.widgets.inactive.fg_stroke = Stroke::new(1.0, p.text_secondary);
    v.widgets.inactive.bg_stroke = border;
    v.widgets.inactive.corner_radius = corner;

    v.widgets.hovered.bg_fill = p.surface_hover;
    v.widgets.hovered.weak_bg_fill = p.surface_hover;
    v.widgets.hovered.fg_stroke = Stroke::new(1.0, p.text);
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, p.border_strong);
    v.widgets.hovered.corner_radius = corner;

    v.widgets.active.bg_fill = p.accent_pressed;
    v.widgets.active.weak_bg_fill = p.accent_pressed;
    v.widgets.active.fg_stroke = Stroke::new(1.0, Color32::WHITE);
    v.widgets.active.bg_stroke = Stroke::new(1.0, p.accent);
    v.widgets.active.corner_radius = corner;

    v.widgets.open.bg_fill = p.surface_e3;
    v.widgets.open.weak_bg_fill = p.surface_e3;
    v.widgets.open.fg_stroke = Stroke::new(1.0, p.text);
    v.widgets.open.bg_stroke = Stroke::new(1.0, p.border_strong);
    v.widgets.open.corner_radius = corner;

    v.selection.bg_fill = p.surface_selected;
    v.selection.stroke = Stroke::new(1.0, p.accent);
    v.slider_trailing_fill = true;
    v.striped = true;
    v.hyperlink_color = p.accent;

    v.window_corner_radius = CornerRadius::same(CORNER_MD);
    v.menu_corner_radius = CornerRadius::same(CORNER_MD);
    v.window_stroke = Stroke::new(1.0, p.border_strong);
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
/// Alias of [`DARK`]'s `dim`.
pub const DIM: Color32 = DARK.dim;

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

#[cfg(test)]
mod tests {
    use super::*;

    /// sRGB channel (0..255) -> linear component, per WCAG 2.1.
    fn linear(v: u8) -> f64 {
        let c = v as f64 / 255.0;
        if c <= 0.039_28 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    }

    /// WCAG 2.1 relative luminance of an sRGB colour (0.0..=1.0).
    fn luminance(c: Color32) -> f64 {
        0.2126 * linear(c.r()) + 0.7152 * linear(c.g()) + 0.0722 * linear(c.b())
    }

    /// WCAG 2.1 contrast ratio between two opaque colours (1.0..=21.0).
    fn contrast(a: Color32, b: Color32) -> f64 {
        let (la, lb) = (luminance(a), luminance(b));
        let (hi, lo) = if la >= lb { (la, lb) } else { (lb, la) };
        (hi + 0.05) / (lo + 0.05)
    }

    fn rgb(r: u8, g: u8, b: u8) -> Color32 {
        Color32::from_rgb(r, g, b)
    }

    /// (c) The dark palette *is* the pre-F13 look. Literal bytes, so a silent
    /// change to any dark role fails here.
    #[test]
    fn dark_palette_matches_legacy_literals() {
        assert_eq!(DARK.background, rgb(0x0F, 0x11, 0x15));
        assert_eq!(DARK.surface, rgb(0x18, 0x1B, 0x21));
        assert_eq!(DARK.surface_e0, rgb(0x14, 0x16, 0x1B));
        assert_eq!(DARK.surface_e2, rgb(0x1D, 0x21, 0x28));
        assert_eq!(DARK.surface_e3, rgb(0x23, 0x28, 0x30));
        assert_eq!(DARK.surface_hover, rgb(0x2A, 0x30, 0x3A));
        assert_eq!(DARK.surface_selected, rgb(0x1E, 0x3A, 0x5F));
        assert_eq!(DARK.border, rgb(0x26, 0x2B, 0x33));
        assert_eq!(DARK.border_strong, rgb(0x34, 0x3B, 0x47));
        assert_eq!(DARK.text, rgb(0xE6, 0xE9, 0xEF));
        assert_eq!(DARK.text_secondary, rgb(0xA8, 0xB0, 0xBD));
        assert_eq!(DARK.text_muted, rgb(0x8A, 0x93, 0xA3));
        assert_eq!(DARK.text_disabled, rgb(0x5A, 0x62, 0x70));
        assert_eq!(DARK.accent, rgb(0x4C, 0x8D, 0xFF));
        assert_eq!(DARK.accent_pressed, rgb(0x3A, 0x76, 0xDB));
        assert_eq!(DARK.success, rgb(0x3F, 0xB9, 0x50));
        assert_eq!(DARK.warning, rgb(0xE3, 0xB3, 0x41));
        assert_eq!(DARK.danger, rgb(0xF8, 0x51, 0x49));
        assert_eq!(DARK.info, rgb(0x58, 0xA6, 0xFF));
        assert_eq!(DARK.dim, rgb(90, 95, 105));

        // The public aliases still resolve to the dark palette: no caller
        // changes, and the dark look cannot drift from the palette.
        assert_eq!(BG_BASE, DARK.background);
        assert_eq!(SURFACE_E0, DARK.surface_e0);
        assert_eq!(SURFACE_E1, DARK.surface);
        assert_eq!(SURFACE_E2, DARK.surface_e2);
        assert_eq!(SURFACE_E3, DARK.surface_e3);
        assert_eq!(SURFACE_HOVER, DARK.surface_hover);
        assert_eq!(SURFACE_SELECTED, DARK.surface_selected);
        assert_eq!(BORDER_SUBTLE, DARK.border);
        assert_eq!(BORDER_STRONG, DARK.border_strong);
        assert_eq!(TEXT_PRIMARY, DARK.text);
        assert_eq!(TEXT_SECONDARY, DARK.text_secondary);
        assert_eq!(TEXT_MUTED, DARK.text_muted);
        assert_eq!(TEXT_DISABLED, DARK.text_disabled);
        assert_eq!(ACCENT, DARK.accent);
        assert_eq!(ACCENT_PRESSED, DARK.accent_pressed);
        assert_eq!(SUCCESS, DARK.success);
        assert_eq!(WARNING, DARK.warning);
        assert_eq!(DANGER, DARK.danger);
        assert_eq!(INFO, DARK.info);
        assert_eq!(DIM, DARK.dim);
    }

    /// (a) Roles that must differ do differ, in both palettes.
    #[test]
    fn palettes_have_distinct_roles() {
        for (name, p) in [("dark", &DARK), ("light", &LIGHT)] {
            // Five-layer elevation must not collapse.
            assert_ne!(p.surface_e0, p.surface, "{name}: E0 vs E1");
            assert_ne!(p.surface, p.surface_e2, "{name}: E1 vs E2");
            assert_ne!(p.surface_e2, p.surface_e3, "{name}: E2 vs E3");
            // Borders must be visible against the surface.
            assert_ne!(p.border, p.surface, "{name}: border vs surface");
            assert_ne!(p.border_strong, p.border, "{name}: strong vs subtle border");
            // Text ramp must be three distinct steps.
            assert_ne!(p.text, p.text_secondary, "{name}: text vs secondary");
            assert_ne!(p.text_secondary, p.text_muted, "{name}: secondary vs muted");
            // Accent must stand off the neutrals it is drawn over.
            assert_ne!(p.accent, p.background, "{name}: accent vs background");
            assert_ne!(p.accent, p.surface, "{name}: accent vs surface");
            // Semantic colours must never collide (colour is not the only cue,
            // but two identical semantics would be a defect regardless).
            for (a, b) in [
                (p.accent, p.success),
                (p.accent, p.warning),
                (p.accent, p.danger),
                (p.accent, p.info),
                (p.success, p.warning),
                (p.success, p.danger),
                (p.success, p.info),
                (p.warning, p.danger),
                (p.warning, p.info),
                (p.danger, p.info),
            ] {
                assert_ne!(a, b, "{name}: semantic role collision");
            }
        }
    }

    /// (b) Light-theme legibility, computed from the sRGB bytes.
    /// Targets: AAA (>= 7:1) primary/secondary body text on the surface and
    /// the accent as link text on the background; AA (>= 4.5:1) muted text and
    /// the semantic colours as text.
    #[test]
    fn light_palette_meets_wcag_aa_body_text() {
        let text_surface = contrast(LIGHT.text, LIGHT.surface);
        assert!(
            text_surface >= 7.0,
            "text on surface {text_surface:.2} < 7.0 (AAA)"
        );
        let text_bg = contrast(LIGHT.text, LIGHT.background);
        assert!(
            text_bg >= 7.0,
            "text on background {text_bg:.2} < 7.0 (AAA)"
        );
        let sec_surface = contrast(LIGHT.text_secondary, LIGHT.surface);
        assert!(
            sec_surface >= 7.0,
            "secondary on surface {sec_surface:.2} < 7.0 (AAA)"
        );
        let muted_surface = contrast(LIGHT.text_muted, LIGHT.surface);
        assert!(
            muted_surface >= 4.5,
            "muted on surface {muted_surface:.2} < 4.5 (AA)"
        );
        let accent_bg = contrast(LIGHT.accent, LIGHT.background);
        assert!(
            accent_bg >= 4.5,
            "accent on background {accent_bg:.2} < 4.5 (AA)"
        );
        let accent_surface = contrast(LIGHT.accent, LIGHT.surface);
        assert!(
            accent_surface >= 4.5,
            "accent on surface {accent_surface:.2} < 4.5 (AA)"
        );
        for (name, c) in [
            ("success", LIGHT.success),
            ("warning", LIGHT.warning),
            ("danger", LIGHT.danger),
            ("info", LIGHT.info),
        ] {
            let r = contrast(c, LIGHT.background);
            assert!(r >= 4.5, "{name} on background {r:.2} < 4.5 (AA)");
        }
        // Active widget label sits on the pressed accent fill.
        let on_active = contrast(Color32::WHITE, LIGHT.accent_pressed);
        assert!(
            on_active >= 4.5,
            "white on accent_pressed {on_active:.2} < 4.5"
        );
    }

    /// The mode plumbing the integrator wires up.
    #[test]
    fn theme_mode_defaults_to_dark_and_selects_palettes() {
        assert_eq!(ThemeMode::default(), ThemeMode::Dark);
        assert_eq!(*ThemeMode::Dark.palette(), DARK);
        assert_eq!(*ThemeMode::Light.palette(), LIGHT);
        assert_ne!(DARK.surface, LIGHT.surface);
        assert_ne!(DARK.text, LIGHT.text);
    }
}
