//! Editor tab strip + breadcrumbs: the VS Code-shell window into the codebase.
//! OWNER: integrator (card B5-04). A [`TabView`] is one open document; the
//! strip renders them, and [`TabView::title`] names what a tab shows so the
//! strip, the status bar and the palette all agree.

use eframe::egui;

use crate::app::{App, Mode, Selection};
use crate::diagrams::DiagramsTab;
use crate::theme;

/// One open document. `file` distinguishes a file tab (`Some(path)`) from a
/// repository-level view (`None`), and `mode` is how that document is being
/// read (editor, call graph, flow trace, diagrams).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct TabView {
    pub file: Option<String>,
    pub mode: Mode,
    /// Remembered only while `mode == Diagrams`; switching back should return
    /// the reader to the diagram they were looking at, not sub-tab 0.
    pub diagrams: DiagramsTab,
}

impl Default for TabView {
    fn default() -> Self {
        Self {
            file: None,
            mode: Mode::Text,
            diagrams: DiagramsTab::Control,
        }
    }
}

impl TabView {
    /// What this tab is called. A file tab shows the file's basename; a
    /// repository view shows the view name — `Call graph`, `Flow trace`,
    /// `Diagrams · Types`, and so on.
    pub fn title(&self) -> String {
        match &self.file {
            Some(path) => path.rsplit('/').next().unwrap_or(path).to_string(),
            None => match self.mode {
                Mode::Text => "Editor".into(),
                Mode::Graph => "Call graph".into(),
                Mode::Flow => "Flow trace".into(),
                Mode::Diagrams => format!("Diagrams · {}", diagram_label(self.diagrams)),
            },
        }
    }

    /// The breadcrumb parts this tab contributes when it is the active one.
    pub fn crumb(&self) -> Vec<String> {
        match &self.file {
            Some(path) => path.split('/').map(str::to_string).collect(),
            None => vec![self.title()],
        }
    }
}

/// Human names for the diagram sub-tabs, shared with the tab strip and the
/// breadcrumbs so one wording shows everywhere.
pub fn diagram_label(tab: DiagramsTab) -> &'static str {
    match tab {
        DiagramsTab::Control => "Control flow",
        DiagramsTab::Types => "Types",
        DiagramsTab::Modules => "Modules",
        DiagramsTab::Directory => "Directory",
        DiagramsTab::Files => "File links",
    }
}

// ── App-side helpers (kept here so tab semantics live in one file) ──────────

impl App {
    /// Open `file` in the tab strip: an already-open file is activated; when
    /// the active tab is a repository view a new tab is appended, and when it
    /// is a file tab the active tab is reused (VS Code's preview-tab reuse).
    pub fn open_in_tabs(&mut self, file: String) {
        if let Some(hit) = self
            .tabs
            .iter()
            .position(|t| t.file.as_deref() == Some(&file))
        {
            self.activate_tab(hit);
            return;
        }
        let reuse = self.tabs[self.active_tab].file.is_none();
        let insert_at = if reuse {
            self.active_tab
        } else {
            self.tabs.len()
        };
        self.tabs.insert(
            insert_at,
            TabView {
                file: Some(file),
                mode: Mode::Text,
                ..TabView::default()
            },
        );
        self.activate_tab(insert_at);
    }

    /// Make tab `idx` the active one and sync the rest of the app to it (the
    /// mode, the open file, and the remembered diagram sub-tab).
    pub fn activate_tab(&mut self, idx: usize) {
        let idx = idx.min(self.tabs.len().saturating_sub(1));
        self.active_tab = idx;
        let tab = self.tabs[idx].clone();
        self.mode = tab.mode;
        if tab.mode == Mode::Text {
            // Route through `open_file_with`: setting the pointer alone left
            // the buffer unloaded, so switching to a file tab showed the
            // editor's empty hint.
            if let Some(file) = tab.file.clone() {
                self.open_file_with(file, None);
            } else {
                self.open_file = None;
            }
        }
        if tab.mode == Mode::Diagrams {
            self.diagrams.tab = tab.diagrams;
        }
        self.jump_line = None;
    }

    /// Close a tab; the neighbour tab (or the last remaining one) becomes
    /// active, so the reader never lands on an empty strip.
    pub fn close_tab(&mut self, idx: usize) {
        if self.tabs.is_empty() {
            return;
        }
        if self.tabs.len() == 1 {
            // The strip never empties: closing the last tab restores the
            // default repository view the app starts with (this used to
            // index an empty Vec and panic).
            self.tabs[0] = TabView::default();
            self.activate_tab(0);
            return;
        }
        if idx >= self.tabs.len() {
            return;
        }
        self.tabs.remove(idx);
        let next = self.active_tab.min(self.tabs.len().saturating_sub(1));
        self.activate_tab(next);
    }
}

/// The horizontal strip of open documents, VS Code style: the active tab gets
/// an accent line on top, unsaved edits show a dirty dot, and every tab is one
/// click target.
pub fn tab_strip(ui: &mut egui::Ui, app: &mut App) {
    // Dynamic: with more tabs than fit, the strip scrolls sideways instead of
    // pushing tabs off the window edge where they can't be reached.
    //
    // `auto_shrink([false, true])`: fill the width, but NEVER take more height
    // than the tabs themselves. A bare `.auto_shrink(false)` here made the
    // strip claim the whole central pane's height, pushing FILES/editor/
    // EXPLAIN — and every other mode's view — below the clip rect (the
    // "nothing shows up in the view area" bug).
    egui::ScrollArea::horizontal()
        .auto_shrink([false, true])
        .max_height(theme::TAB_HEIGHT)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = theme::SPACE_1;
                ui.set_min_height(theme::TAB_HEIGHT);
                let tabs = app.tabs.clone();
                for (i, tab) in tabs.iter().enumerate() {
                    let active = i == app.active_tab;
                    let dirty = tab
                        .file
                        .as_deref()
                        .is_some_and(|f| app.dirty_edits.contains(f));
                    let title = tab.title();
                    let name_color = if active {
                        theme::TEXT_PRIMARY
                    } else {
                        theme::TEXT_SECONDARY
                    };

                    // File-type icon, then the title, then the close affordance —
                    // dirty shows the filled dot (VS Code swaps × ↔ dot), not a
                    // "*" suffix. Every part is its own hover+click target.
                    match &tab.file {
                        Some(f) => {
                            crate::icons::file_mark(ui, f, 13.0);
                        }
                        None => {
                            let icon = match tab.mode {
                                crate::app::Mode::Graph => crate::icons::GRAPH,
                                crate::app::Mode::Flow => crate::icons::ARROW_RIGHT,
                                crate::app::Mode::Diagrams => crate::icons::BEAKER,
                                crate::app::Mode::Text => crate::icons::FILE,
                            };
                            ui.label(crate::icons::glyph(icon, 13.0).color(if active {
                                theme::ACCENT
                            } else {
                                theme::TEXT_MUTED
                            }));
                        }
                    }

                    let text = if active {
                        egui::RichText::new(&title).color(name_color).strong()
                    } else {
                        egui::RichText::new(&title).color(name_color)
                    };
                    let resp = ui.add(egui::Button::new(text).frame(false)).on_hover_text(
                        match (&tab.file, dirty) {
                            (Some(f), true) => format!("{f} (unsaved edits)"),
                            (Some(f), false) => f.clone(),
                            (None, _) => title.clone(),
                        },
                    );
                    if resp.clicked() {
                        app.activate_tab(i);
                    }
                    // Keep the active tab reachable: scroll it into view when it
                    // changes (scroll_to_me is a no-op when it's already visible).
                    if active {
                        resp.scroll_to_me(Some(egui::Align::Center));
                    }

                    if tab.file.is_some() || dirty {
                        let (mark, hover) = if dirty {
                            (
                                crate::icons::CIRCLE_FILLED,
                                format!("{title} — unsaved edits"),
                            )
                        } else {
                            (crate::icons::CLOSE, format!("Close {title}"))
                        };
                        let close = ui
                            .add(
                                egui::Button::new(crate::icons::glyph(mark, 11.0).color(
                                    if dirty {
                                        theme::ACCENT
                                    } else {
                                        theme::TEXT_MUTED
                                    },
                                ))
                                .frame(false),
                            )
                            .on_hover_text(hover);
                        if close.clicked() {
                            // Dirty tabs must survive the trip: ask first.
                            app.request_close_tab(i);
                        }
                    }

                    if active {
                        let rect = resp.rect;
                        let top = egui::Rect::from_min_max(
                            rect.min,
                            egui::pos2(rect.max.x, rect.min.y + 2.0),
                        );
                        ui.painter()
                            .rect_filled(top, egui::CornerRadius::ZERO, theme::ACCENT);
                    }
                }
            });
        });
    theme::separator(ui);
}

/// Breadcrumbs under the tab strip: for a file tab `dir › dir › file.rs` plus
/// the selected symbol; for a repository view, just the view name.
pub fn breadcrumbs(ui: &mut egui::Ui, app: &App) {
    let Some(tab) = app.tabs.get(app.active_tab) else {
        return;
    };

    let mut parts = tab.crumb();
    if let (Some(Selection::Symbol(id)), Some(idx)) = (&app.selection, &app.index) {
        if let Some(sym) = idx.symbols.iter().find(|s| &s.id == id) {
            parts.push(sym.name.clone());
        }
    }

    // Dynamic: the joined crumb must respect the pane it lives in — a deep
    // path collapses its middle (keeps the first segment and the file name,
    // the two things you actually need) instead of running off the edge.
    let budget = (ui.available_width() - 2.0 * theme::SPACE_3).max(80.0);
    let font_id = egui::FontId::new(theme::FONT_SMALL, egui::FontFamily::Proportional);
    let width_of = |s: &str| {
        ui.painter()
            .layout_no_wrap(s.to_owned(), font_id.clone(), theme::TEXT_MUTED)
            .size()
            .x
    };
    let mut crumb = parts.join(" › ");
    if parts.len() > 3 && width_of(&crumb) > budget {
        let first = &parts[0];
        let last = &parts[parts.len() - 1];
        crumb = format!("{first} › … › {last}");
    }

    ui.label(
        egui::RichText::new(crumb)
            .size(theme::FONT_SMALL)
            .color(theme::TEXT_MUTED),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_tab_titles_and_breadcrumbs_with_the_real_name() {
        let t = TabView {
            file: Some("crates/gui/src/app.rs".into()),
            ..TabView::default()
        };
        assert_eq!(t.title(), "app.rs");
        assert_eq!(t.crumb(), ["crates", "gui", "src", "app.rs"]);
    }

    #[test]
    fn view_tabs_title_by_mode_and_keep_their_diagram() {
        let t = TabView {
            mode: Mode::Graph,
            ..TabView::default()
        };
        assert_eq!(t.title(), "Call graph");
        let mut t = TabView {
            mode: Mode::Diagrams,
            diagrams: DiagramsTab::Types,
            ..TabView::default()
        };
        t.diagrams = DiagramsTab::Modules;
        assert_eq!(t.title(), "Diagrams · Modules");
        assert_eq!(t.crumb(), ["Diagrams · Modules"]);
    }
}
