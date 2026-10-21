//! Sidebar: explorer · map · usage · health · notes · onboarding · search.
//! OWNER of this aggregate: integrator; each panel belongs to its own card.
//! With B5-04 the selector is an activity (driven by the icon strip in
//! `app.rs`), so no tab strip is drawn in here.
pub mod explorer;
pub mod flow;
pub mod map;
pub mod search;
pub mod tour;

use eframe::egui;

use crate::theme;

/// Sidebar view selector — one activity = one side-panel section. `Map` keeps
/// the v1 stacked panels under itself; Explorer and Search are VS Code-shell
/// additions (B5-04).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tab {
    #[default]
    Explorer,
    Search,
    Map,
    Usage,
    Health,
    Notes,
    Onboard,
}

/// All panels, top to bottom. Each panel owns its own `ScrollArea` sized to
/// the space that is left, so the sidebar never overflows and long lists
/// scroll in place. The selector is the app-level activity (the icon strip in
/// `app.rs`) — no tab strip is drawn here.
pub fn ui(ui: &mut egui::Ui, app: &mut crate::app::App) {
    if app.index.is_none() {
        empty_state(ui);
        return;
    }
    match app.sidebar_tab {
        // The v1 stacked panels under one activity. ONE wrapping scroll —
        // each section's own inner cap (map 50%, tour 45%) used to mean
        // that on a short window the Flow section and everything under the
        // caps was clipped and unreachable, while on a tall window the map
        // artificially stopped at 360px and left half the panel blank.
        Tab::Map => {
            egui::ScrollArea::vertical()
                .id_salt("map-stack")
                .auto_shrink(false)
                .show(ui, |ui| {
                    map::ui(ui, app);
                    theme::separator(ui);
                    tour::ui(ui, app);
                    theme::separator(ui);
                    flow::ui(ui, app);
                });
        }
        // Full-height panels for the two new activities (search used to live
        // at the bottom of Map; as its own activity it gets the whole panel).
        Tab::Explorer => explorer::ui(ui, app),
        Tab::Search => search::ui(ui, app),
        Tab::Usage => crate::usage::ui(ui, app),
        Tab::Health => crate::health::ui(ui, app),
        Tab::Notes => crate::notes::ui(ui, app),
        Tab::Onboard => crate::onboard::ui(ui, app),
    }
}

/// Muted hint shown while no repository is loaded (visibility of system status).
fn empty_state(ui: &mut egui::Ui) {
    ui.add_space(theme::SPACE_6);
    ui.vertical_centered(|ui| {
        ui.label(
            egui::RichText::new("No repository loaded")
                .size(theme::FONT_BODY)
                .color(theme::TEXT_SECONDARY),
        );
        ui.add_space(theme::SPACE_2);
        ui.label(
            egui::RichText::new("open a repo (Ctrl+K or Open…)")
                .size(theme::FONT_SMALL)
                .color(theme::TEXT_MUTED),
        );
    });
}

// ── Shared row primitives ───────────────────────────────────────────────────

/// Right-hand metadata for a [`data_row`]: muted text or an accent count pill.
#[derive(Clone, Copy)]
pub(crate) struct Meta<'a> {
    text: &'a str,
    color: egui::Color32,
    badge: bool,
}

impl<'a> Meta<'a> {
    /// Muted right-aligned text (LOC, `path:line`, caller counts…).
    pub(crate) fn muted(text: &'a str) -> Self {
        Self {
            text,
            color: theme::TEXT_MUTED,
            badge: false,
        }
    }

    /// Semantic-tinted text (god-file LOC, cycle sizes…).
    pub(crate) fn tinted(text: &'a str, color: egui::Color32) -> Self {
        Self {
            text,
            color,
            badge: false,
        }
    }

    /// Accent-tinted count pill (fan-in badges).
    pub(crate) fn badge(text: &'a str) -> Self {
        Self {
            text,
            color: theme::ACCENT,
            badge: true,
        }
    }
}

const BADGE_PAD_X: f32 = 5.0;
const BADGE_HEIGHT: f32 = 16.0;

/// One sidebar list row: `name` on the left (elided with `…`, full text on
/// hover) and optional right-aligned `meta`. Thin wrapper over
/// [`theme::list_row`] so every panel shares hover/selection visuals.
pub(crate) fn data_row(
    ui: &mut egui::Ui,
    selected: bool,
    id_salt: impl std::hash::Hash + std::fmt::Debug,
    name: impl Into<egui::WidgetText>,
    meta: Option<Meta<'_>>,
) -> egui::Response {
    data_row_marked(ui, selected, id_salt, None, name, meta)
}

/// A [`data_row`] that leads with the file's type chip, for lists of paths
/// (explorer, quick open). The chip is painted, so the name column starts at
/// the same x on every row.
pub(crate) fn file_data_row(
    ui: &mut egui::Ui,
    selected: bool,
    id_salt: impl std::hash::Hash + std::fmt::Debug,
    path: &str,
    name: impl Into<egui::WidgetText>,
    meta: Option<Meta<'_>>,
) -> egui::Response {
    data_row_marked(ui, selected, id_salt, Some(path), name, meta)
}

/// Size of the leading type chip. Rows are 24 px; 14 px leaves the chip
/// visually centred without crowding the name.
pub(crate) const FILE_MARK_SIZE: f32 = 14.0;

fn data_row_marked(
    ui: &mut egui::Ui,
    selected: bool,
    id_salt: impl std::hash::Hash + std::fmt::Debug,
    mark: Option<&str>,
    name: impl Into<egui::WidgetText>,
    meta: Option<Meta<'_>>,
) -> egui::Response {
    theme::list_row(ui, selected, id_salt, |ui| {
        let row = ui.max_rect();
        let font = egui::FontId::new(theme::FONT_SMALL, egui::FontFamily::Proportional);
        let meta_w = match meta {
            Some(m) => {
                let w = text_width(ui, m.text, font.clone());
                if m.badge {
                    w + 2.0 * BADGE_PAD_X
                } else {
                    w
                }
            }
            None => 0.0,
        };
        let gap = if meta.is_some() { theme::SPACE_2 } else { 0.0 };
        // Reserve the right edge for `meta` and the left edge for the type
        // chip, so the name elides instead of colliding.
        let mark_w = if mark.is_some() {
            FILE_MARK_SIZE * 1.15 + theme::SPACE_1
        } else {
            0.0
        };
        let name_budget = (row.width() - meta_w - gap - mark_w).max(theme::SPACE_6);

        if let Some(path) = mark {
            let resp = crate::icons::file_mark(ui, path, FILE_MARK_SIZE);
            resp.on_hover_text(path.rsplit('/').next().unwrap_or(path));
        }

        ui.allocate_ui_with_layout(
            egui::vec2(name_budget, theme::ROW_HEIGHT),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.add(egui::Label::new(name).truncate());
            },
        );

        if let Some(m) = meta {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if m.badge {
                    let w = text_width(ui, m.text, font.clone()) + 2.0 * BADGE_PAD_X;
                    let (rect, _) =
                        ui.allocate_exact_size(egui::vec2(w, BADGE_HEIGHT), egui::Sense::hover());
                    ui.painter().rect_filled(
                        rect,
                        egui::CornerRadius::same((BADGE_HEIGHT / 2.0) as u8),
                        theme::accent_subtle(48),
                    );
                    ui.painter().text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        m.text,
                        font,
                        m.color,
                    );
                } else {
                    ui.label(
                        egui::RichText::new(m.text)
                            .size(theme::FONT_SMALL)
                            .color(m.color),
                    );
                }
            });
        }
    })
}

/// Width of `text` rendered in `font` on a single line (for right-align budgets).
fn text_width(ui: &egui::Ui, text: &str, font: egui::FontId) -> f32 {
    ui.painter()
        .layout_no_wrap(text.to_string(), font, theme::TEXT_MUTED)
        .size()
        .x
}
