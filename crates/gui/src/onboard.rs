//! "Repo 101" onboarding census (v2 F1). OWNER: DSN-4.
//!
//! Shown after `learn()` completes: what the codebase is (language mix, LOC,
//! symbols), how it is shaped (module tree), where the risk is (hotspots, unused
//! variables, cycles) and where to start reading (entrypoints first). Read the
//! cached `Index` + `Analysis`; never re-analyze here.
//!
//! Data sources (all read-only — nothing is recomputed in the GUI):
//!   * `Index::files` / `Index::symbols` — language mix, LOC, symbols, entrypoints
//!   * `Analysis::modules` + `module_edges` — the module tree (populated by CORE-9)
//!   * `core::complexity::{per_file, hotspots, unused_variables}` — risk read-out
//!   * `core::modules::cycles` / `core::metrics::compute` — import cycles
//!   * `core::tour::tour` — the suggested reading order
//!
//! Every number is read verbatim. When a source is absent (no analysis, empty
//! module tree) the panel says so and falls back rather than inventing a value.
use std::collections::{BTreeMap, BTreeSet};

use eframe::egui;
use repowalk_core::analysis::{Analysis, ModuleEdgeKind, ModuleKind, ModuleNode};
use repowalk_core::complexity;
use repowalk_core::metrics;
use repowalk_core::model::Index;
use repowalk_core::modules;
use repowalk_core::tour;

use crate::app::{App, Selection};
use crate::sidebar::{data_row, Meta};
use crate::theme;

// ── Caps (the sidebar is narrow; lists stay short and scroll) ───────────────
const MAX_HOT_FILES: usize = 5;
const MAX_HOT_FUNCS: usize = 5;
const MAX_UNUSED_SHOWN: usize = 3;
const MAX_CYCLES_SHOWN: usize = 3;
const MAX_ENTRYPOINTS: usize = 5;
const MAX_TOUR: usize = 6;
const MAX_TOP_DIRS: usize = 10;
const MODULE_ROW_BUDGET: usize = 40;
const MODULE_MAX_DEPTH: usize = 4;

// ── Panel ───────────────────────────────────────────────────────────────────

/// Render the Repo 101 report for the loaded `Index` / cached `Analysis`.
pub fn ui(ui: &mut egui::Ui, app: &mut App) {
    let Some(idx) = app.index.clone() else {
        // Normally the sidebar gates this (no index → empty state), but be honest
        // if it is ever reached mid-learn.
        live_only(ui, app);
        return;
    };
    let analysis = app.analysis.clone();
    let cen = census(&idx);

    egui::ScrollArea::vertical()
        .id_salt("onboard-scroll")
        .show(ui, |ui| {
            what_this_is(ui, &cen);
            theme::separator(ui);
            how_shaped(ui, &idx, analysis.as_deref());
            theme::separator(ui);
            where_risk(ui, app, &idx, analysis.as_deref());
            theme::separator(ui);
            where_start(ui, app, &idx, &cen);
            theme::separator(ui);
            live_state(ui, app, analysis.as_deref());
        });
}

// ── 1. What this is ─────────────────────────────────────────────────────────

fn what_this_is(ui: &mut egui::Ui, cen: &Census) {
    theme::section_header(ui, "What this is");
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(cen.loc.to_string())
                .size(theme::FONT_HEADING)
                .strong()
                .color(theme::TEXT_PRIMARY),
        );
        ui.label(
            egui::RichText::new("LOC")
                .size(theme::FONT_SMALL)
                .color(theme::TEXT_MUTED),
        );
    });
    muted(
        ui,
        &format!(
            "{} files · {} symbols · {} entry point{}",
            cen.files,
            cen.symbols,
            cen.entrypoints.len(),
            if cen.entrypoints.len() == 1 { "" } else { "s" }
        ),
    );

    if cen.langs.is_empty() {
        muted(ui, "languages: unknown");
    } else {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = theme::SPACE_2;
            for (lang, n) in &cen.langs {
                ui.label(
                    egui::RichText::new(format!("{lang}: {n}"))
                        .size(theme::FONT_SMALL)
                        .color(theme::TEXT_SECONDARY),
                );
            }
        });
    }
}

// ── 2. How it is shaped ─────────────────────────────────────────────────────

fn how_shaped(ui: &mut egui::Ui, idx: &Index, analysis: Option<&Analysis>) {
    theme::section_header(ui, "How it is shaped");

    match analysis {
        Some(a) if !a.modules.is_empty() => {
            let tree = tree_of(a);
            let mut budget = MODULE_ROW_BUDGET;
            for root in &tree.roots {
                render_branch(ui, &tree, root, 0, &mut budget);
            }
            if budget == 0 {
                muted(ui, "… tree truncated");
            }
            let file_nodes = a
                .modules
                .iter()
                .filter(|n| n.kind == ModuleKind::File)
                .count();
            muted(
                ui,
                &format!("{file_nodes} file node(s) rolled into the counts above"),
            );
        }
        _ => {
            // CORE-9 is wiring `Analysis::modules`; until then fall back to the
            // top-level directories so the shape is still visible.
            muted(ui, "module tree not populated yet — showing top-level dirs");
            for (dir, files, loc) in top_level_dirs(idx).iter().take(MAX_TOP_DIRS) {
                data_row(
                    ui,
                    false,
                    ("onboard-dir", dir.as_str()),
                    dir.clone(),
                    Some(Meta::muted(&format!("{files} files · {loc} LOC"))),
                );
            }
        }
    }
}

/// The module-tree view: `Submodule` edges give the parent → child shape.
struct Tree<'a> {
    children: BTreeMap<&'a str, Vec<&'a str>>,
    nodes: BTreeMap<&'a str, &'a ModuleNode>,
    roots: Vec<&'a str>,
}

fn tree_of(analysis: &Analysis) -> Tree<'_> {
    let mut children: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    let mut has_parent: BTreeSet<&str> = BTreeSet::new();
    for e in &analysis.module_edges {
        if e.kind == ModuleEdgeKind::Submodule {
            children
                .entry(e.from.as_str())
                .or_default()
                .push(e.to.as_str());
            has_parent.insert(e.to.as_str());
        }
    }
    for kids in children.values_mut() {
        kids.sort_unstable();
    }
    let nodes: BTreeMap<&str, &ModuleNode> = analysis
        .modules
        .iter()
        .map(|n| (n.id.as_str(), n))
        .collect();
    let roots: Vec<&str> = analysis
        .modules
        .iter()
        .map(|n| n.id.as_str())
        .filter(|id| !has_parent.contains(id))
        .collect();
    Tree {
        children,
        nodes,
        roots,
    }
}

/// Depth-first module rows. Only `Crate`/`Dir`/`Module` nodes are shown (files
/// are many and are represented by their parents' rolled-up counts). The depth
/// cap plus the row budget guarantee termination and bounded height.
fn render_branch(ui: &mut egui::Ui, tree: &Tree<'_>, id: &str, depth: usize, budget: &mut usize) {
    if *budget == 0 || depth > MODULE_MAX_DEPTH {
        return;
    }
    if let Some(node) = tree.nodes.get(id).copied() {
        if node.kind != ModuleKind::File {
            *budget -= 1;
            module_row(ui, node, depth);
        }
    }
    if let Some(kids) = tree.children.get(id) {
        for child in kids {
            render_branch(ui, tree, child, depth + 1, budget);
        }
    }
}

fn module_row(ui: &mut egui::Ui, node: &ModuleNode, depth: usize) {
    ui.horizontal(|ui| {
        ui.add_space(depth as f32 * theme::TREE_INDENT);
        let (glyph, color) = match node.kind {
            ModuleKind::Crate => ("▣", theme::INFO),
            ModuleKind::Module => ("◆", theme::module_color(&node.id)),
            ModuleKind::Dir => ("▸", theme::TEXT_SECONDARY),
            ModuleKind::File => ("·", theme::TEXT_MUTED),
        };
        ui.label(
            egui::RichText::new(format!("{glyph} {}", tail(&node.id)))
                .size(theme::FONT_SMALL)
                .color(color)
                .strong(),
        );
        ui.label(
            egui::RichText::new(format!("{} loc · {} sym", node.loc, node.symbols))
                .size(theme::FONT_SMALL)
                .color(theme::TEXT_MUTED),
        );
    });
}

// ── 3. Where the risk is ────────────────────────────────────────────────────

fn where_risk(ui: &mut egui::Ui, app: &mut App, idx: &Index, analysis: Option<&Analysis>) {
    theme::section_header(ui, "Where the risk is");
    let Some(a) = analysis else {
        muted(ui, "unknown — no analysis yet (press Learn)");
        return;
    };
    let r = risk(a);

    // Most complex files.
    if r.hot_files.is_empty() {
        muted(ui, "hot files: none (no control-flow data)");
    } else {
        muted(ui, "most complex files");
        for (i, (file, total, max)) in r.hot_files.iter().enumerate() {
            let selected = app.open_file.as_deref() == Some(file.as_str());
            let meta = format!("cx {total} · max {max}");
            let resp = data_row(
                ui,
                selected,
                ("onboard-hot-file", i),
                short_path(file),
                Some(Meta::tinted(&meta, theme::WARNING)),
            );
            if resp.clicked() {
                let path = file.clone();
                app.select(Selection::File(path));
            }
            resp.on_hover_text(file.as_str());
        }
    }

    // Most complex functions.
    if r.hot_functions.is_empty() {
        muted(ui, "hot functions: none (no control-flow data)");
    } else {
        muted(ui, "most complex functions");
        for (i, (sym, cx)) in r.hot_functions.iter().enumerate() {
            let selected = app.selection.as_ref() == Some(&Selection::Symbol(sym.clone()));
            let label = idx
                .symbols
                .iter()
                .find(|s| s.id == *sym)
                .map(|s| s.name.clone())
                .unwrap_or_else(|| short_path(&complexity::parse_symbol_id(sym).0));
            let resp = data_row(
                ui,
                selected,
                ("onboard-hot-fn", i),
                label,
                Some(Meta::tinted(&format!("cx {cx}"), theme::WARNING)),
            );
            if resp.clicked() {
                let id = sym.clone();
                app.select(Selection::Symbol(id));
            }
            resp.on_hover_text(sym.as_str());
        }
    }

    // Unused variables (dead-code candidates).
    let unused = complexity::unused_variables(a);
    muted(
        ui,
        &format!(
            "{} unused variable{}",
            unused.len(),
            if unused.len() == 1 { "" } else { "s" }
        ),
    );
    for (i, v) in unused.iter().take(MAX_UNUSED_SHOWN).enumerate() {
        let label = format!("{} · {}", short_path(&v.file), v.name);
        let resp = data_row(
            ui,
            false,
            ("onboard-unused", i),
            label,
            Some(Meta::muted(&format!("L{}", v.decl_line))),
        );
        if resp.clicked() {
            // One navigation path: opens a tab too, so the tab strip
            // follows the editor instead of showing the previous file.
            let file = v.file.clone();
            let line = v.decl_line;
            app.goto_file(file, Some(line), None);
        }
        resp.on_hover_text(format!("{}:{}:{}", v.file, v.decl_line, v.name));
    }
    if unused.len() > MAX_UNUSED_SHOWN {
        muted(ui, &format!("+{} more", unused.len() - MAX_UNUSED_SHOWN));
    }

    // Import cycles.
    let cycles = cycle_groups(idx, a);
    muted(
        ui,
        &format!(
            "{} import cycle{}",
            cycles.len(),
            if cycles.len() == 1 { "" } else { "s" }
        ),
    );
    for (i, cyc) in cycles.iter().take(MAX_CYCLES_SHOWN).enumerate() {
        let label = cyc
            .iter()
            .map(|s| short_path(&complexity::parse_symbol_id(s).0))
            .collect::<Vec<_>>()
            .join(" ↔ ");
        let meta = format!("{} node(s)", cyc.len());
        let resp = data_row(
            ui,
            false,
            ("onboard-cycle", i),
            egui::RichText::new(label).color(theme::DANGER),
            Some(Meta::tinted(&meta, theme::DANGER)),
        );
        if resp.clicked() {
            if let Some(first) = cyc.first() {
                let (file, line) = complexity::parse_symbol_id(first);
                if idx.files.iter().any(|f| f.path == file) {
                    app.goto_file(file, (line > 0).then_some(line), None);
                }
            }
        }
        resp.on_hover_text(cyc.join(" ↔ "));
    }
    if cycles.len() > MAX_CYCLES_SHOWN {
        muted(ui, &format!("+{} more", cycles.len() - MAX_CYCLES_SHOWN));
    }
}

// ── 4. Where to start ───────────────────────────────────────────────────────

fn where_start(ui: &mut egui::Ui, app: &mut App, idx: &Index, cen: &Census) {
    theme::section_header(ui, "Where to start");

    // Entry points first: the first thing to read in an unfamiliar repo.
    if cen.entrypoints.is_empty() {
        muted(ui, "entry points: none detected");
    } else {
        muted(ui, "entry points");
        for (i, path) in cen.entrypoints.iter().take(MAX_ENTRYPOINTS).enumerate() {
            let selected = app.open_file.as_deref() == Some(path.as_str());
            let resp = data_row(
                ui,
                selected,
                ("onboard-entry", i),
                short_path(path),
                Some(Meta::badge("entry")),
            );
            if resp.clicked() {
                let p = path.clone();
                app.select(Selection::File(p));
            }
            resp.on_hover_text(path.as_str());
        }
    }

    // Then the tour's ranked order (fan-in + entrypoint + docs − depth, SCCs
    // collapsed); reuse core's ranking rather than re-deriving it here.
    muted(ui, "suggested reading order");
    let items = tour::tour(idx, MAX_TOUR + cen.entrypoints.len());
    let mut shown = 0usize;
    for item in &items {
        if cen.entrypoints.contains(&item.path) {
            continue;
        }
        if shown >= MAX_TOUR {
            break;
        }
        let selected = app.open_file.as_deref() == Some(item.path.as_str());
        let resp = data_row(
            ui,
            selected,
            ("onboard-read", shown),
            short_path(&item.path),
            Some(Meta::badge("read")),
        );
        if resp.clicked() {
            let path = item.path.clone();
            app.select(Selection::File(path));
        }
        resp.on_hover_text(item.path.as_str());
        ui.horizontal(|ui| {
            ui.add_space(theme::SPACE_2);
            ui.add(
                egui::Label::new(
                    egui::RichText::new(&item.why)
                        .size(theme::FONT_SMALL)
                        .color(theme::TEXT_MUTED),
                )
                .truncate(),
            );
        });
        shown += 1;
    }
    if shown == 0 {
        muted(ui, "no reading order available");
    }
}

// ── 5. Live state ───────────────────────────────────────────────────────────

/// No index at all: show the learn phase if one is running, else the prompt.
fn live_only(ui: &mut egui::Ui, app: &App) {
    theme::section_header(ui, "Repo 101");
    let (phase, done, total) = progress(app);
    if phase.is_empty() {
        muted(ui, "no repository loaded — press Learn");
    } else {
        muted(ui, &format!("learning · {phase} · {done}/{total} files"));
    }
}

/// The learn phase (driven by `app.progress`) plus the analysis counts once they
/// land. When there is no analysis yet, say so instead of showing empty numbers.
fn live_state(ui: &mut egui::Ui, app: &App, analysis: Option<&Analysis>) {
    theme::section_header(ui, "Status");
    let (phase, done, total, finished) = {
        let p = app.progress.lock().unwrap();
        (p.phase.clone(), p.done, p.total, p.finished)
    };
    if !finished && !phase.is_empty() && done < total {
        muted(ui, &format!("learning · {phase} · {done}/{total} files"));
    } else if let Some(a) = analysis {
        muted(
            ui,
            &format!(
                "learned · {} variables · {} cfgs · {} types",
                a.variables.len(),
                a.cfgs.len(),
                a.types.len()
            ),
        );
    } else {
        muted(ui, "analysis not loaded — press Learn");
    }
}

fn progress(app: &App) -> (String, usize, usize) {
    let p = app.progress.lock().unwrap();
    (p.phase.clone(), p.done, p.total)
}

// ── Pure census / risk reads (unit-testable, no egui) ───────────────────────

/// The static census: what the repo *is*, derived from the `Index` alone.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Census {
    pub files: usize,
    pub loc: usize,
    pub symbols: usize,
    /// Entrypoint paths, sorted.
    pub entrypoints: Vec<String>,
    /// `(language, file count)`, count desc then language asc.
    pub langs: Vec<(String, usize)>,
}

/// Language mix from `index.files[].lang`: count desc, then name asc.
pub(crate) fn lang_mix(idx: &Index) -> Vec<(String, usize)> {
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for f in &idx.files {
        *counts.entry(f.lang.as_str()).or_insert(0) += 1;
    }
    let mut langs: Vec<(String, usize)> = counts
        .into_iter()
        .map(|(lang, n)| (lang.to_string(), n))
        .collect();
    langs.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    langs
}

pub(crate) fn census(idx: &Index) -> Census {
    let mut entrypoints: Vec<String> = idx
        .files
        .iter()
        .filter(|f| f.is_entrypoint)
        .map(|f| f.path.clone())
        .collect();
    entrypoints.sort();
    Census {
        files: idx.files.len(),
        loc: idx.files.iter().map(|f| f.lines).sum(),
        symbols: idx.symbols.len(),
        entrypoints,
        langs: lang_mix(idx),
    }
}

/// The risk read-out from the cached `Analysis` (F6). All values verbatim.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Risk {
    /// `(file, total, max)` — most complex files first.
    pub hot_files: Vec<(String, u32, u32)>,
    /// `(symbol id, cyclomatic)` — most complex functions first.
    pub hot_functions: Vec<(String, u32)>,
    pub unused: usize,
}

pub(crate) fn risk(analysis: &Analysis) -> Risk {
    let hot_files = complexity::per_file(analysis)
        .into_iter()
        .take(MAX_HOT_FILES)
        .map(|f| (f.file, f.total, f.max))
        .collect();
    let hot_functions = complexity::hotspots(analysis, MAX_HOT_FUNCS)
        .into_iter()
        .map(|h| (h.symbol, h.cyclomatic))
        .collect();
    Risk {
        hot_files,
        hot_functions,
        unused: complexity::unused_variables(analysis).len(),
    }
}

/// Import cycles: module-level when the module tree is populated, else the v1
/// symbol-level cycles from `metrics::compute`. Always read from core.
pub(crate) fn cycle_groups(idx: &Index, analysis: &Analysis) -> Vec<Vec<String>> {
    if analysis.modules.is_empty() {
        metrics::compute(idx).cycles
    } else {
        modules::cycles(&analysis.modules, &analysis.module_edges)
    }
}

/// Top-level directories from `index.files` — used when `Analysis::modules` is
/// empty. `(dir, files, loc)`, files desc then dir asc.
pub(crate) fn top_level_dirs(idx: &Index) -> Vec<(String, usize, usize)> {
    let mut acc: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    for f in &idx.files {
        let dir = match f.path.split_once('/') {
            Some((first, _)) => first.to_string(),
            None => "(root)".to_string(),
        };
        let e = acc.entry(dir).or_insert((0, 0));
        e.0 += 1;
        e.1 += f.lines;
    }
    let mut out: Vec<(String, usize, usize)> = acc
        .into_iter()
        .map(|(dir, (files, loc))| (dir, files, loc))
        .collect();
    out.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    out
}

// ── Small helpers ───────────────────────────────────────────────────────────

fn muted(ui: &mut egui::Ui, text: &str) {
    ui.label(
        egui::RichText::new(text)
            .size(theme::FONT_SMALL)
            .color(theme::TEXT_MUTED),
    );
}

/// `crates/gui/src/sidebar/mod.rs` → `sidebar/mod.rs` (last two segments).
fn short_path(path: &str) -> String {
    let mut parts: Vec<&str> = path.rsplit('/').take(2).collect();
    parts.reverse();
    parts.join("/")
}

/// Last path segment of a module id (the tree indents for hierarchy).
fn tail(id: &str) -> String {
    id.rsplit('/').next().unwrap_or(id).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use repowalk_core::analysis::{Analysis, ModuleEdge, ModuleNode};
    use repowalk_core::model::{FileEntry, Symbol, SymbolKind};

    fn file(path: &str, lang: &str, lines: usize, entry: bool) -> FileEntry {
        FileEntry {
            path: path.into(),
            lang: lang.into(),
            lines,
            mtime: 0,
            is_entrypoint: entry,
        }
    }

    fn sym(file: &str, line: usize) -> Symbol {
        Symbol {
            id: format!("{file}:{line}"),
            name: "f".into(),
            kind: SymbolKind::Function,
            signature: String::new(),
            doc: String::new(),
            start_line: line,
            end_line: line,
            container: None,
            file: file.into(),
        }
    }

    #[test]
    fn census_counts_files_loc_symbols_and_language_mix() {
        let mut idx = Index::new("/repo");
        idx.files = vec![
            file("src/main.rs", "rust", 10, true),
            file("src/lib.rs", "rust", 20, false),
            file("tool.py", "python", 5, false),
            file("app.ts", "typescript", 7, false),
        ];
        idx.symbols = vec![sym("src/main.rs", 1), sym("src/lib.rs", 2)];

        let c = census(&idx);
        assert_eq!(c.files, 4);
        assert_eq!(c.loc, 42);
        assert_eq!(c.symbols, 2);
        assert_eq!(
            c.langs,
            vec![
                ("rust".to_string(), 2),
                ("python".to_string(), 1),
                ("typescript".to_string(), 1),
            ]
        );
    }

    #[test]
    fn census_counts_entrypoints_sorted() {
        let mut idx = Index::new("/repo");
        idx.files = vec![
            file("src/main.rs", "rust", 1, true),
            file("src/bin/a.rs", "rust", 1, true),
            file("src/lib.rs", "rust", 1, false),
        ];

        let c = census(&idx);
        assert_eq!(
            c.entrypoints,
            vec!["src/bin/a.rs".to_string(), "src/main.rs".to_string()]
        );
    }

    #[test]
    fn empty_analysis_risk_cycles_and_fallback_do_not_panic() {
        let mut idx = Index::new("/repo");
        idx.files = vec![file("src/main.rs", "rust", 10, true)];
        let a = Analysis::new();

        // An empty analysis yields empty aggregates (no panic, no invented value).
        assert_eq!(risk(&a), Risk::default());
        assert!(cycle_groups(&idx, &a).is_empty());

        // The module-tree fallback still describes the shape from the index.
        assert_eq!(top_level_dirs(&idx), vec![("src".to_string(), 1, 10)]);
    }

    #[test]
    fn module_tree_roots_are_nodes_without_parents() {
        let mut a = Analysis::new();
        a.modules = vec![
            ModuleNode {
                id: "repo".into(),
                kind: ModuleKind::Crate,
                loc: 30,
                symbols: 2,
            },
            ModuleNode {
                id: "src".into(),
                kind: ModuleKind::Module,
                loc: 30,
                symbols: 2,
            },
            ModuleNode {
                id: "src/lib.rs".into(),
                kind: ModuleKind::File,
                loc: 30,
                symbols: 2,
            },
            ModuleNode {
                id: "orphan".into(),
                kind: ModuleKind::Dir,
                loc: 0,
                symbols: 0,
            },
        ];
        a.module_edges = vec![
            ModuleEdge {
                from: "repo".into(),
                to: "src".into(),
                kind: ModuleEdgeKind::Submodule,
                count: 1,
            },
            ModuleEdge {
                from: "src".into(),
                to: "src/lib.rs".into(),
                kind: ModuleEdgeKind::Submodule,
                count: 1,
            },
        ];

        let tree = tree_of(&a);
        assert_eq!(tree.roots, vec!["repo", "orphan"]);
        assert_eq!(tree.children.get("repo").cloned(), Some(vec!["src"]));
    }

    /// The census/risk reads consume the *real* `(Index, Analysis)` from `learn`,
    /// not a hand-built pair, and stay consistent with the artifacts.
    #[test]
    fn real_learn_pipeline_feeds_the_census() {
        let root = std::env::temp_dir().join(format!("rw-onboard-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(
            root.join("src/main.rs"),
            "fn main() {\n    let unused = 1;\n    println!(\"hi\");\n}\n",
        )
        .unwrap();
        std::fs::write(
            root.join("src/lib.rs"),
            "pub fn add(a: i32, b: i32) -> i32 {\n    if a > b { a } else { b }\n}\n",
        )
        .unwrap();

        let (idx, analysis) = repowalk_core::learn::learn(&root, &|_| {}).unwrap();
        let c = census(&idx);
        assert_eq!(c.files, idx.files.len());
        assert_eq!(c.loc, idx.files.iter().map(|f| f.lines).sum::<usize>());
        assert_eq!(c.symbols, idx.symbols.len());
        assert!(c.langs.iter().any(|(lang, _)| lang == "rust"));
        assert!(c.entrypoints.iter().any(|p| p.ends_with("main.rs")));

        let r = risk(&analysis);
        assert_eq!(r.unused, complexity::unused_variables(&analysis).len());
        // Cycles read from the same analysis; deterministic and panic-free.
        assert_eq!(cycle_groups(&idx, &analysis), cycle_groups(&idx, &analysis));

        eprintln!(
            "ONBOARD census: files={} loc={} symbols={} entrypoints={:?} langs={:?}",
            c.files, c.loc, c.symbols, c.entrypoints, c.langs
        );
        eprintln!(
            "ONBOARD risk: hot_files={:?} hot_functions={:?} unused={} cycles={:?}",
            r.hot_files,
            r.hot_functions,
            r.unused,
            cycle_groups(&idx, &analysis)
        );

        let _ = std::fs::remove_dir_all(&root);
    }
}
