//! Per-file variable table: kind, type, read/write/use counts, decl line.
//! OWNER: TEXT-2.
//!
//! Data contract: `analysis.variables` (`Variable { id, name, kind, type_name,
//! file, line, reads, writes, first_use, last_use, enclosing }`). A declaration
//! counts as one write in every language; `reads == 0` marks a never-read local.
//!
//! This panel is a pure read of the artifact: rows come straight from
//! `Analysis::variables`, filtered to the file open in Text mode — or, when
//! nothing is open, to a file picked from a filterable list. `type_name` is
//! annotation-only, so an absent type renders a muted `—` rather than a guess.
//! `reads == 0` rows carry the same `theme::WARNING` signal that
//! `core::complexity::unused_variables` uses, so the F6 unused inventory reads as
//! one colour everywhere.
use eframe::egui;

use crate::app::{App, Selection};
use crate::sidebar::{data_row, Meta};
use crate::theme;
use crate::ui_kit;
use repowalk_core::analysis::{Analysis, VarKind, Variable};

/// File rows rendered before the selector collapses to a "+N more" hint.
const MAX_FILES: usize = 200;

/// How the variable table is ordered. `Reads` / `Writes` rank most-used first
/// (a hotspot view); `Name` / `Line` ascend. Every order breaks ties with an
/// explicit key, so the same analysis always yields the same list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum VarSort {
    Name,
    Reads,
    Writes,
    #[default]
    Line,
}

impl VarSort {
    fn label(self) -> &'static str {
        match self {
            VarSort::Name => "name",
            VarSort::Reads => "reads",
            VarSort::Writes => "writes",
            VarSort::Line => "line",
        }
    }
}

/// Panel view state kept in egui temp memory. The tab struct is owned by the
/// integrator, so the panel keeps its own UI state out of `App` (no per-frame
/// allocation of the `Analysis`).
#[derive(Debug, Clone, Default)]
struct ViewState {
    sort: VarSort,
    hide_unused: bool,
    name_filter: String,
    file_filter: String,
    picked_file: Option<String>,
    selected_var: Option<String>,
}

pub fn ui(ui: &mut egui::Ui, app: &mut App) {
    // Arc clone only — the `Variable` rows are borrowed, never duplicated.
    let Some(analysis) = app.analysis.clone() else {
        ui_kit::empty_state(ui, "Variables", ui_kit::NO_ANALYSIS_HINT);
        return;
    };

    let state_id = egui::Id::new("usage-variables-state");
    let mut state = ui
        .ctx()
        .data_mut(|d| d.get_temp::<ViewState>(state_id))
        .unwrap_or_default();

    // Text mode's open file wins; otherwise the user picks from the list.
    let mut viewed = app.open_file.clone().or_else(|| state.picked_file.clone());
    if viewed.is_none() {
        if let Some(picked) = file_selector(ui, app, &analysis, &mut state) {
            state.picked_file = Some(picked.clone());
            viewed = Some(picked);
        }
    }

    if let Some(file) = viewed {
        variables_ui(ui, app, &analysis, &mut state, &file);
    }
    // else: the selector rendered its own honest state.

    ui.ctx().data_mut(|d| d.insert_temp(state_id, state));
}

/// Summary header + sort/filter controls + the rows for one file.
fn variables_ui(
    ui: &mut egui::Ui,
    app: &mut App,
    analysis: &Analysis,
    state: &mut ViewState,
    file: &str,
) {
    let file_vars = file_variables(analysis, file);
    let (count, functions, never_read) = summary(&file_vars);

    ui.label(
        egui::RichText::new(short_path(file))
            .size(theme::FONT_SMALL)
            .color(theme::TEXT_SECONDARY),
    );
    ui.horizontal_wrapped(|ui| {
        ui.label(
            egui::RichText::new(format!("{count} variables · {functions} functions"))
                .size(theme::FONT_SMALL)
                .color(theme::TEXT_MUTED),
        );
        let color = if never_read > 0 {
            theme::WARNING
        } else {
            theme::TEXT_MUTED
        };
        ui.label(
            egui::RichText::new(format!("· {never_read} never read"))
                .size(theme::FONT_SMALL)
                .color(color),
        );
    });

    controls(ui, state);

    if file_vars.is_empty() {
        muted(ui, "no variables found in this file");
        return;
    }

    let mut rows = visible_variables(&file_vars, &state.name_filter, state.hide_unused);
    sort_variables(&mut rows, state.sort);
    if rows.is_empty() {
        muted(ui, "no variables match the filter");
        return;
    }

    theme::separator(ui);
    let cap = ui.available_height().max(theme::ROW_HEIGHT * 3.0);
    egui::ScrollArea::vertical()
        .id_salt("var-rows")
        .max_height(cap)
        .show(ui, |ui| {
            for v in rows.iter().copied() {
                variable_row(ui, app, state, v);
            }
        });
}

/// Name filter + sort picker + the "hide unused" toggle.
fn controls(ui: &mut egui::Ui, state: &mut ViewState) {
    let width = ui.available_width();
    ui.add(
        egui::TextEdit::singleline(&mut state.name_filter)
            .hint_text("filter names…")
            .desired_width(width),
    );
    ui.horizontal_wrapped(|ui| {
        ui.label(
            egui::RichText::new("sort")
                .size(theme::FONT_SMALL)
                .color(theme::TEXT_MUTED),
        );
        egui::ComboBox::from_id_salt("var-sort")
            .selected_text(
                egui::RichText::new(state.sort.label())
                    .size(theme::FONT_SMALL)
                    .color(theme::TEXT_SECONDARY),
            )
            .width(76.0)
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut state.sort, VarSort::Name, "name");
                ui.selectable_value(&mut state.sort, VarSort::Reads, "reads");
                ui.selectable_value(&mut state.sort, VarSort::Writes, "writes");
                ui.selectable_value(&mut state.sort, VarSort::Line, "line");
            });
        ui.checkbox(
            &mut state.hide_unused,
            egui::RichText::new("hide unused").size(theme::FONT_SMALL),
        );
    });
}

/// One variable: `name  Kind  type` left, `r/w/u · Lline` right. Clicking jumps
/// to and selects the declaration.
fn variable_row(ui: &mut egui::Ui, app: &mut App, state: &mut ViewState, v: &Variable) {
    let unused = is_unused(v);
    let selected = state.selected_var.as_deref() == Some(v.id.as_str());
    let uses = v.reads + v.writes;

    let meta_text = format!("r{} w{} u{} · L{}", v.reads, v.writes, uses, v.decl_line);
    let meta = if unused {
        Meta::tinted(&meta_text, theme::WARNING)
    } else {
        Meta::muted(&meta_text)
    };

    let resp = data_row(
        ui,
        selected,
        ("var-row", v.id.as_str()),
        name_widget(v, unused),
        Some(meta),
    );
    if resp.clicked() {
        state.selected_var = Some(v.id.clone());
        let file = v.file.clone();
        app.open_file_with(file.clone(), Some(v.decl_line));
        app.selection = Some(Selection::File(file));
    }
    resp.on_hover_text(format!(
        "{} ({}) · {} reads, {} writes, {} uses · declared on line {}",
        v.name,
        kind_label(v.kind),
        v.reads,
        v.writes,
        uses,
        v.decl_line
    ));
}

/// `name  Kind  type [unused]` as one layout job so the left cell keeps several
/// muted tones inside the shared [`data_row`] primitive.
fn name_widget(v: &Variable, unused: bool) -> egui::text::LayoutJob {
    let body = egui::FontId::new(theme::FONT_BODY, egui::FontFamily::Proportional);
    let small = egui::FontId::new(theme::FONT_SMALL, egui::FontFamily::Proportional);
    let name_color = if unused {
        theme::WARNING
    } else {
        theme::TEXT_PRIMARY
    };

    let mut job = egui::text::LayoutJob::default();
    job.append(&v.name, 0.0, span(body, name_color));
    job.append("  ", 0.0, span(small.clone(), theme::TEXT_MUTED));
    job.append(
        kind_label(v.kind),
        0.0,
        span(small.clone(), theme::TEXT_SECONDARY),
    );
    job.append("  ", 0.0, span(small.clone(), theme::TEXT_MUTED));
    // Annotation-only: an absent type stays a muted dash, never an inferred one.
    job.append(
        v.type_name.as_deref().unwrap_or("—"),
        0.0,
        span(small.clone(), theme::TEXT_MUTED),
    );
    if unused {
        job.append("  unused", 0.0, span(small, theme::WARNING));
    }
    job
}

fn span(font_id: egui::FontId, color: egui::Color32) -> egui::text::TextFormat {
    egui::text::TextFormat {
        font_id,
        color,
        ..Default::default()
    }
}

/// Filterable list of files to inspect when no file is open in Text mode.
/// Returns the path the user picked, if any.
fn file_selector(
    ui: &mut egui::Ui,
    app: &App,
    analysis: &Analysis,
    state: &mut ViewState,
) -> Option<String> {
    theme::section_header(ui, "Pick a file");
    let width = ui.available_width();
    ui.add(
        egui::TextEdit::singleline(&mut state.file_filter)
            .hint_text("filter files…")
            .desired_width(width),
    );

    let counts = variable_counts(analysis);
    // `owned` backs the fallback list only when there is no index; declared out
    // here so `files` can borrow it for the rest of the function.
    let owned: Vec<String>;
    let files: Vec<&str> = match &app.index {
        Some(idx) => idx.files.iter().map(|f| f.path.as_str()).collect(),
        None => {
            owned = distinct_variable_files(analysis);
            owned.iter().map(String::as_str).collect()
        }
    };
    if files.is_empty() {
        muted(ui, "no files indexed yet");
        return None;
    }

    let needle = state.file_filter.trim().to_lowercase();
    let matched: Vec<&str> = files
        .into_iter()
        .filter(|p| needle.is_empty() || p.to_lowercase().contains(&needle))
        .collect();
    if matched.is_empty() {
        muted(ui, "no files match the filter");
        return None;
    }

    ui.label(
        egui::RichText::new(format!("{} file(s)", matched.len()))
            .size(theme::FONT_SMALL)
            .color(theme::TEXT_MUTED),
    );

    let mut picked = None;
    let cap = ui.available_height().max(theme::ROW_HEIGHT * 3.0);
    egui::ScrollArea::vertical()
        .id_salt("var-file-list")
        .max_height(cap)
        .show(ui, |ui| {
            for path in matched.iter().copied().take(MAX_FILES) {
                let n = counts.get(path).copied().unwrap_or(0);
                let count_text = format!("{n} var{}", if n == 1 { "" } else { "s" });
                let meta = if n > 0 {
                    Meta::badge(&count_text)
                } else {
                    Meta::muted(&count_text)
                };
                let selected = state.picked_file.as_deref() == Some(path);
                let resp = data_row(
                    ui,
                    selected,
                    ("var-file", path),
                    short_path(path),
                    Some(meta),
                );
                if resp.clicked() {
                    picked = Some(path.to_string());
                }
                resp.on_hover_text(path);
            }
        });
    if matched.len() > MAX_FILES {
        muted(
            ui,
            &format!(
                "+{} more files — refine the filter",
                matched.len() - MAX_FILES
            ),
        );
    }
    picked
}

// ── Pure helpers (unit-tested; no UI) ───────────────────────────────────────

/// `true` when the variable is never read — the F6 dead-code signal.
fn is_unused(v: &Variable) -> bool {
    v.reads == 0
}

fn kind_label(kind: VarKind) -> &'static str {
    match kind {
        VarKind::Local => "Local",
        VarKind::Param => "Param",
        VarKind::Field => "Field",
        VarKind::Static => "Static",
        VarKind::Global => "Global",
    }
}

/// Every variable declared in `file`, in artifact order.
fn file_variables<'a>(analysis: &'a Analysis, file: &str) -> Vec<&'a Variable> {
    analysis
        .variables
        .iter()
        .filter(|v| v.file == file)
        .collect()
}

/// Apply the name filter (case-insensitive substring) and the "hide unused"
/// toggle. Ordering is left to [`sort_variables`].
fn visible_variables<'a>(
    vars: &[&'a Variable],
    name_filter: &str,
    hide_unused: bool,
) -> Vec<&'a Variable> {
    let needle = name_filter.trim().to_lowercase();
    vars.iter()
        .copied()
        .filter(|v| !hide_unused || !is_unused(v))
        .filter(|v| needle.is_empty() || v.name.to_lowercase().contains(&needle))
        .collect()
}

/// Deterministic order for the chosen key; reads/writes rank most-used first.
fn sort_variables(rows: &mut [&Variable], sort: VarSort) {
    rows.sort_by(|a, b| match sort {
        VarSort::Name => a
            .name
            .cmp(&b.name)
            .then_with(|| a.decl_line.cmp(&b.decl_line))
            .then_with(|| a.id.cmp(&b.id)),
        VarSort::Reads => b
            .reads
            .cmp(&a.reads)
            .then_with(|| a.name.cmp(&b.name))
            .then_with(|| a.decl_line.cmp(&b.decl_line)),
        VarSort::Writes => b
            .writes
            .cmp(&a.writes)
            .then_with(|| a.name.cmp(&b.name))
            .then_with(|| a.decl_line.cmp(&b.decl_line)),
        VarSort::Line => a
            .decl_line
            .cmp(&b.decl_line)
            .then_with(|| a.name.cmp(&b.name))
            .then_with(|| a.id.cmp(&b.id)),
    });
}

/// `(variables, functions, never read)` for the summary header. A "function"
/// is a distinct enclosing scope among these variables, so the count always
/// matches what the table can show.
fn summary(vars: &[&Variable]) -> (usize, usize, usize) {
    let mut functions: std::collections::BTreeSet<&str> = std::collections::BTreeSet::new();
    let mut never_read = 0usize;
    for v in vars {
        if !v.enclosing.is_empty() {
            functions.insert(v.enclosing.as_str());
        }
        if is_unused(v) {
            never_read += 1;
        }
    }
    (vars.len(), functions.len(), never_read)
}

/// Sorted, unique file paths that declare at least one variable.
fn distinct_variable_files(analysis: &Analysis) -> Vec<String> {
    let mut files: std::collections::BTreeSet<&str> = std::collections::BTreeSet::new();
    for v in &analysis.variables {
        files.insert(v.file.as_str());
    }
    files.into_iter().map(str::to_string).collect()
}

/// Variables declared per file, for the selector's count badges.
fn variable_counts(analysis: &Analysis) -> std::collections::BTreeMap<&str, usize> {
    let mut counts: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
    for v in &analysis.variables {
        *counts.entry(v.file.as_str()).or_insert(0) += 1;
    }
    counts
}

/// `crates/gui/src/usage/variables.rs` → `usage/variables.rs` (last two segments).
fn short_path(path: &str) -> String {
    let mut parts: Vec<&str> = path.rsplit('/').take(2).collect();
    parts.reverse();
    parts.join("/")
}

fn muted(ui: &mut egui::Ui, text: &str) {
    ui.label(
        egui::RichText::new(text)
            .size(theme::FONT_SMALL)
            .color(theme::TEXT_MUTED),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn var(
        file: &str,
        line: usize,
        name: &str,
        kind: VarKind,
        reads: usize,
        writes: usize,
    ) -> Variable {
        Variable {
            id: format!("{file}:{line}:{name}"),
            name: name.into(),
            kind,
            file: file.into(),
            decl_line: line,
            scope_start: line,
            scope_end: line + 3,
            type_name: None,
            reads,
            writes,
            first_use: None,
            last_use: None,
            enclosing: format!("{file}:1"),
        }
    }

    #[test]
    fn hide_unused_and_name_filter_compose() {
        let alpha = var("f.rs", 3, "alpha", VarKind::Local, 0, 1);
        let beta = var("f.rs", 7, "Beta", VarKind::Param, 2, 0);
        let gamma = var("f.rs", 9, "gamma", VarKind::Static, 0, 0);
        let all = vec![&alpha, &beta, &gamma];

        assert_eq!(visible_variables(&all, "", false).len(), 3);

        let used = visible_variables(&all, "", true);
        assert_eq!(
            used.iter().map(|v| v.name.as_str()).collect::<Vec<_>>(),
            vec!["Beta"]
        );

        // Name filter is a case-insensitive substring.
        let g = visible_variables(&all, "GAM", false);
        assert_eq!(
            g.iter().map(|v| v.name.as_str()).collect::<Vec<_>>(),
            vec!["gamma"]
        );

        // Filters compose: gamma is unused, so hiding unused removes it too.
        assert!(visible_variables(&all, "gamma", true).is_empty());
    }

    #[test]
    fn sort_orders_are_deterministic() {
        let a = var("f.rs", 9, "alpha", VarKind::Local, 1, 3);
        let b = var("f.rs", 2, "beta", VarKind::Local, 5, 0);
        let c = var("f.rs", 5, "alpha", VarKind::Local, 5, 3);
        let mut rows = vec![&a, &b, &c];

        sort_variables(&mut rows, VarSort::Line);
        assert_eq!(
            rows.iter().map(|v| v.decl_line).collect::<Vec<_>>(),
            vec![2, 5, 9]
        );

        sort_variables(&mut rows, VarSort::Name);
        assert_eq!(
            rows.iter()
                .map(|v| (v.name.as_str(), v.decl_line))
                .collect::<Vec<_>>(),
            vec![("alpha", 5), ("alpha", 9), ("beta", 2)]
        );

        // Reads: most-read first; the 5-read tie breaks by name then line.
        sort_variables(&mut rows, VarSort::Reads);
        assert_eq!(
            rows.iter()
                .map(|v| (v.name.as_str(), v.decl_line))
                .collect::<Vec<_>>(),
            vec![("alpha", 5), ("beta", 2), ("alpha", 9)]
        );

        // Writes: most-written first; the 3-write tie breaks by line.
        sort_variables(&mut rows, VarSort::Writes);
        assert_eq!(
            rows.iter()
                .map(|v| (v.name.as_str(), v.decl_line))
                .collect::<Vec<_>>(),
            vec![("alpha", 5), ("alpha", 9), ("beta", 2)]
        );
    }

    #[test]
    fn reads_zero_is_unused() {
        assert!(is_unused(&var("f.rs", 1, "x", VarKind::Local, 0, 4)));
        assert!(!is_unused(&var("f.rs", 1, "x", VarKind::Local, 1, 0)));
        // A written-but-never-read field still counts as unused.
        assert!(is_unused(&var("f.rs", 2, "f", VarKind::Field, 0, 7)));
    }

    #[test]
    fn summary_counts_functions_and_never_read() {
        let a = var("f.rs", 1, "x", VarKind::Local, 0, 1);
        let mut b = var("f.rs", 9, "y", VarKind::Param, 2, 0);
        b.enclosing = "f.rs:8".into();
        let mut c = var("f.rs", 20, "z", VarKind::Global, 0, 0);
        c.enclosing = String::new();
        let vars = vec![&a, &b, &c];

        // 3 variables, 2 distinct enclosing scopes (the empty one is ignored),
        // and 2 never read (a and c).
        assert_eq!(summary(&vars), (3, 2, 2));
    }

    #[test]
    fn empty_inputs_do_not_panic() {
        let analysis = Analysis::new();
        assert!(file_variables(&analysis, "none.rs").is_empty());
        assert!(distinct_variable_files(&analysis).is_empty());
        assert!(variable_counts(&analysis).is_empty());

        let empty: Vec<&Variable> = Vec::new();
        assert!(visible_variables(&empty, "x", true).is_empty());
        assert_eq!(summary(&empty), (0, 0, 0));

        let mut rows: Vec<&Variable> = Vec::new();
        sort_variables(&mut rows, VarSort::Reads);
        assert!(rows.is_empty());
    }
}
