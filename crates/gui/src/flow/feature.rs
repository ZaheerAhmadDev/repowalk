//! Feature tab (F2b/F5): pick an entrypoint + depth and read
//! `repowalk_core::feature::slice` as an ordered call tree, with a
//! covered-vs-never-called roll-up. OWNER: TL-GRAPH (GRAPH-5). Reuses the
//! shared transport/playback and step-detail panel; adds only the entry picker,
//! the ordered step rows, and the coverage header.
//!
//! ## Honesty (R-20)
//!
//! The slice is walked by core `feature::slice`: **only `EdgeKind::Call` edges**
//! are followed and `Confidence::Unresolved` hops are **skipped** (they name no
//! known symbol and would fabricate nodes). `Exact`, `NameOnly` and `Ambiguous`
//! hops are walked, but a name-only / ambiguous hop is a *guess* — the UI never
//! presents one as confirmed (see the footnote under the steps).
//!
//! Coverage is not re-implemented here: it reads core's recorded-caller
//! aggregate (`metrics::compute().fan_in`). A callable is **covered** when the
//! index records at least one caller edge to it and **never called** otherwise
//! — a recorded-edge fact, not a proof of dead code.
use std::collections::BTreeSet;

use eframe::egui;
use repowalk_core::feature::{FeatureSlice, FeatureStep};
use repowalk_core::metrics;
use repowalk_core::model::{Index, Symbol, SymbolKind};
use repowalk_core::search;

use crate::app::Selection;
use crate::flow::{render, FlowState};
use crate::theme;

/// Fuzzy hits offered by the "any symbol" entry search.
const MAX_PICK_HITS: usize = 8;

/// What a click in the Feature tab resolved to.
pub enum FeaturePick {
    /// A slice step: seek the playhead and select the symbol.
    Step(usize, String),
    /// A coverage-list symbol: select it, leave the playhead alone.
    Symbol(String),
}

/// A callable is a function or a method — the only kinds for which "called" /
/// "never called" means anything.
pub fn is_callable(kind: SymbolKind) -> bool {
    matches!(kind, SymbolKind::Function | SymbolKind::Method)
}

/// Covered-vs-never-called tally over the callables in `scope` (`None` = the
/// whole index).
pub struct Coverage {
    pub total: usize,
    pub covered: usize,
    /// Symbol ids of callables with no recorded caller, sorted `(file, line)`.
    pub never_called: Vec<String>,
}

impl Coverage {
    pub fn never_called_count(&self) -> usize {
        self.never_called.len()
    }
}

/// Classify callables as covered / never called from core's caller aggregate.
///
/// `metrics::compute().fan_in` keys a callee endpoint to its distinct callers;
/// a Call edge's callee is a symbol id (`file:line`) while an Import edge's is a
/// file path, so testing callable ids against those keys is exactly "does the
/// index record a caller for this callable". No traversal is re-implemented.
pub fn coverage(idx: &Index, scope: Option<&[String]>) -> Coverage {
    let computed = metrics::compute(idx);
    let called: BTreeSet<&str> = computed.fan_in.iter().map(|(id, _)| id.as_str()).collect();
    let in_scope = |id: &str| scope.is_none_or(|ids| ids.iter().any(|s| s == id));

    let mut total = 0usize;
    let mut covered = 0usize;
    let mut never_called = Vec::new();
    for sym in &idx.symbols {
        if !is_callable(sym.kind) || !in_scope(&sym.id) {
            continue;
        }
        total += 1;
        if called.contains(sym.id.as_str()) {
            covered += 1;
        } else {
            never_called.push(sym.id.clone());
        }
    }
    never_called.sort_by(|a, b| {
        render::file_of(a)
            .cmp(render::file_of(b))
            .then_with(|| render::line_of(a).cmp(&render::line_of(b)))
    });
    Coverage {
        total,
        covered,
        never_called,
    }
}

/// The symbol that best represents an entrypoint *file*: its `main`/function
/// with the smallest start line, else its first callable, else its first symbol.
fn primary_symbol<'a>(idx: &'a Index, file: &str) -> Option<&'a str> {
    let mut cands: Vec<&Symbol> = idx.symbols.iter().filter(|s| s.file == file).collect();
    cands.sort_by(|a, b| {
        a.start_line
            .cmp(&b.start_line)
            .then_with(|| a.id.cmp(&b.id))
    });
    cands
        .iter()
        .find(|s| is_callable(s.kind) && s.name == "main")
        .or_else(|| cands.iter().find(|s| is_callable(s.kind)))
        .or_else(|| cands.first())
        .map(|s| s.id.as_str())
}

/// One representative symbol id per `is_entrypoint` file (sorted by path).
/// Files with no indexed symbol are skipped rather than guessed at.
pub fn entrypoints(idx: &Index) -> Vec<String> {
    let mut files: Vec<&str> = idx
        .files
        .iter()
        .filter(|f| f.is_entrypoint)
        .map(|f| f.path.as_str())
        .collect();
    files.sort_unstable();
    files
        .into_iter()
        .filter_map(|f| primary_symbol(idx, f).map(str::to_string))
        .collect()
}

// ── Entry picker ────────────────────────────────────────────────────────────

/// Entry picker: current entry, entrypoint shortcuts, a fuzzy "any symbol"
/// search, and the depth cap. A change rewinds the playhead.
pub fn picker(ui: &mut egui::Ui, flow: &mut FlowState, idx: &Index) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = theme::SPACE_2;
        ui.label(
            egui::RichText::new("entrypoint")
                .size(theme::FONT_SMALL)
                .strong()
                .color(theme::TEXT_MUTED),
        );
        if flow.feature_entry.is_empty() {
            ui.label(
                egui::RichText::new("not set — pick a shortcut or search below")
                    .size(theme::FONT_SMALL)
                    .color(theme::TEXT_MUTED),
            );
        } else {
            ui.label(
                egui::RichText::new(render::display_name(idx, &flow.feature_entry))
                    .size(theme::FONT_BODY)
                    .strong(),
            );
            ui.label(
                egui::RichText::new(render::file_of(&flow.feature_entry))
                    .size(theme::FONT_SMALL)
                    .color(theme::TEXT_MUTED),
            );
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .add(egui::Slider::new(&mut flow.feature_depth, 0..=6).text("depth"))
                .on_hover_text("How many call hops down from the entry to walk")
                .changed()
            {
                flow.playback.playhead = 0.0;
            }
        });
    });

    let eps = entrypoints(idx);
    if !eps.is_empty() {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = theme::SPACE_2;
            ui.label(
                egui::RichText::new("entrypoints")
                    .size(theme::FONT_SMALL)
                    .color(theme::TEXT_MUTED),
            );
            for id in &eps {
                let label = render::display_name(idx, id);
                if ui.small_button(label).on_hover_text(id.as_str()).clicked() {
                    set_entry(flow, id);
                }
            }
        });
    }

    egui::TextEdit::singleline(&mut flow.feature_query)
        .id(egui::Id::new("flow-feature-pick"))
        .hint_text("search any symbol for the entrypoint…")
        .desired_width(ui.available_width())
        .show(ui);
    if flow.feature_query.is_empty() {
        return;
    }
    let sym_ids: BTreeSet<&str> = idx.symbols.iter().map(|s| s.id.as_str()).collect();
    for hit in search::search(idx, &flow.feature_query, MAX_PICK_HITS) {
        // Only symbol hits can seed a slice; file hits are ignored.
        if !sym_ids.contains(hit.target.as_str()) {
            continue;
        }
        let label = format!("{}  ({})", render::display_name(idx, &hit.target), hit.path);
        if ui
            .selectable_label(false, egui::RichText::new(label).size(theme::FONT_SMALL))
            .clicked()
        {
            set_entry(flow, &hit.target);
            flow.feature_query.clear();
        }
    }
}

/// Set the feature entry (rewinding only when it actually changes).
fn set_entry(flow: &mut FlowState, id: &str) {
    if flow.feature_entry != id {
        flow.playback.playhead = 0.0;
    }
    flow.feature_entry = id.to_string();
}

// ── Feature view ────────────────────────────────────────────────────────────

/// Coverage header + ordered steps + the R-20 footnote. Returns the click, if
/// any (the caller selects the symbol and, for a step, seeks the playhead).
pub fn ui(
    ui: &mut egui::Ui,
    flow: &mut FlowState,
    idx: &Index,
    slice: &FeatureSlice,
    selection: Option<&Selection>,
    step: usize,
    progress: f32,
) -> Option<FeaturePick> {
    let mut pick = None;

    let symbols: Vec<String> = slice.symbols().iter().map(|s| s.to_string()).collect();
    if let Some(id) = coverage_panel(ui, flow, idx, &symbols, selection) {
        pick = Some(FeaturePick::Symbol(id));
    }
    ui.add_space(theme::SPACE_2);
    theme::separator(ui);

    let selected = match selection {
        Some(Selection::Symbol(id)) => Some(id.as_str()),
        _ => None,
    };
    let cap = (ui.available_height() - theme::SPACE_8).max(theme::ROW_HEIGHT * 3.0);
    egui::ScrollArea::vertical()
        .id_salt("flow-feature-steps")
        .max_height(cap)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for (i, s) in slice.steps.iter().enumerate() {
                let is_selected = selected == Some(s.symbol.as_str());
                if step_row(ui, s, i, i == step, is_selected, progress).clicked() {
                    pick = Some(FeaturePick::Step(i, s.symbol.clone()));
                }
            }
        });

    footnote(ui);
    pick
}

/// Coverage header (scope toggle + counts) and the collapsible "never called"
/// list. Returns a symbol id clicked in that list.
fn coverage_panel(
    ui: &mut egui::Ui,
    flow: &mut FlowState,
    idx: &Index,
    slice_symbols: &[String],
    selection: Option<&Selection>,
) -> Option<String> {
    let mut pick = None;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = theme::SPACE_2;
        ui.label(
            egui::RichText::new("coverage")
                .size(theme::FONT_SMALL)
                .strong()
                .color(theme::TEXT_MUTED),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let to_repo = ui
                .selectable_label(flow.feature_scope_all, "repo")
                .clicked();
            if to_repo && !flow.feature_scope_all {
                flow.feature_scope_all = true;
                flow.playback.playhead = 0.0;
            }
            let to_slice = ui
                .selectable_label(!flow.feature_scope_all, "slice")
                .clicked();
            if to_slice && flow.feature_scope_all {
                flow.feature_scope_all = false;
                flow.playback.playhead = 0.0;
            }
        });
    });

    let scope = if flow.feature_scope_all {
        None
    } else {
        Some(slice_symbols)
    };
    let cov = coverage(idx, scope);

    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = theme::SPACE_1;
        ui.label(
            egui::RichText::new(cov.covered.to_string())
                .size(theme::FONT_BODY)
                .strong()
                .color(theme::SUCCESS),
        );
        ui.label(
            egui::RichText::new(format!("of {} callables covered", cov.total))
                .size(theme::FONT_SMALL)
                .color(theme::TEXT_SECONDARY),
        );
        ui.label(
            egui::RichText::new("·")
                .size(theme::FONT_SMALL)
                .color(theme::TEXT_MUTED),
        );
        let dead = cov.never_called_count();
        ui.label(
            egui::RichText::new(format!("{dead} never called"))
                .size(theme::FONT_SMALL)
                .color(if dead == 0 {
                    theme::SUCCESS
                } else {
                    theme::WARNING
                }),
        );
    });

    if !cov.never_called.is_empty() {
        egui::CollapsingHeader::new(
            egui::RichText::new(format!("never called ({})", cov.never_called_count()))
                .size(theme::FONT_SMALL)
                .color(theme::TEXT_MUTED),
        )
        .id_salt("flow-feature-never")
        .show(ui, |ui| {
            for id in &cov.never_called {
                let selected = matches!(selection, Some(Selection::Symbol(s)) if s == id);
                let label = format!("{}  {}", render::display_name(idx, id), render::file_of(id));
                if ui
                    .selectable_label(selected, egui::RichText::new(label).size(theme::FONT_SMALL))
                    .clicked()
                {
                    pick = Some(id.clone());
                }
            }
        });
    }
    pick
}

/// One ordered step row: depth badge, symbol name, call-site caption. Active
/// (playhead) and selected (app selection) states are visible with zero motion;
/// the active wash only intensifies the current step.
fn step_row(
    ui: &mut egui::Ui,
    step: &FeatureStep,
    i: usize,
    active: bool,
    selected: bool,
    progress: f32,
) -> egui::Response {
    let width = ui.available_width();
    let (rect, resp) =
        ui.allocate_exact_size(egui::vec2(width, theme::ROW_HEIGHT), egui::Sense::click());

    let corner = egui::CornerRadius::same(theme::CORNER_SM);
    let fill = if active {
        theme::with_alpha(theme::ACCENT, (theme::step_alpha(progress) * 255.0) as u8)
    } else if selected {
        theme::SURFACE_SELECTED
    } else if resp.hovered() {
        theme::SURFACE_HOVER
    } else {
        egui::Color32::TRANSPARENT
    };
    if fill != egui::Color32::TRANSPARENT {
        ui.painter().rect_filled(rect, corner, fill);
    }
    if selected {
        let spine = egui::Rect::from_min_max(
            egui::pos2(rect.min.x, rect.min.y + 2.0),
            egui::pos2(rect.min.x + 2.0, rect.max.y - 2.0),
        );
        ui.painter()
            .rect_filled(spine, egui::CornerRadius::ZERO, theme::ACCENT);
    }

    let indent = theme::SPACE_2 + step.depth as f32 * theme::TREE_INDENT;
    let inner = egui::Rect::from_min_max(
        egui::pos2(rect.min.x + indent, rect.min.y),
        egui::pos2(rect.max.x - theme::SPACE_2, rect.max.y),
    );
    let name_color = if active || selected {
        theme::TEXT_PRIMARY
    } else {
        theme::TEXT_SECONDARY
    };
    ui.scope_builder(
        egui::UiBuilder::new()
            .id_salt(("flow-feature-step", i))
            .max_rect(inner)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
        |ui| {
            ui.spacing_mut().item_spacing.x = theme::SPACE_2;
            depth_badge(ui, step.depth);
            ui.add(
                egui::Label::new(
                    egui::RichText::new(&step.name)
                        .size(theme::FONT_BODY)
                        .color(name_color),
                )
                .truncate(),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    egui::RichText::new(call_caption(step))
                        .monospace()
                        .size(theme::FONT_SMALL)
                        .color(theme::TEXT_MUTED),
                );
            });
        },
    );

    let caption = if step.depth == 0 {
        format!("{}\nentrypoint — the slice always starts here", step.name)
    } else {
        format!(
            "{}\nreached at line {} (depth {})\nclick to select & seek",
            step.name, step.line, step.depth
        )
    };
    resp.on_hover_text(caption)
}

/// Small rounded `d{n}` depth badge.
fn depth_badge(ui: &mut egui::Ui, depth: usize) {
    let font = egui::FontId::new(theme::FONT_SMALL, egui::FontFamily::Proportional);
    let galley = ui
        .painter()
        .layout_no_wrap(depth_badge_text(depth), font, theme::TEXT_SECONDARY);
    let size = egui::vec2(galley.size().x + 10.0, 16.0);
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    ui.painter()
        .rect_filled(rect, egui::CornerRadius::same(8), theme::accent_subtle(48));
    ui.painter().galley(
        egui::pos2(rect.left() + 5.0, rect.center().y - galley.size().y / 2.0),
        galley,
        theme::TEXT_SECONDARY,
    );
}

/// Depth badge text (`d0` = the entry).
fn depth_badge_text(depth: usize) -> String {
    format!("d{depth}")
}

/// Right-hand caption: the entry says "entry"; a reached step shows the call
/// site line it was reached at (the line lives in the *caller's* file, so only
/// the line is honest to show here).
fn call_caption(step: &FeatureStep) -> String {
    if step.depth == 0 {
        "entry".to_string()
    } else {
        format!("line {}", step.line)
    }
}

/// Muted R-20 footnote: what the slice walks and what is never "confirmed".
fn footnote(ui: &mut egui::Ui) {
    ui.add_space(theme::SPACE_1);
    ui.add(
        egui::Label::new(
            egui::RichText::new(
                "follows Call edges only · Unresolved edges skipped · name-only / ambiguous hops are not confirmed",
            )
            .size(theme::FONT_SMALL)
            .color(theme::TEXT_DISABLED),
        )
        .truncate(),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flow::{sequence, FlowTab, Sequence};
    use repowalk_core::model::{Confidence, Edge, EdgeKind, FileEntry};

    fn sym(file: &str, line: usize, name: &str, kind: SymbolKind) -> Symbol {
        Symbol {
            id: format!("{file}:{line}"),
            name: name.into(),
            kind,
            signature: String::new(),
            doc: String::new(),
            start_line: line,
            end_line: line + 40,
            container: None,
            file: file.into(),
        }
    }

    fn call(caller: &str, callee: &str, line: usize, confidence: Confidence) -> Edge {
        Edge {
            caller: caller.into(),
            callee: callee.into(),
            line,
            kind: EdgeKind::Call,
            confidence,
        }
    }

    fn feature_flow(entry: &str) -> FlowState {
        FlowState {
            tab: FlowTab::Feature,
            feature_entry: entry.into(),
            ..Default::default()
        }
    }

    /// A hand-built index: `a` calls `c` at line 7 and `b` at line 3, so the
    /// core contract orders the depth-1 layer by call line (b before c). The
    /// rendering rule maps that to `d{depth}` badges + `line {n}` captions.
    #[test]
    fn slice_steps_follow_core_order_with_lines_and_depth_badges() {
        let mut idx = Index::new("/repo");
        idx.symbols = vec![
            sym("src/a.rs", 10, "a", SymbolKind::Function),
            sym("src/b.rs", 1, "b", SymbolKind::Function),
            sym("src/c.rs", 1, "c", SymbolKind::Function),
        ];
        idx.edges = vec![
            call("src/a.rs:10", "src/c.rs:1", 7, Confidence::Exact),
            call("src/a.rs:10", "src/b.rs:1", 3, Confidence::Exact),
        ];

        let slice = repowalk_core::feature::slice(&idx, "src/a.rs:10", 2).expect("known entry");
        let rows: Vec<(String, String, String, usize)> = slice
            .steps
            .iter()
            .map(|s| {
                (
                    s.name.clone(),
                    depth_badge_text(s.depth),
                    call_caption(s),
                    s.line,
                )
            })
            .collect();
        assert_eq!(
            rows,
            vec![
                ("a".into(), "d0".into(), "entry".into(), 0),
                ("b".into(), "d1".into(), "line 3".into(), 3),
                ("c".into(), "d1".into(), "line 7".into(), 7),
            ]
        );
    }

    /// Coverage classifies only callables; a callable with a recorded caller is
    /// covered, everything else is never called. The playhead/selection states
    /// are rendering, but the classification rule is pure and tested here.
    #[test]
    fn coverage_classifies_called_vs_never_called() {
        let mut idx = Index::new("/repo");
        idx.symbols = vec![
            sym("src/a.rs", 1, "a", SymbolKind::Function),
            sym("src/b.rs", 1, "b", SymbolKind::Function),
            sym("src/s.rs", 1, "S", SymbolKind::Struct), // not callable
            sym("src/c.rs", 1, "c", SymbolKind::Method),
        ];
        idx.edges = vec![call("src/a.rs:1", "src/b.rs:1", 2, Confidence::Exact)];

        let all = coverage(&idx, None);
        assert_eq!((all.total, all.covered), (3, 1));
        assert_eq!(
            all.never_called,
            vec!["src/a.rs:1".to_string(), "src/c.rs:1".to_string()]
        );

        // Scope narrows the tally to the slice's symbols.
        let scope = vec!["src/a.rs:1".to_string(), "src/b.rs:1".to_string()];
        let scoped = coverage(&idx, Some(&scope));
        assert_eq!((scoped.total, scoped.covered), (2, 1));
        assert_eq!(scoped.never_called, vec!["src/a.rs:1".to_string()]);
    }

    /// Empty / unknown / single-step states are honest and never panic.
    #[test]
    fn empty_unknown_and_single_step_states_are_honest() {
        let empty = Index::new("/repo");

        // No entry → asked to pick; unknown entry → core slice None → named.
        let no_entry = feature_flow("");
        assert!(matches!(
            sequence(&empty, &no_entry),
            Sequence::PickEndpoints
        ));
        let unknown = feature_flow("src/a.rs:1");
        assert!(matches!(
            sequence(&empty, &unknown),
            Sequence::UnknownFeatureEntry
        ));

        // A known, edgeless entry renders a one-step feature, not a broken view.
        let mut idx = Index::new("/repo");
        idx.symbols = vec![sym("src/z.rs", 1, "z", SymbolKind::Function)];
        match sequence(&idx, &feature_flow("src/z.rs:1")) {
            Sequence::Feature(s) => {
                assert_eq!(s.steps.len(), 1);
                assert_eq!(call_caption(&s.steps[0]), "entry");
            }
            _ => panic!("expected a one-step Feature sequence"),
        }

        // Empty index: coverage + entrypoints are empty, not panicking.
        let cov = coverage(&empty, None);
        assert_eq!(
            (cov.total, cov.covered, cov.never_called_count()),
            (0, 0, 0)
        );
        assert!(entrypoints(&empty).is_empty());
    }

    /// Entrypoint files map to their `main`/first callable for the picker.
    #[test]
    fn entrypoint_files_resolve_to_their_primary_symbol() {
        let mut idx = Index::new("/repo");
        idx.files = vec![FileEntry {
            path: "src/main.rs".into(),
            lang: "rust".into(),
            lines: 10,
            mtime: 0,
            is_entrypoint: true,
        }];
        idx.symbols = vec![
            sym("src/main.rs", 20, "helper", SymbolKind::Function),
            sym("src/main.rs", 1, "main", SymbolKind::Function),
        ];
        assert_eq!(entrypoints(&idx), vec!["src/main.rs:1".to_string()]);
    }
}
