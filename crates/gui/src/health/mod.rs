//! Repository health panel (v2 F6) — the "where is the risk" view. OWNER: DSN-3.
//!
//! This is a pure read of core: the v1 overview strip comes from
//! [`metrics::compute`]; complexity-by-module, god files, hotspots and the
//! unused-variable count come from [`metrics::compute_v2`] (which never
//! recomputes anything here); the architecture guardrails come from
//! [`rules::load`] + [`rules::check`]. Findings arrive pre-sorted and are
//! rendered in that order.
//!
//! ## Caching (without an `app.rs` edit)
//!
//! `App` has no health-cache field and this card owns `health/**` only. The
//! bundle is therefore cached in egui's temp memory under a signature key that
//! mirrors `App::graph_state`'s `((edges, symbols, …), built)` pattern: the
//! fingerprint is the `(index Arc ptr, file/symbol/edge/import counts,
//! analysis Arc ptr, cfg/variable/type/module counts)` tuple. A frame whose key
//! matches reuses the cached `Arc<…>` bundles, so the v1+v2 metrics, the
//! module→file index and the findings are computed **once per artifact**, not
//! once per frame.
//!
//! ## Honest states
//!
//! No repository → a notice. Index present but the v2 analysis missing → the
//! overview still renders, but each v2 row says which analysis artifact it
//! needs instead of showing a `0` that would read as a result. An empty
//! findings list is called out as "no findings", distinct from "no guardrails
//! configured".
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use eframe::egui;
use repowalk_core::analysis::{Analysis, ModuleEdgeKind, ModuleKind};
use repowalk_core::complexity::Hotspot;
use repowalk_core::metrics::{self, GodFile, Metrics, MetricsV2, ModuleComplexity};
use repowalk_core::rules::{self, Finding, Rules, Severity};

use crate::app::{App, Mode, Selection};
use crate::sidebar::{data_row, Meta};
use crate::theme;

/// Module rows rendered before the list collapses to a "+N more" hint.
const MAX_MODULES: usize = 40;
/// Hotspots shown (top N by cyclomatic; the bundle is already ranked).
const MAX_HOTSPOTS: usize = 12;
/// Finding / god-file rows rendered before a "+N more" hint.
const MAX_ROWS: usize = 30;

/// Basenames that make a file the natural entry point of its module (mirrors
/// the module-root set core uses to type a directory as a `Module`).
const MODULE_ENTRY_FILES: &[&str] = &[
    "mod.rs",
    "lib.rs",
    "main.rs",
    "__init__.py",
    "__init__.pyi",
    "index.ts",
    "index.tsx",
    "index.js",
    "index.jsx",
    "index.mjs",
    "index.cjs",
    "mod.ts",
];

/// v2 rows that cannot be rendered without the analysis artifact, with the
/// reason. Shown as an explicit "unavailable" list — never as a bare zero.
const V2_ROWS: &[(&str, &str)] = &[
    (
        "Complexity by module",
        "needs the v2 analysis (`metrics::complexity_by_module`)",
    ),
    (
        "God files",
        "needs the v2 analysis (fan-out counts call sites)",
    ),
    ("Hotspots", "needs the v2 analysis (function CFGs)"),
    (
        "Unused variables",
        "needs the v2 analysis (variable read counts)",
    ),
];

// ── Cache ───────────────────────────────────────────────────────────────────

/// Fingerprint guarding a rebuild, on the `App::graph_state` signature pattern.
/// The `Arc` pointers change on every `learn`, and the counts catch an in-place
/// mutation; together they mean "same key ⇒ same artifacts".
#[derive(Clone, PartialEq, Eq)]
struct CacheKey {
    idx: usize,
    an: usize,
    files: usize,
    symbols: usize,
    edges: usize,
    imports: usize,
    cfgs: usize,
    variables: usize,
    types: usize,
    modules: usize,
}

impl CacheKey {
    fn of(idx: &Arc<repowalk_core::model::Index>, analysis: Option<&Arc<Analysis>>) -> Self {
        Self {
            idx: Arc::as_ptr(idx) as usize,
            an: analysis.map_or(0, |a| Arc::as_ptr(a) as usize),
            files: idx.files.len(),
            symbols: idx.symbols.len(),
            edges: idx.edges.len(),
            imports: idx.imports.len(),
            cfgs: analysis.map_or(0, |a| a.cfgs.len()),
            variables: analysis.map_or(0, |a| a.variables.len()),
            types: analysis.map_or(0, |a| a.types.len()),
            modules: analysis.map_or(0, |a| a.modules.len()),
        }
    }
}

/// Everything the panel reads, computed once per artifact and cached.
#[derive(Clone)]
struct Cache {
    key: CacheKey,
    v1: Arc<Metrics>,
    /// `None` when no analysis artifact has landed yet.
    v2: Option<Arc<MetricsV2>>,
    /// Module node id → descendant file paths (repo-relative).
    module_files: Arc<BTreeMap<String, Vec<String>>>,
    /// Guardrail findings, pre-sorted by `rules::check`.
    findings: Arc<Vec<Finding>>,
    /// Whether any rule is configured (distinct from "no findings").
    rules_configured: bool,
}

impl Cache {
    fn build(
        key: CacheKey,
        idx: &repowalk_core::model::Index,
        analysis: Option<&Analysis>,
        repo: Option<&Path>,
    ) -> Self {
        let v1 = Arc::new(metrics::compute(idx));
        let v2 = analysis.map(|a| Arc::new(metrics::compute_v2(idx, a)));
        let module_files = Arc::new(if analysis.is_some() {
            module_files_map(idx)
        } else {
            BTreeMap::new()
        });

        // The rule config can change on disk; it is read with the artifact, so a
        // config edit is picked up on the next `learn`/re-open (no per-frame I/O).
        let rules = repo.map(rules::load).unwrap_or_default();
        let rules_configured = rules_configured(&rules);
        // Without an analysis the index-only rules (imports, cycles, file LOC)
        // still run; the cyclomatic rule is reported as skipped in the panel.
        let empty = Analysis::new();
        let a = analysis.unwrap_or(&empty);
        let findings = Arc::new(rules::check(idx, a, &rules));

        Self {
            key,
            v1,
            v2,
            module_files,
            findings,
            rules_configured,
        }
    }
}

// ── Rendering ───────────────────────────────────────────────────────────────

pub fn ui(ui: &mut egui::Ui, app: &mut App) {
    let Some(idx) = app.index.clone() else {
        notice(
            ui,
            "Repository health",
            &["no repository loaded — open a repo to score it"],
        );
        return;
    };
    // Cheap Arc clones; the analysis itself is never deep-cloned per frame.
    let analysis = app.analysis.clone();
    let repo = app.repo.clone();

    let key = CacheKey::of(&idx, analysis.as_ref());
    let cache_id = egui::Id::new("health-v2-cache");
    let cache: Cache = match ui.ctx().data_mut(|d| d.get_temp::<Cache>(cache_id)) {
        Some(c) if c.key == key => c,
        _ => {
            let built = Cache::build(key, &idx, analysis.as_deref(), repo.as_deref());
            ui.ctx()
                .data_mut(|d| d.insert_temp(cache_id, built.clone()));
            built
        }
    };

    egui::ScrollArea::vertical()
        .id_salt("health-scroll")
        .show(ui, |ui| {
            overview(ui, &idx, &cache.v1);
            theme::separator(ui);
            guardrails(ui, app, &cache);
            theme::separator(ui);
            match &cache.v2 {
                Some(v2) => {
                    complexity_table(ui, app, &cache.module_files, &v2.complexity_by_module);
                    theme::separator(ui);
                    god_files(ui, app, &v2.god_files);
                    theme::separator(ui);
                    hotspot_list(ui, app, &v2.hotspots);
                    theme::separator(ui);
                    unused_variables(ui, v2.unused_variables);
                }
                None => unavailable_rows(ui),
            }
        });
}

/// Overview strip: files · LOC · symbols · cycles (all from `metrics::compute`).
fn overview(ui: &mut egui::Ui, idx: &repowalk_core::model::Index, v1: &Metrics) {
    theme::section_header(ui, "Overview");
    let cycles = v1.cycles.len();
    ui.horizontal_wrapped(|ui| {
        metric(
            ui,
            &idx.files.len().to_string(),
            "files",
            theme::TEXT_PRIMARY,
        );
        dot(ui);
        metric(ui, &v1.total_loc.to_string(), "LOC", theme::TEXT_PRIMARY);
        dot(ui);
        metric(
            ui,
            &idx.symbols.len().to_string(),
            "symbols",
            theme::TEXT_PRIMARY,
        );
        dot(ui);
        metric(
            ui,
            &cycles.to_string(),
            "cycles",
            if cycles == 0 {
                theme::SUCCESS
            } else {
                theme::WARNING
            },
        );
    });
}

fn metric(ui: &mut egui::Ui, value: &str, label: &str, color: egui::Color32) {
    ui.label(
        egui::RichText::new(value)
            .size(theme::FONT_BODY)
            .strong()
            .color(color),
    );
    ui.label(
        egui::RichText::new(label)
            .size(theme::FONT_SMALL)
            .color(theme::TEXT_MUTED),
    );
}

fn dot(ui: &mut egui::Ui) {
    ui.label(
        egui::RichText::new("·")
            .size(theme::FONT_SMALL)
            .color(theme::TEXT_DISABLED),
    );
}

/// Guardrail findings, each tinted by severity and labelled with its rule.
fn guardrails(ui: &mut egui::Ui, app: &mut App, cache: &Cache) {
    theme::section_header(ui, "Guardrails");
    if !cache.rules_configured {
        muted(
            ui,
            "no guardrails configured — add .repowalk/rules.json (or rules.json)",
        );
        return;
    }
    if cache.v2.is_none() {
        muted(
            ui,
            "index-only rules below; the cyclomatic rule needs the v2 analysis",
        );
    }
    // A clean run says so; otherwise the headline counts by severity. Either way
    // the section is never blank.
    let headline = findings_headline(&cache.findings);
    let color = if cache.findings.is_empty() {
        theme::SUCCESS
    } else {
        theme::DANGER
    };
    ui.label(
        egui::RichText::new(headline)
            .size(theme::FONT_SMALL)
            .color(color),
    );
    if cache.findings.is_empty() {
        return;
    }
    for f in cache.findings.iter().take(MAX_ROWS) {
        finding_row(ui, app, f);
    }
    if cache.findings.len() > MAX_ROWS {
        muted(
            ui,
            &format!("+{} more findings", cache.findings.len() - MAX_ROWS),
        );
    }
}

fn finding_row(ui: &mut egui::Ui, app: &mut App, f: &Finding) {
    let color = severity_color(f.severity);
    let location = if f.line == 0 {
        f.file.clone()
    } else {
        format!("{}:{}", f.file, f.line)
    };
    let resp = data_row(
        ui,
        false,
        ("health-finding", f.rule.as_str(), f.file.as_str(), f.line),
        finding_widget(f),
        Some(Meta::tinted(&location, color)),
    );
    if resp.clicked() && !f.file.is_empty() {
        goto(app, f.file.clone(), (f.line > 0).then_some(f.line));
    }
    resp.on_hover_text(format!(
        "[{}] {}\n{}\n{}",
        severity_tag(f.severity),
        f.rule,
        location,
        f.message
    ));
}

/// Sorted module-complexity table; a click opens the module's entry file in
/// Text mode (the sidebar has no multi-file view, so the row opens the module's
/// natural entry — `lib.rs`/`main.rs`/`mod.rs`/`index.*` — and the hover names
/// the row's file count).
fn complexity_table(
    ui: &mut egui::Ui,
    app: &mut App,
    module_files: &BTreeMap<String, Vec<String>>,
    rows: &[ModuleComplexity],
) {
    theme::section_header(ui, "Complexity by module");
    if rows.is_empty() {
        muted(ui, "no function CFGs to score");
        return;
    }
    for m in rows.iter().take(MAX_MODULES) {
        let selected = module_is_open(module_files, &m.id, app.open_file.as_deref());
        let meta = format!("Σ{} · max {} · {} fn", m.total, m.max, m.functions);
        let resp = data_row(
            ui,
            selected,
            ("health-module", m.id.as_str()),
            module_widget(m),
            Some(Meta::muted(&meta)),
        );
        let entry = if resp.clicked() {
            module_files
                .get(&m.id)
                .and_then(|fs| module_entry_file(fs))
                .map(str::to_string)
        } else {
            None
        };
        if let Some(path) = entry {
            goto(app, path, None);
        }
        resp.on_hover_text(format!(
            "{} ({}) · {} file(s) · {} function(s) · total {}, max {}\nclick to open the module's entry file in Text mode",
            m.id,
            kind_label(m.kind),
            m.files,
            m.functions,
            m.total,
            m.max
        ));
    }
    if rows.len() > MAX_MODULES {
        muted(ui, &format!("+{} more modules", rows.len() - MAX_MODULES));
    }
}

/// God files: the rule that flagged each one is always shown, never a bare LOC.
fn god_files(ui: &mut egui::Ui, app: &mut App, rows: &[GodFile]) {
    theme::section_header(ui, "God files");
    if rows.is_empty() {
        muted(ui, "none — no file trips the LOC or fan-out p99 rule");
        return;
    }
    for g in rows.iter().take(MAX_ROWS) {
        let resp = data_row(
            ui,
            false,
            ("health-god", g.path.as_str()),
            short_path(&g.path),
            Some(Meta::tinted(&god_file_meta(g), theme::WARNING)),
        );
        if resp.clicked() {
            goto(app, g.path.clone(), None);
        }
        resp.on_hover_text(format!(
            "{}\nrule(s): {}\n{} L · fan-out {}",
            g.path,
            g.reasons.join(", "),
            g.loc,
            g.fan_out
        ));
    }
    if rows.len() > MAX_ROWS {
        muted(ui, &format!("+{} more god files", rows.len() - MAX_ROWS));
    }
}

/// Top functions by cyclomatic complexity; a click jumps to the function line.
fn hotspot_list(ui: &mut egui::Ui, app: &mut App, rows: &[Hotspot]) {
    theme::section_header(ui, "Hotspots");
    if rows.is_empty() {
        muted(ui, "no function CFGs to score");
        return;
    }
    for h in rows.iter().take(MAX_HOTSPOTS) {
        let selected = app
            .open_file
            .as_deref()
            .is_some_and(|p| p == h.file.as_str() && app.jump_line == Some(h.line));
        let meta = format!("cyc {} · nest {}", h.cyclomatic, h.max_nesting);
        let resp = data_row(
            ui,
            selected,
            ("health-hotspot", h.symbol.as_str()),
            format!("{}:{}", short_path(&h.file), h.line),
            Some(Meta::tinted(
                &meta,
                if h.cyclomatic >= 20 {
                    theme::WARNING
                } else {
                    theme::TEXT_MUTED
                },
            )),
        );
        if resp.clicked() {
            goto(app, h.file.clone(), Some(h.line));
        }
        resp.on_hover_text(format!(
            "{} · line {}\ncyclomatic {} · max nesting {}",
            h.symbol, h.line, h.cyclomatic, h.max_nesting
        ));
    }
    if rows.len() > MAX_HOTSPOTS {
        muted(ui, &format!("+{} more hotspots", rows.len() - MAX_HOTSPOTS));
    }
}

/// Never-read variables. A real `0` (analysis present) is a result, not missing.
fn unused_variables(ui: &mut egui::Ui, count: usize) {
    theme::section_header(ui, "Unused variables");
    let (text, color) = if count == 0 {
        (
            "0 never-read variables — no dead-code candidates".to_string(),
            theme::SUCCESS,
        )
    } else {
        (
            format!("{count} never-read variable(s) — dead-code candidates"),
            theme::WARNING,
        )
    };
    ui.label(
        egui::RichText::new(text)
            .size(theme::FONT_SMALL)
            .color(color),
    );
}

/// Explicit "unavailable" list for the v2 rows when the analysis is missing.
fn unavailable_rows(ui: &mut egui::Ui) {
    theme::section_header(ui, "Complexity · god files · hotspots · unused");
    muted(
        ui,
        "unavailable — these rows need the v2 analysis, which has not landed:",
    );
    for (row, why) in V2_ROWS {
        ui.horizontal_wrapped(|ui| {
            ui.label(
                egui::RichText::new(format!("• {row}"))
                    .size(theme::FONT_SMALL)
                    .color(theme::TEXT_SECONDARY),
            );
            ui.label(
                egui::RichText::new(*why)
                    .size(theme::FONT_SMALL)
                    .color(theme::TEXT_MUTED),
            );
        });
    }
}

// ── Pure helpers (unit-tested; no UI) ───────────────────────────────────────

/// `Severity` → the semantic token the panel tints with.
fn severity_color(severity: Severity) -> egui::Color32 {
    match severity {
        Severity::Error => theme::DANGER,
        Severity::Warning => theme::WARNING,
    }
}

fn severity_tag(severity: Severity) -> &'static str {
    match severity {
        Severity::Error => "error",
        Severity::Warning => "warn",
    }
}

/// The clean-vs-findings render rule: a clean run says so rather than showing
/// nothing; otherwise it counts by severity.
fn findings_headline(findings: &[Finding]) -> String {
    if findings.is_empty() {
        return "no findings — every rule passes".to_string();
    }
    let errors = findings
        .iter()
        .filter(|f| f.severity == Severity::Error)
        .count();
    let warnings = findings.len() - errors;
    format!(
        "{} finding(s) · {errors} error · {warnings} warning",
        findings.len()
    )
}

/// God-file meta that always names the rule, so the number is never bare.
fn god_file_meta(g: &GodFile) -> String {
    let rule = if g.reasons.is_empty() {
        "flagged".to_string()
    } else {
        g.reasons.join("+")
    };
    format!("{rule} · {} L · fan-out {}", g.loc, g.fan_out)
}

/// Whether the loaded config actually enables any rule (vs. `Rules::default`).
fn rules_configured(rules: &Rules) -> bool {
    !rules.forbid_imports.is_empty()
        || rules.max_cyclomatic.is_some()
        || rules.max_file_loc.is_some()
        || !rules.no_cycles.is_empty()
}

/// The natural entry file of a module: its module-root file, else the first.
fn module_entry_file(files: &[String]) -> Option<&str> {
    files
        .iter()
        .find(|p| basename(p).is_some_and(|b| MODULE_ENTRY_FILES.contains(&b)))
        .or_else(|| files.first())
        .map(String::as_str)
}

/// `true` when `open` is one of the module's descendant files.
fn module_is_open(
    module_files: &BTreeMap<String, Vec<String>>,
    id: &str,
    open: Option<&str>,
) -> bool {
    let Some(path) = open else {
        return false;
    };
    module_files
        .get(id)
        .is_some_and(|files| files.iter().any(|f| f == path))
}

/// Module node id → its descendant file paths, from the `Submodule` tree.
///
/// Each file contributes itself and every ancestor, so a `Crate`/`Dir`/`Module`
/// row can resolve the files beneath it without a second tree walk.
fn module_files_map(idx: &repowalk_core::model::Index) -> BTreeMap<String, Vec<String>> {
    let (nodes, edges) = repowalk_core::modules::build(idx);
    let parent: BTreeMap<&str, &str> = edges
        .iter()
        .filter(|e| e.kind == ModuleEdgeKind::Submodule)
        .map(|e| (e.to.as_str(), e.from.as_str()))
        .collect();

    let mut out: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for n in nodes.iter().filter(|n| n.kind == ModuleKind::File) {
        let mut cur = Some(n.id.as_str());
        while let Some(id) = cur {
            out.entry(id.to_string()).or_default().push(n.id.clone());
            cur = parent.get(id).copied();
        }
    }
    out
}

fn kind_label(kind: ModuleKind) -> &'static str {
    match kind {
        ModuleKind::Crate => "crate",
        ModuleKind::Dir => "dir",
        ModuleKind::Module => "module",
        ModuleKind::File => "file",
    }
}

fn basename(path: &str) -> Option<&str> {
    path.rsplit('/').next().filter(|s| !s.is_empty())
}

/// `crates/gui/src/health/mod.rs` → `health/mod.rs` (last two segments).
fn short_path(path: &str) -> String {
    let mut parts: Vec<&str> = path.rsplit('/').take(2).collect();
    parts.reverse();
    parts.join("/")
}

/// Navigate to a file (and line) in Text mode via the shared selection.
fn goto(app: &mut App, file: String, line: Option<usize>) {
    app.mode = Mode::Text;
    app.open_file_with(file.clone(), line);
    app.selection = Some(Selection::File(file));
}

fn module_widget(m: &ModuleComplexity) -> egui::text::LayoutJob {
    let body = egui::FontId::new(theme::FONT_BODY, egui::FontFamily::Proportional);
    let small = egui::FontId::new(theme::FONT_SMALL, egui::FontFamily::Proportional);
    let mut job = egui::text::LayoutJob::default();
    job.append(&short_path(&m.id), 0.0, span(body, theme::TEXT_PRIMARY));
    job.append("  ", 0.0, span(small.clone(), theme::TEXT_MUTED));
    job.append(kind_label(m.kind), 0.0, span(small, theme::TEXT_SECONDARY));
    job
}

fn finding_widget(f: &Finding) -> egui::text::LayoutJob {
    let small = egui::FontId::new(theme::FONT_SMALL, egui::FontFamily::Proportional);
    let mut job = egui::text::LayoutJob::default();
    job.append(
        severity_tag(f.severity),
        0.0,
        span(small.clone(), severity_color(f.severity)),
    );
    job.append("  ", 0.0, span(small.clone(), theme::TEXT_MUTED));
    job.append(&f.rule, 0.0, span(small.clone(), theme::TEXT_PRIMARY));
    job.append("  ", 0.0, span(small.clone(), theme::TEXT_MUTED));
    job.append(&f.message, 0.0, span(small, theme::TEXT_SECONDARY));
    job
}

fn span(font_id: egui::FontId, color: egui::Color32) -> egui::text::TextFormat {
    egui::text::TextFormat {
        font_id,
        color,
        ..Default::default()
    }
}

fn muted(ui: &mut egui::Ui, text: &str) {
    ui.label(
        egui::RichText::new(text)
            .size(theme::FONT_SMALL)
            .color(theme::TEXT_MUTED),
    );
}

/// Centered empty-state card (a landed view with no data to show).
fn notice(ui: &mut egui::Ui, title: &str, lines: &[&str]) {
    ui.add_space(theme::SPACE_8);
    ui.vertical_centered(|ui| {
        ui.label(
            egui::RichText::new(title)
                .size(theme::FONT_HEADING)
                .color(theme::TEXT_SECONDARY),
        );
        ui.add_space(theme::SPACE_2);
        for line in lines {
            ui.label(
                egui::RichText::new(*line)
                    .size(theme::FONT_SMALL)
                    .color(theme::TEXT_PRIMARY),
            );
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use repowalk_core::model::{FileEntry, Index};

    fn file(path: &str, lines: usize) -> FileEntry {
        FileEntry {
            path: path.into(),
            lang: "rust".into(),
            lines,
            mtime: 0,
            is_entrypoint: false,
        }
    }

    fn finding(rule: &str, severity: Severity, file: &str, line: usize) -> Finding {
        Finding {
            rule: rule.into(),
            severity,
            file: file.into(),
            line,
            message: format!("{rule} breached at {file}"),
        }
    }

    fn god(path: &str, loc: usize, fan_out: usize, reasons: &[&str]) -> GodFile {
        GodFile {
            path: path.into(),
            loc,
            fan_out,
            reasons: reasons.iter().map(|r| r.to_string()).collect(),
        }
    }

    #[test]
    fn severity_maps_to_danger_and_warning_tints() {
        assert_eq!(severity_color(Severity::Error), theme::DANGER);
        assert_eq!(severity_color(Severity::Warning), theme::WARNING);
        assert_eq!(severity_tag(Severity::Error), "error");
        assert_eq!(severity_tag(Severity::Warning), "warn");
    }

    #[test]
    fn clean_vs_findings_render_rule() {
        // Clean run: a positive statement, not an empty pane.
        let clean = findings_headline(&[]);
        assert!(clean.contains("no findings"), "{clean}");

        // Findings: counted by severity, pre-sorted order preserved downstream.
        let rows = vec![
            finding(
                "no_cycles crates/**",
                Severity::Error,
                "crates/core/src/a.rs",
                0,
            ),
            finding(
                "max_cyclomatic",
                Severity::Warning,
                "crates/gui/src/b.rs",
                7,
            ),
            finding("max_file_loc", Severity::Warning, "crates/gui/src/c.rs", 0),
        ];
        let head = findings_headline(&rows);
        assert!(head.contains("3 finding"), "{head}");
        assert!(head.contains("1 error"), "{head}");
        assert!(head.contains("2 warning"), "{head}");

        // A non-empty list renders each finding's rule + severity tag.
        let job = finding_widget(&rows[1]);
        assert!(job.text.contains("max_cyclomatic"), "{}", job.text);
        assert!(job.text.contains("warn"), "{}", job.text);
    }

    #[test]
    fn god_file_meta_never_shows_a_bare_number() {
        let loc_only = god("src/big.rs", 2500, 3, &["loc"]);
        let meta = god_file_meta(&loc_only);
        assert!(meta.contains("loc"), "{meta}");
        assert!(meta.contains("2500 L"), "{meta}");
        assert!(meta.contains("fan-out 3"), "{meta}");

        let both = god("src/kitchen.rs", 900, 44, &["loc", "fan-out"]);
        let meta = god_file_meta(&both);
        assert!(meta.contains("loc+fan-out"), "{meta}");

        // Even an unlabelled row still names the flag rather than a lone number.
        let bare = god_file_meta(&god("src/x.rs", 10, 1, &[]));
        assert!(bare.contains("flagged"), "{bare}");
    }

    #[test]
    fn unavailable_rows_name_the_missing_analysis() {
        assert_eq!(V2_ROWS.len(), 4);
        for (row, why) in V2_ROWS {
            assert!(!row.is_empty());
            assert!(why.contains("v2 analysis"), "{row}: {why}");
        }
    }

    #[test]
    fn module_entry_file_prefers_the_module_root() {
        let files = vec![
            "crates/core/src/a.rs".to_string(),
            "crates/core/src/lib.rs".to_string(),
        ];
        assert_eq!(module_entry_file(&files), Some("crates/core/src/lib.rs"));

        let no_root = vec!["x/a.rs".to_string(), "x/b.rs".to_string()];
        assert_eq!(module_entry_file(&no_root), Some("x/a.rs"));
        assert_eq!(module_entry_file(&[]), None);
        // No `/` at all still resolves the basename.
        assert_eq!(module_entry_file(&["main.rs".to_string()]), Some("main.rs"));
    }

    #[test]
    fn module_files_map_rolls_descendants_up_the_tree() {
        let mut idx = Index::new("/repo");
        idx.files = vec![
            file("crates/core/src/lib.rs", 40),
            file("crates/core/src/a.rs", 10),
        ];

        let map = module_files_map(&idx);
        let core = map.get("core").expect("crate node");
        assert_eq!(
            core,
            &vec![
                "crates/core/src/a.rs".to_string(),
                "crates/core/src/lib.rs".to_string()
            ]
        );
        // The directory and the file node itself both resolve.
        assert_eq!(map.get("crates/core/src").map(Vec::len), Some(2));
        assert_eq!(
            map.get("crates/core/src/lib.rs"),
            Some(&vec!["crates/core/src/lib.rs".to_string()])
        );
        assert!(module_is_open(&map, "core", Some("crates/core/src/a.rs")));
        assert!(!module_is_open(&map, "core", Some("crates/gui/src/app.rs")));
        assert!(!module_is_open(&map, "core", None));
    }

    #[test]
    fn rules_configured_distinguishes_default_from_a_rule() {
        assert!(!rules_configured(&Rules::default()));
        assert!(!rules_configured(&Rules {
            forbid_imports: vec![],
            max_cyclomatic: None,
            max_file_loc: None,
            no_cycles: vec![],
        }));
        assert!(rules_configured(&Rules {
            max_file_loc: Some(5000),
            ..Rules::default()
        }));
        assert!(rules_configured(&Rules {
            forbid_imports: vec![("core".into(), "gui".into())],
            ..Rules::default()
        }));
        assert!(rules_configured(&Rules {
            no_cycles: vec!["crates/**".into()],
            ..Rules::default()
        }));
    }
}
