//! Ctrl+P quick open: a file-path switcher overlay, VS Code style.
//! OWNER: integrator (card B5-04). Filtering is a pure function so the match
//! quality is testable without a UI.

use eframe::egui::{self, Align2};

use crate::app::{App, Selection};
use crate::sidebar::Meta;
use crate::theme;

/// Maximum rows the overlay shows at once; the input narrows before scrolling
/// matters, so the list stays one screenful.
const MAX_ROWS: usize = 14;

impl App {
    /// The paths quick open offers: every indexed file, in walk order.
    fn quick_open_paths(&self) -> Vec<String> {
        match &self.index {
            Some(idx) => idx.files.iter().map(|f| f.path.clone()).collect(),
            None => Vec::new(),
        }
    }
}

/// Rank `paths` by how well they match `query` (case-insensitive substring).
///
/// * empty query → the input order;
/// * non-empty query → matches only, ranked by (1) the earliest position the
///   match starts at in the path, then (2) the input order — `app` then ranks
///   `src/app.rs` above `docs/grapple.rs`, and equal positions keep the
///   repo's walk order.
pub fn filter_paths(paths: &[String], query: &str) -> Vec<String> {
    let q = query.to_lowercase();
    if q.is_empty() {
        return paths.to_vec();
    }
    let mut hits: Vec<(usize, usize, String)> = paths
        .iter()
        .enumerate()
        .filter_map(|(i, p)| {
            let lower = p.to_lowercase();
            lower.find(&q).map(|at| (at, i, p.clone()))
        })
        .collect();
    hits.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
    hits.into_iter().map(|(_, _, p)| p).collect()
}

/// The overlay, anchored top-centre. Enter opens the best match, a click opens
/// its row, and every open goes through [`App::select`] so the tab strip and
/// the selection stay one truth.
pub fn ui(ui: &mut egui::Ui, app: &mut App) {
    if !app.show_quick_open {
        return;
    }

    let mut picked: Option<String> = None;
    let mut close = app.show_quick_open;

    egui::Window::new("Quick open")
        .open(&mut close)
        .resizable(false)
        .collapsible(false)
        .default_width((ui.ctx().viewport_rect().width() * 0.9).min(420.0))
        .anchor(Align2::CENTER_TOP, [0.0, 24.0])
        .show(ui.ctx(), |ui| {
            // The input keeps focus for the overlay's lifetime; Esc/Enter are
            // read at the context level so they work while the edit is focused.
            let edit = egui::TextEdit::singleline(&mut app.quick_open_query)
                .hint_text("file name, or a path fragment…");
            let resp = ui.add(edit);
            resp.request_focus();

            ui.add_space(theme::SPACE_1);

            let paths = app.quick_open_paths();
            let shown = filter_paths(&paths, &app.quick_open_query);
            if shown.is_empty() {
                ui.weak("no file matches");
            }

            egui::ScrollArea::vertical()
                .id_salt("quick-open-list")
                .max_height(theme::ROW_HEIGHT * MAX_ROWS as f32)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    for path in shown.iter().take(MAX_ROWS) {
                        let name = path.rsplit('/').next().unwrap_or(path);
                        let dir = path[..path.len() - name.len()].trim_end_matches('/');
                        let meta_text = dir.to_string();
                        let resp = crate::sidebar::data_row(
                            ui,
                            app.open_file.as_deref() == Some(path.as_str()),
                            ("quick-open", path.clone()),
                            name.to_string(),
                            if dir.is_empty() {
                                None
                            } else {
                                Some(Meta::muted(&meta_text))
                            },
                        );
                        if resp.clicked() {
                            picked = Some(path.clone());
                        }
                    }
                });

            ui.add_space(theme::SPACE_1);
            ui.colored_label(
                theme::TEXT_MUTED,
                egui::RichText::new("↵ open best match · esc close").size(theme::FONT_SMALL),
            );

            if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                picked = shown.first().cloned();
            }
        });

    app.show_quick_open = close;
    if close && picked.is_none() {
        // Closed via ×/Esc without a pick; clear the stale query.
        app.quick_open_query.clear();
    }
    if let Some(file) = picked {
        app.show_quick_open = false;
        app.quick_open_query.clear();
        app.select(Selection::File(file));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths() -> Vec<String> {
        vec![
            "src/main.rs".to_string(),
            "src/app.rs".to_string(),
            "docs/grapple.rs".to_string(),
        ]
    }

    #[test]
    fn an_empty_query_keeps_the_input_order() {
        let paths = paths();
        assert_eq!(filter_paths(&paths, ""), paths);
    }

    #[test]
    fn the_query_ranks_matches_by_distance_to_the_file_name() {
        let hit = filter_paths(&paths(), "app");
        // `src/app.rs` ends with the match; `docs/grapple.rs` has it deeper.
        assert_eq!(hit.first(), Some(&"src/app.rs".to_string()));
        assert_eq!(hit.len(), 2);
    }

    #[test]
    fn equal_distance_falls_back_to_the_input_order() {
        assert_eq!(
            filter_paths(&paths(), "main"),
            vec!["src/main.rs".to_string()]
        );
        let both_first = filter_paths(&paths(), "src");
        assert_eq!(both_first.first(), Some(&"src/main.rs".to_string()));
    }

    #[test]
    fn no_matches_is_an_empty_list_not_an_error() {
        assert!(filter_paths(&paths(), "zzz").is_empty());
    }
}
