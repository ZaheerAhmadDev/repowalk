//! References panel: every use site of the selected symbol, grouped by file.
//! OWNER: TEXT-3.
//!
//! Data comes from `repowalk_core::refs`. We use `references_ex` (not the plain
//! union) so resolved hits and name-only hits are rendered differently —
//! **risk R-20**: an unresolved use carries a raw name, and the panel must never
//! present it as a confirmed reference. `references_by_file` supplies the
//! grouping; the resolved/name-only tag per site is recovered from the `RefSet`.
//!
//! Jump-to a site goes through `app.open_file_with(path, Some(line))` and leaves
//! `app.selection` on the target, so the global selection still drives every
//! pane. Selecting the *target* elsewhere uses `app.select(Selection::Symbol(id))`.
//!
//! Known limitation (documented in `crates/core/src/refs.rs`): only `Variable`
//! and `TypeNode` carry a name next to an id in `Analysis`, so a use recorded by
//! raw name cannot be linked by `core::refs` alone. This panel supplies the
//! symbol's name from the index (`references_for_symbol`), so a cross-file call
//! — the common case — is found and listed in the **name-only** group instead of
//! showing "no references".
use eframe::egui;
use std::collections::HashSet;

use crate::app::{App, Selection};
use crate::sidebar::{data_row, Meta};
use crate::theme;
use repowalk_core::analysis::{Analysis, UseKind, UseSite};
use repowalk_core::refs::{self, RefSet};

/// Which resolution classes the list shows. `All` is the honest default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum ResFilter {
    #[default]
    All,
    Resolved,
    NameOnly,
}

impl ResFilter {
    fn label(self) -> &'static str {
        match self {
            ResFilter::All => "All",
            ResFilter::Resolved => "Resolved",
            ResFilter::NameOnly => "Name-only",
        }
    }

    /// Whether a site of this resolution class is kept.
    fn allows(self, name_only: bool) -> bool {
        match self {
            ResFilter::All => true,
            ResFilter::Resolved => !name_only,
            ResFilter::NameOnly => name_only,
        }
    }
}

/// Panel-local view state, kept in egui memory (the shared `UsageState` is owned
/// by the integrator and must not grow per-panel fields).
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct RefsState {
    pub(crate) res: ResFilter,
    pub(crate) kind: Option<UseKind>,
}

/// A use site plus whether the analyzer only matched it by name (R-20).
#[derive(Debug, Clone, PartialEq, Eq)]
struct RefRow {
    site: UseSite,
    name_only: bool,
}

/// Stable rank for a [`UseKind`], used to build collision-free row ids.
fn kind_rank(kind: UseKind) -> u8 {
    match kind {
        UseKind::Call => 0,
        UseKind::TypeRef => 1,
        UseKind::FieldAccess => 2,
        UseKind::VarRead => 3,
        UseKind::VarWrite => 4,
        UseKind::Import => 5,
    }
}

/// Human badge label for a use kind.
fn kind_label(kind: UseKind) -> &'static str {
    match kind {
        UseKind::Call => "Call",
        UseKind::TypeRef => "TypeRef",
        UseKind::FieldAccess => "FieldAccess",
        UseKind::VarRead => "VarRead",
        UseKind::VarWrite => "VarWrite",
        UseKind::Import => "Import",
    }
}

/// Identity of a site, including its target so two identical-looking sites with
/// different targets never collide.
fn site_key(site: &UseSite) -> (String, usize, usize, u8, String) {
    (
        site.file.clone(),
        site.line,
        site.col,
        kind_rank(site.kind),
        site.target.clone(),
    )
}

/// The target's use sites, including the symbol's **name-only** call sites.
///
/// A function symbol id carries no name in the analysis artifact, so a
/// cross-file call — recorded with the raw callee name as its target — cannot be
/// linked from the analysis alone. The index has the name; hand it to
/// `references_for_symbol` and the hits land in the name-only group (risk R-20:
/// never shown as confirmed).
fn refs_for(app: &App, analysis: &Analysis, target: &str) -> RefSet {
    match app
        .index
        .as_ref()
        .and_then(|ix| ix.symbols.iter().find(|s| s.id == target))
    {
        Some(sym) => refs::references_for_symbol(analysis, target, &sym.name),
        None => refs::references_ex(analysis, target),
    }
}

/// Tag the set's uses resolved / name-only and group them by file — the shape
/// the panel renders.
fn grouped_rows(set: &RefSet) -> Vec<(String, Vec<RefRow>)> {
    let name_only: HashSet<(String, usize, usize, u8, String)> =
        set.by_name.iter().map(site_key).collect();
    set.by_file()
        .into_iter()
        .map(|(file, sites)| {
            let rows = sites
                .into_iter()
                .map(|site| {
                    let name_only = name_only.contains(&site_key(&site));
                    RefRow { site, name_only }
                })
                .collect();
            (file, rows)
        })
        .collect()
}

/// Whether a row survives the current filters.
fn passes(state: RefsState, row: &RefRow) -> bool {
    state.res.allows(row.name_only) && state.kind.is_none_or(|k| row.site.kind == k)
}

/// Apply the filters, dropping now-empty file groups.
fn filtered_rows(rows: &[(String, Vec<RefRow>)], state: RefsState) -> Vec<(String, Vec<RefRow>)> {
    rows.iter()
        .filter_map(|(file, sites)| {
            let kept: Vec<RefRow> = sites
                .iter()
                .filter(|row| passes(state, row))
                .cloned()
                .collect();
            (!kept.is_empty()).then(|| (file.clone(), kept))
        })
        .collect()
}

/// What the panel body should show.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PanelState {
    NoAnalysis,
    NoSelection,
    NoRefs,
    FilteredOut,
    Ready,
}

/// Decide the body state from cheap facts (kept pure so it is testable).
fn panel_state(has_analysis: bool, has_target: bool, total: usize, shown: usize) -> PanelState {
    if !has_analysis {
        PanelState::NoAnalysis
    } else if !has_target {
        PanelState::NoSelection
    } else if total == 0 {
        PanelState::NoRefs
    } else if shown == 0 {
        PanelState::FilteredOut
    } else {
        PanelState::Ready
    }
}

/// Hint shown when a selected symbol has no use sites at all.
fn no_refs_hint(target: &str) -> String {
    format!("no references found for {target}")
}

pub fn ui(ui: &mut egui::Ui, app: &mut App) {
    let id = egui::Id::new("usage-refs-state");
    let mut state = ui
        .ctx()
        .data_mut(|d| d.get_temp::<RefsState>(id))
        .unwrap_or_default();

    let Some(analysis) = app.analysis.clone() else {
        placeholder(ui, "no analysis yet — press Learn first");
        ui.ctx().data_mut(|d| d.insert_temp(id, state));
        return;
    };

    let target = match &app.selection {
        Some(Selection::Symbol(s)) => Some(s.clone()),
        _ => None,
    };
    let Some(target) = target else {
        placeholder(ui, "select a symbol to list its references");
        ui.ctx().data_mut(|d| d.insert_temp(id, state));
        return;
    };

    let set = refs_for(app, &analysis, &target);
    let total = set.len();
    header(ui, &target, &set);
    filters(ui, &mut state);
    theme::separator(ui);

    let rows = grouped_rows(&set);
    let shown = filtered_rows(&rows, state);
    let shown_count: usize = shown.iter().map(|(_, sites)| sites.len()).sum();

    match panel_state(true, true, total, shown_count) {
        PanelState::NoRefs => placeholder(ui, &no_refs_hint(&target)),
        PanelState::FilteredOut => {
            placeholder(ui, "no sites match the current filters");
        }
        PanelState::Ready => {
            if shown_count < total {
                muted(ui, &format!("showing {shown_count} of {total} site(s)"));
            }
            egui::ScrollArea::vertical()
                .id_salt("usage-refs-list")
                .show(ui, |ui| {
                    for (file, sites) in &shown {
                        file_group(ui, app, file, sites);
                    }
                });
        }
        PanelState::NoAnalysis | PanelState::NoSelection => {}
    }

    ui.ctx().data_mut(|d| d.insert_temp(id, state));
}

/// Target + `N resolved · M name-only`, driven by `references_ex`.
fn header(ui: &mut egui::Ui, target: &str, set: &RefSet) {
    theme::section_header(ui, "REFERENCES");
    ui.add(
        egui::Label::new(
            egui::RichText::new(target)
                .size(theme::FONT_BODY)
                .strong()
                .color(theme::TEXT_PRIMARY),
        )
        .truncate(),
    );
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = theme::SPACE_1;
        let (resolved, name_only) = (set.resolved.len(), set.by_name.len());
        ui.label(
            egui::RichText::new(format!("{resolved} resolved"))
                .size(theme::FONT_SMALL)
                .color(theme::TEXT_SECONDARY),
        );
        ui.label(
            egui::RichText::new("·")
                .size(theme::FONT_SMALL)
                .color(theme::TEXT_DISABLED),
        );
        let name_color = if name_only > 0 {
            theme::WARNING
        } else {
            theme::TEXT_MUTED
        };
        ui.label(
            egui::RichText::new(format!("{name_only} name-only"))
                .size(theme::FONT_SMALL)
                .color(name_color),
        )
        .on_hover_text(
            "name-only sites are unresolved: the analyzer recorded a raw name, \
             not a symbol id (risk R-20).",
        );
    });
}

/// Resolution + kind filters.
fn filters(ui: &mut egui::Ui, state: &mut RefsState) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = theme::SPACE_1;
        ui.label(
            egui::RichText::new("Show")
                .size(theme::FONT_SMALL)
                .color(theme::TEXT_MUTED),
        );
        egui::ComboBox::from_id_salt("usage-refs-res")
            .selected_text(state.res.label())
            .show_ui(ui, |ui| {
                for f in [ResFilter::All, ResFilter::Resolved, ResFilter::NameOnly] {
                    ui.selectable_value(&mut state.res, f, f.label());
                }
            });
        ui.label(
            egui::RichText::new("Kind")
                .size(theme::FONT_SMALL)
                .color(theme::TEXT_MUTED),
        );
        egui::ComboBox::from_id_salt("usage-refs-kind")
            .selected_text(state.kind.map_or("All", kind_label))
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut state.kind, None, "All");
                for k in [
                    UseKind::Call,
                    UseKind::TypeRef,
                    UseKind::FieldAccess,
                    UseKind::VarRead,
                    UseKind::VarWrite,
                    UseKind::Import,
                ] {
                    ui.selectable_value(&mut state.kind, Some(k), kind_label(k));
                }
            });
    });
}

/// One file's header + its site rows.
fn file_group(ui: &mut egui::Ui, app: &mut App, file: &str, sites: &[RefRow]) {
    ui.add_space(theme::SPACE_2);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = theme::SPACE_1;
        ui.add(
            egui::Label::new(
                egui::RichText::new(file)
                    .size(theme::FONT_SMALL)
                    .strong()
                    .color(theme::TEXT_SECONDARY),
            )
            .truncate(),
        );
        ui.label(
            egui::RichText::new(format!("({})", sites.len()))
                .size(theme::FONT_SMALL)
                .color(theme::TEXT_MUTED),
        );
    });
    for (i, row) in sites.iter().enumerate() {
        site_row(ui, app, file, i, row);
    }
}

/// One site row: `[Kind] line:col  snippet`, jump-to on click.
fn site_row(ui: &mut egui::Ui, app: &mut App, file: &str, index: usize, row: &RefRow) {
    let site = &row.site;
    let selected =
        app.open_file.as_deref() == Some(site.file.as_str()) && app.jump_line == Some(site.line);

    let mut text = format!("[{}] {}:{}", kind_label(site.kind), site.line, site.col);
    if !site.snippet.is_empty() {
        text.push_str("  ");
        text.push_str(&site.snippet);
    }
    // Name-only sites are muted + italic and carry a `?` marker so they read as
    // unconfirmed; resolved sites are secondary text.
    let (label, meta) = if row.name_only {
        (
            egui::RichText::new(format!("? {text}"))
                .size(theme::FONT_SMALL)
                .italics()
                .color(theme::TEXT_MUTED),
            Meta::tinted("name-only", theme::WARNING),
        )
    } else {
        (
            egui::RichText::new(text)
                .size(theme::FONT_SMALL)
                .color(theme::TEXT_SECONDARY),
            Meta::muted("resolved"),
        )
    };

    let resp = data_row(
        ui,
        selected,
        (
            "ref-site",
            file,
            index,
            kind_rank(site.kind),
            site.line,
            site.col,
        ),
        label,
        Some(meta),
    );
    if resp.clicked() {
        // Jump to the site but keep `app.selection` on the target, so the
        // references stay up and one selection still drives every pane.
        app.open_file_with(site.file.clone(), Some(site.line));
    }
    let hover = if row.name_only {
        format!(
            "{}:{} — name-only match: not resolved to a symbol id (R-20)",
            site.file, site.line
        )
    } else {
        format!("{}:{} — resolved", site.file, site.line)
    };
    resp.on_hover_text(hover);
}

/// Small muted hint under a section.
fn muted(ui: &mut egui::Ui, text: &str) {
    ui.label(
        egui::RichText::new(text)
            .size(theme::FONT_SMALL)
            .color(theme::TEXT_MUTED),
    );
}

/// Centered muted placeholder for the empty / no-selection states.
fn placeholder(ui: &mut egui::Ui, text: &str) {
    ui.add_space(theme::SPACE_6);
    ui.vertical_centered(|ui| {
        ui.label(
            egui::RichText::new(text)
                .size(theme::FONT_SMALL)
                .color(theme::TEXT_SECONDARY),
        );
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use repowalk_core::analysis::{VarKind, Variable};

    fn variable(id: &str, name: &str) -> Variable {
        let (file, decl_line) = id.rsplit_once(':').unwrap_or((id, "1"));
        Variable {
            id: id.into(),
            name: name.into(),
            kind: VarKind::Local,
            file: file.into(),
            decl_line: decl_line.parse().unwrap_or(1),
            scope_start: 1,
            scope_end: 40,
            type_name: None,
            reads: 0,
            writes: 0,
            first_use: None,
            last_use: None,
            enclosing: String::new(),
        }
    }

    fn site(target: &str, file: &str, line: usize, col: usize, kind: UseKind) -> UseSite {
        UseSite {
            target: target.into(),
            file: file.into(),
            line,
            col,
            kind,
            snippet: format!("line {line}"),
        }
    }

    /// A variable `q` with one resolved use (by id) and one name-only use.
    fn analysis_with_q() -> Analysis {
        let mut a = Analysis::new();
        a.variables.push(variable("src/a.rs:3:q", "q"));
        a.uses = vec![
            site("src/a.rs:3:q", "src/a.rs", 5, 9, UseKind::VarRead),
            site("q", "src/a.rs", 7, 1, UseKind::VarRead),
        ];
        a
    }

    #[test]
    fn rendering_rule_tags_resolved_vs_name_only() {
        let a = analysis_with_q();
        let set = refs::references_ex(&a, "q");
        assert_eq!((set.resolved.len(), set.by_name.len()), (1, 1));

        let rows = grouped_rows(&set);
        assert_eq!(rows.len(), 1);
        let (file, sites) = &rows[0];
        assert_eq!(file, "src/a.rs");
        assert_eq!(sites.len(), 2);
        // line 5 came from the canonical id -> resolved; line 7 is the raw name.
        let by_line = |n: usize| sites.iter().find(|r| r.site.line == n).unwrap();
        assert!(!by_line(5).name_only);
        assert!(by_line(7).name_only);
        assert_eq!(by_line(7).site.target, "q");
    }

    #[test]
    fn symbol_name_from_the_index_links_cross_file_call_sites() {
        // A function symbol id carries no name in the analysis, so the same
        // check `refs::references_ex` fails is what the panel must get right.
        let mut a = Analysis::new();
        a.uses
            .push(site("helper", "src/main.rs", 8, 20, UseKind::Call));
        a.uses
            .push(site("helper2", "src/main.rs", 9, 20, UseKind::Call));

        let direct = refs::references_ex(&a, "src/util.rs:9");
        assert!(direct.is_empty(), "{direct:?}");

        let linked = refs::references_for_symbol(&a, "src/util.rs:9", "helper");
        assert_eq!(linked.by_name.len(), 1, "{linked:?}");
        assert_eq!(linked.by_name[0].file, "src/main.rs");

        // And the panel's grouping carries it, tagged name-only.
        let rows = grouped_rows(&linked);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].0, "src/main.rs");
        assert!(rows[0].1[0].name_only);
    }

    #[test]
    fn filter_logic_keeps_the_right_classes_and_kinds() {
        let resolved = RefRow {
            site: site("id", "f.rs", 1, 1, UseKind::Call),
            name_only: false,
        };
        let by_name = RefRow {
            site: site("name", "f.rs", 2, 1, UseKind::VarRead),
            name_only: true,
        };

        let only = |res, kind| RefsState { res, kind };
        assert!(passes(only(ResFilter::All, None), &resolved));
        assert!(passes(only(ResFilter::All, None), &by_name));
        assert!(passes(only(ResFilter::Resolved, None), &resolved));
        assert!(!passes(only(ResFilter::Resolved, None), &by_name));
        assert!(!passes(only(ResFilter::NameOnly, None), &resolved));
        assert!(passes(only(ResFilter::NameOnly, None), &by_name));
        // Kind filter intersects with the resolution filter.
        assert!(passes(only(ResFilter::All, Some(UseKind::Call)), &resolved));
        assert!(!passes(only(ResFilter::All, Some(UseKind::Call)), &by_name));
        assert!(passes(
            only(ResFilter::All, Some(UseKind::VarRead)),
            &by_name
        ));

        // filtered_rows drops a file group once every row is filtered out.
        let grouped = vec![("f.rs".to_string(), vec![resolved.clone(), by_name.clone()])];
        let kept = filtered_rows(&grouped, only(ResFilter::NameOnly, None));
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].1.len(), 1);
        assert!(kept[0].1[0].name_only);
        let none = filtered_rows(&grouped, only(ResFilter::Resolved, Some(UseKind::VarWrite)));
        assert!(none.is_empty());
    }

    #[test]
    fn empty_states_are_explicit_and_never_blank() {
        assert_eq!(panel_state(false, false, 0, 0), PanelState::NoAnalysis);
        assert_eq!(panel_state(true, false, 0, 0), PanelState::NoSelection);
        assert_eq!(panel_state(true, true, 0, 0), PanelState::NoRefs);
        assert_eq!(panel_state(true, true, 3, 0), PanelState::FilteredOut);
        assert_eq!(panel_state(true, true, 3, 3), PanelState::Ready);

        // Unknown target / empty analysis: no rows, no panic.
        let empty = Analysis::new();
        assert!(grouped_rows(&RefSet::default()).is_empty());
        assert!(refs::references_ex(&empty, "anything").is_empty());

        // A selected symbol with no sites says so, with the id it was given.
        assert_eq!(no_refs_hint("greet"), "no references found for greet");
        assert_eq!(
            no_refs_hint("src/a.rs:3:q"),
            "no references found for src/a.rs:3:q"
        );
    }
}
