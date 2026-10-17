//! AppState, layout shell, mode toggle, keyboard, selection sync (plan §4.2-4.3).
//! Redesigned around the `theme` token system: toolbar (segmented control +
//! export menu), sidebar, central pane, status bar. OWNER: RSK-14.
use eframe::egui;
use repowalk_core::analysis::Analysis;
use repowalk_core::export::Diagram;
use repowalk_core::model::Index;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use crate::theme;
use crate::{diagrams, flow, graph, icons, quick_open, sidebar, tabs, text, usage};

/// (edges, symbols, edge_calls, edge_imports) signature guarding graph rebuilds.
pub type GraphState = ((usize, usize, bool, bool), crate::graph::render::WalkGraph);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Text,
    Graph,
    Flow,
    /// v2: CFG / type / module diagrams (F2, F3).
    Diagrams,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Selection {
    Symbol(String),
    File(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortBy {
    FanIn,
    Loc,
    Name,
}

/// What the A→B trace picker is waiting for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TracePick {
    #[default]
    None,
    PickA,
    PickB,
}

/// Background indexing progress shared with the worker thread.
#[derive(Default)]
pub struct IndexProgress {
    pub done: usize,
    pub total: usize,
    pub finished: bool,
    pub error: Option<String>,
    /// Current learn phase (`walk` / `parse` / `resolve` / `analyze` / `cache`).
    pub phase: String,
    /// Non-fatal degradation note, e.g. analysis unavailable on a read-only repo.
    pub warning: Option<String>,
    /// Running extraction counts while learning (zero until the analyze phase).
    pub variables: usize,
    pub cfgs: usize,
    pub types: usize,
    pub uses: usize,
    pub index: Option<Arc<Index>>,
    /// The v2 analysis artifact, produced by the same `learn` pass as the index.
    pub analysis: Option<Arc<Analysis>>,
}

pub struct App {
    pub repo: Option<PathBuf>,
    pub repo_input: String,
    pub index: Option<Arc<Index>>,
    pub mode: Mode,
    pub selection: Option<Selection>,
    pub trace: Option<(String, String)>,
    pub trace_pick: TracePick,
    pub pending_trace_a: Option<String>,
    pub query: String,
    pub sort: SortBy,
    pub edge_calls: bool,
    pub edge_imports: bool,
    pub show_shortcuts: bool,
    pub show_command_palette: bool,
    pub palette_query: String,
    pub show_open_popup: bool,
    pub search_focus: bool,
    pub list_cursor: usize,
    pub open_file: Option<String>,
    pub jump_line: Option<usize>,
    pub dirty_edits: std::collections::HashSet<String>,
    pub editor_buffers: std::collections::HashMap<String, String>,
    pub error: Option<String>,
    pub progress: Arc<Mutex<IndexProgress>>,
    pub pending_screenshot: Option<(u64, PathBuf)>,
    pub status: String,
    /// B5-04: open documents for the VS Code tab strip, and which is active.
    pub tabs: Vec<crate::tabs::TabView>,
    pub active_tab: usize,
    /// B5-04: Ctrl+P quick-open overlay state.
    pub show_quick_open: bool,
    pub quick_open_query: String,
    /// B5-04: Ctrl+B hides the sidebar; the activity bar itself stays.
    pub show_sidebar: bool,
    pub graph_state: Option<GraphState>,
    pub flow: crate::flow::FlowState,
    /// v2 analysis artifact; `None` until the first `learn` completes.
    pub analysis: Option<Arc<Analysis>>,
    pub sidebar_tab: sidebar::Tab,
    pub usage: usage::UsageState,
    pub diagrams: diagrams::DiagramsState,
    /// Dark (default) or light palette, switchable at runtime (DSN-2 / F13).
    pub theme_mode: theme::ThemeMode,
}

/// The egui context handle used by the indexing thread to request repaints.
static APP_CTX: std::sync::OnceLock<egui::Context> = std::sync::OnceLock::new();

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::apply(&cc.egui_ctx);
        let _ = APP_CTX.set(cc.egui_ctx.clone());

        let repo = std::env::args().nth(1).map(PathBuf::from);
        let mut app = App {
            repo: None,
            repo_input: repo
                .as_ref()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default(),
            index: None,
            mode: Mode::Text,
            selection: None,
            trace: None,
            trace_pick: TracePick::None,
            pending_trace_a: None,
            query: String::new(),
            sort: SortBy::FanIn,
            edge_calls: true,
            edge_imports: true,
            show_shortcuts: false,
            show_command_palette: false,
            palette_query: String::new(),
            show_open_popup: false,
            search_focus: false,
            list_cursor: 0,
            open_file: None,
            jump_line: None,
            dirty_edits: Default::default(),
            editor_buffers: Default::default(),
            error: None,
            progress: Arc::new(Mutex::new(IndexProgress::default())),
            pending_screenshot: None,
            status: String::new(),
            tabs: vec![crate::tabs::TabView::default()],
            active_tab: 0,
            show_quick_open: false,
            quick_open_query: String::new(),
            show_sidebar: true,
            graph_state: None,
            flow: Default::default(),
            analysis: None,
            sidebar_tab: sidebar::Tab::default(),
            usage: Default::default(),
            diagrams: Default::default(),
            theme_mode: theme::ThemeMode::Dark,
        };
        if let Some(repo) = repo {
            app.start_indexing(repo);
        }
        app
    }

    /// Kick off a background indexing run (plan §3.1: progress overlay).
    pub fn start_indexing(&mut self, repo: PathBuf) {
        let progress = Arc::new(Mutex::new(IndexProgress::default()));
        let shared = Arc::clone(&progress);
        let ctx = APP_CTX.get().cloned();
        self.repo = Some(repo.clone());
        self.error = None;
        self.selection = None;
        self.trace = None;
        self.trace_pick = TracePick::None;
        self.editor_buffers.clear();
        self.dirty_edits.clear();
        self.open_file = None;
        self.tabs = vec![crate::tabs::TabView::default()];
        self.active_tab = 0;
        self.show_quick_open = false;
        self.quick_open_query.clear();
        self.graph_state = None;
        self.flow = Default::default();
        self.analysis = None;
        self.status = "learning…".to_string();
        std::thread::spawn(move || {
            // F1: `learn` walks, parses, resolves and analyzes in one pass, then
            // caches both artifacts. The phases are surfaced so "open a codebase"
            // also says what the app is doing, and how far it has got.
            let notify = |p: repowalk_core::learn::LearnProgress| {
                if let Ok(mut s) = shared.lock() {
                    s.phase = p.phase.to_string();
                    s.done = p.done;
                    s.total = p.total;
                    s.variables = p.variables;
                    s.cfgs = p.cfgs;
                    s.types = p.types;
                    s.uses = p.uses;
                }
                if let Some(ctx) = &ctx {
                    ctx.request_repaint();
                }
            };
            let mut warning = None;
            let result = match repowalk_core::learn::learn(&repo, &notify) {
                Ok(pair) => Ok(pair),
                Err(e) => {
                    // `learn` persists its caches, so a read-only repo (or a full
                    // disk) would otherwise fail to open at all. Fall back to the
                    // index-only path and say what degraded instead of refusing.
                    match repowalk_core::pipeline::build_index(&repo, &|_: usize| {}) {
                        Ok(idx) => {
                            warning = Some(format!(
                                "analysis unavailable ({e:#}) — showing the index only"
                            ));
                            Ok((idx, Analysis::new()))
                        }
                        Err(_) => Err(e),
                    }
                }
            };
            let mut p = shared.lock().unwrap();
            p.finished = true;
            p.warning = warning;
            match result {
                Ok((idx, analysis)) => {
                    p.total = idx.files.len();
                    p.index = Some(Arc::new(idx));
                    p.analysis = Some(Arc::new(analysis));
                }
                Err(e) => p.error = Some(format!("{e:#}")),
            }
            if let Some(ctx) = &ctx {
                ctx.request_repaint();
            }
        });
        self.progress = progress;
    }

    /// Poll the background worker; adopt the index when it lands.
    pub fn poll_progress(&mut self) {
        let mut p = self.progress.lock().unwrap();
        if p.finished {
            let mut learned: Option<usize> = None;
            if let Some(idx) = p.index.take() {
                learned = Some(idx.files.len());
                self.index = Some(idx);
            }
            if let Some(a) = p.analysis.take() {
                self.status = match learned {
                    Some(files) => format!(
                        "learned {files} files · {} variables · {} cfgs · {} types",
                        a.variables.len(),
                        a.cfgs.len(),
                        a.types.len()
                    ),
                    None => format!("analysis ready: {} variables", a.variables.len()),
                };
                self.analysis = Some(a);
            } else if let Some(files) = learned {
                self.status = format!("learned {files} files");
            }
            if let Some(w) = p.warning.take() {
                self.status = w;
            }
            if let Some(e) = p.error.take() {
                self.error = Some(e);
                self.status.clear();
            }
            p.finished = false;
        }
    }

    /// One selection drives every pane (plan §4.2). File selections also open
    /// or activate a document tab (the VS Code-shell behaviour).
    pub fn select(&mut self, sel: Selection) {
        match &sel {
            Selection::File(path) => {
                self.open_in_tabs(path.clone());
                self.open_file_with(path.clone(), None);
            }
            Selection::Symbol(id) => {
                if let Some(idx) = &self.index {
                    if let Some(sym) = idx.symbols.iter().find(|s| s.id == *id) {
                        let file = sym.file.clone();
                        let line = sym.start_line;
                        self.open_in_tabs(file.clone());
                        self.open_file_with(file, Some(line));
                    }
                }
            }
        }
        self.selection = Some(sel);
    }

    /// Change the reading mode of the active document. Every mode switch (the
    /// toolbar segments, the Tab/g keys, the palette) goes through here so the
    /// tab strip and the mode stay one truth.
    pub fn set_mode(&mut self, mode: Mode) {
        self.mode = mode;
        if let Some(tab) = self.tabs.get_mut(self.active_tab) {
            tab.mode = mode;
            if mode == Mode::Diagrams {
                tab.diagrams = self.diagrams.tab;
            }
        }
        self.jump_line = None;
    }

    pub fn open_file_with(&mut self, path: String, line: Option<usize>) {
        if self.open_file.as_deref() != Some(path.as_str())
            && !self.editor_buffers.contains_key(&path)
        {
            if let Some(repo) = &self.repo {
                match std::fs::read_to_string(repo.join(&path)) {
                    Ok(text) => {
                        self.editor_buffers.insert(path.clone(), text);
                    }
                    Err(e) => {
                        self.error = Some(format!("cannot open {path}: {e}"));
                        return;
                    }
                }
            }
        }
        self.open_file = Some(path);
        self.jump_line = line;
    }

    /// Ctrl+S: write the edited buffer back to disk.
    pub fn save_open_file(&mut self) {
        let Some(path) = self.open_file.clone() else {
            return;
        };
        let Some(repo) = &self.repo else {
            return;
        };
        let Some(text) = self.editor_buffers.get(&path).cloned() else {
            return;
        };
        match std::fs::write(repo.join(&path), text) {
            Ok(()) => {
                self.dirty_edits.remove(&path);
                self.status = format!("saved {path}");
            }
            Err(e) => self.error = Some(format!("cannot save {path}: {e}")),
        }
    }

    pub fn clear_transient(&mut self) {
        self.selection = None;
        self.trace = None;
        self.trace_pick = TracePick::None;
        self.jump_line = None;
    }

    pub fn module_of(path: &str) -> String {
        path.rsplit_once('/')
            .map(|(d, _)| d.to_string())
            .unwrap_or_else(|| "(root)".to_string())
    }

    fn handle_keyboard(&mut self, ctx: &egui::Context) {
        let (
            tab,
            slash,
            esc,
            question,
            g,
            enter,
            j,
            k,
            ctrl_s,
            ctrl_k,
            ctrl_t,
            ctrl_p,
            ctrl_b,
            ctrl_w,
            ctrl_tab,
        ) = ctx.input(|i| {
            (
                i.key_pressed(egui::Key::Tab),
                i.key_pressed(egui::Key::Slash) && i.modifiers.is_none(),
                i.key_pressed(egui::Key::Escape),
                i.key_pressed(egui::Key::Questionmark) && i.modifiers.is_none(),
                i.key_pressed(egui::Key::G) && i.modifiers.is_none(),
                i.key_pressed(egui::Key::Enter),
                i.key_pressed(egui::Key::J) && i.modifiers.is_none(),
                i.key_pressed(egui::Key::K) && i.modifiers.is_none(),
                i.key_pressed(egui::Key::S) && i.modifiers.ctrl && !i.modifiers.shift,
                i.key_pressed(egui::Key::K) && i.modifiers.ctrl,
                i.key_pressed(egui::Key::T) && i.modifiers.ctrl,
                i.key_pressed(egui::Key::P) && i.modifiers.ctrl && !i.modifiers.shift,
                i.key_pressed(egui::Key::B) && i.modifiers.ctrl && !i.modifiers.shift,
                i.key_pressed(egui::Key::W) && i.modifiers.ctrl && !i.modifiers.shift,
                i.key_pressed(egui::Key::Tab) && i.modifiers.ctrl,
            )
        });
        if self.show_command_palette {
            // While the palette is open, only Esc / Ctrl+K apply globally.
            if esc || ctrl_k {
                self.show_command_palette = false;
                self.palette_query.clear();
            }
            return;
        }
        if ctrl_k {
            self.show_command_palette = true;
            self.palette_query.clear();
            return;
        }
        if ctrl_t {
            self.theme_mode = match self.theme_mode {
                theme::ThemeMode::Dark => theme::ThemeMode::Light,
                theme::ThemeMode::Light => theme::ThemeMode::Dark,
            };
            theme::apply_theme(ctx, self.theme_mode);
        }
        if self.show_quick_open {
            // The overlay owns the keyboard; Esc/Ctrl+P close (Enter is read
            // inside the overlay itself, where the matches live).
            if esc || ctrl_p {
                self.show_quick_open = false;
                self.quick_open_query.clear();
            }
            return;
        }
        if ctrl_p {
            self.show_quick_open = true;
            self.quick_open_query.clear();
            return;
        }
        if ctrl_b {
            self.show_sidebar = !self.show_sidebar;
        }
        if ctrl_w {
            self.close_tab(self.active_tab);
        }
        if ctrl_tab && self.tabs.len() > 1 {
            let next = (self.active_tab + 1) % self.tabs.len();
            self.activate_tab(next);
        }
        if tab {
            self.set_mode(match self.mode {
                Mode::Text => Mode::Graph,
                Mode::Graph => Mode::Flow,
                Mode::Flow => Mode::Diagrams,
                Mode::Diagrams => Mode::Text,
            });
        }
        if slash {
            self.search_focus = true;
        }
        if esc {
            self.clear_transient();
            self.show_shortcuts = false;
            self.show_open_popup = false;
        }
        if question {
            self.show_shortcuts = !self.show_shortcuts;
        }
        if g {
            self.set_mode(Mode::Graph);
        }
        if ctrl_s {
            self.save_open_file();
        }
        if j {
            self.list_cursor = self.list_cursor.saturating_add(1);
        }
        if k {
            self.list_cursor = self.list_cursor.saturating_sub(1);
        }
        if enter {
            let cursor = self.list_cursor;
            if self.mode == Mode::Text {
                let items = self.sidebar_list_entries();
                if let Some(sel) = items.get(cursor) {
                    self.select(sel.clone());
                }
            }
        }
    }

    /// Entries the j/k/Enter cursor walks, in sidebar display order.
    fn sidebar_list_entries(&self) -> Vec<Selection> {
        let Some(idx) = &self.index else {
            return Vec::new();
        };
        let mut entries: Vec<Selection> = repowalk_core::tour::tour(idx, 20)
            .into_iter()
            .map(|t| Selection::File(t.path))
            .collect();
        if !self.query.trim().is_empty() {
            for h in repowalk_core::search::search(idx, &self.query, 10) {
                entries.push(Selection::Symbol(h.target));
            }
        } else {
            entries.extend(
                idx.symbols
                    .iter()
                    .take(30)
                    .map(|s| Selection::Symbol(s.id.clone())),
            );
        }
        entries
    }

    /// Drain `Event::Screenshot` for a pending export and write the PNG (RSK-23).
    fn consume_screenshot_event(&mut self, ctx: &egui::Context) {
        let Some((tag, path)) = &self.pending_screenshot else {
            return;
        };
        let tag = *tag;
        let path = path.clone();
        let found = ctx.input(|i| {
            i.events.iter().find_map(|e| match e {
                egui::Event::Screenshot {
                    image, user_data, ..
                } => {
                    let tag_ok = user_data
                        .data
                        .as_ref()
                        .and_then(|d| d.downcast_ref::<u64>())
                        .is_some_and(|t| *t == tag);
                    tag_ok.then(|| Arc::clone(image))
                }
                _ => None,
            })
        });
        if let Some(image) = found {
            self.pending_screenshot = None;
            match crate::export::png::save_png(&image, &path) {
                Ok(()) => self.status = format!("saved {}", path.display()),
                Err(e) => self.error = Some(format!("png export failed: {e}")),
            }
        }
    }

    /// PNG export entry (RSK-23): request a viewport screenshot tagged for us.
    pub fn export_png(&mut self, ctx: &egui::Context) {
        let Some(repo) = self.repo.clone() else {
            self.error = Some("open a repository first".to_string());
            return;
        };
        static TAG: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        let tag = TAG.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.pending_screenshot = Some((tag, repo.join("repowalk-graph.png")));
        crate::export::png::request_screenshot(ctx, tag);
    }

    /// Markdown export entry (RSK-22): tour + health next to the repo.
    pub fn export_markdown(&mut self) {
        let Some(idx) = self.index.clone() else {
            self.error = Some("open a repository first".to_string());
            return;
        };
        let Some(repo) = self.repo.clone() else {
            return;
        };
        let path = repo.join("repowalk-report.md");
        let mut md = crate::export::markdown::tour_markdown(&idx);
        md.push('\n');
        md.push_str(&crate::export::markdown::health_markdown(&idx));
        match std::fs::write(&path, md) {
            Ok(()) => self.status = format!("saved {}", path.display()),
            Err(e) => self.error = Some(format!("markdown export failed: {e}")),
        }
    }

    /// The index + analysis the v2 emitters need, or a status message saying
    /// what is missing. Both are produced by one `learn` pass, so they appear
    /// together — but keep the guard honest in case a repo opened index-only.
    fn export_inputs(&mut self) -> Option<(Arc<Index>, Arc<Analysis>, std::path::PathBuf)> {
        let (Some(idx), Some(analysis)) = (self.index.clone(), self.analysis.clone()) else {
            self.error = Some(if self.index.is_none() {
                "open a repository first".to_string()
            } else {
                "no analysis yet — press Learn".to_string()
            });
            return None;
        };
        let Some(repo) = self.repo.clone() else {
            self.error = Some("open a repository first".to_string());
            return None;
        };
        Some((idx, analysis, repo))
    }

    /// v2 architecture report (F8): the exact Markdown the CLI `report` prints,
    /// written next to the repo. Same core emitter, so the two cannot drift.
    pub fn export_architecture_report(&mut self) {
        let Some((idx, analysis, repo)) = self.export_inputs() else {
            return;
        };
        let md = repowalk_core::export::markdown_report(&idx, &analysis);
        let path = repo.join("repowalk-architecture.md");
        match std::fs::write(&path, md) {
            Ok(()) => self.status = format!("saved {}", path.display()),
            Err(e) => self.error = Some(format!("report export failed: {e}")),
        }
    }

    /// One v2 diagram (F8) as Mermaid / DOT / SVG — the same pure emitters the
    /// CLI `graph` uses. `ext` is the file extension, `format` the emitter key.
    pub fn export_diagram(&mut self, diagram: Diagram, format: &str, ext: &str) {
        let Some((idx, analysis, repo)) = self.export_inputs() else {
            return;
        };
        let text = match format {
            "mermaid" => repowalk_core::export::mermaid(&idx, &analysis, &diagram),
            "dot" => repowalk_core::export::dot(&idx, &analysis, &diagram),
            _ => repowalk_core::export::svg(&idx, &analysis, &diagram),
        };
        let name = match &diagram {
            Diagram::CallGraph => "callgraph",
            Diagram::Types => "types",
            Diagram::Cfg { .. } => "cfg",
            Diagram::Modules => "modules",
            Diagram::Directory { .. } => "directory",
        };
        let path = repo.join(format!("repowalk-{name}.{ext}"));
        match std::fs::write(&path, text) {
            Ok(()) => self.status = format!("saved {}", path.display()),
            Err(e) => self.error = Some(format!("diagram export failed: {e}")),
        }
    }

    // ── Shell chrome ────────────────────────────────────────────────────────

    /// Top toolbar: segmented mode control, open-repo button, export menu,
    /// metrics strip. Recognition-over-recall: shortcuts shown in tooltips.
    fn toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = theme::SPACE_3;

            // Segmented mode control (Fitts: adjacent large targets).
            let text_active = self.mode == Mode::Text;
            let graph_active = self.mode == Mode::Graph;
            let flow_active = self.mode == Mode::Flow;
            let diagrams_active = self.mode == Mode::Diagrams;
            if mode_segment(ui, "Text", "Tab", text_active).clicked() {
                self.set_mode(Mode::Text);
            }
            if mode_segment(ui, "Graph", "Tab", graph_active).clicked() {
                self.set_mode(Mode::Graph);
            }
            if mode_segment(ui, "Flow", "Tab", flow_active).clicked() {
                self.set_mode(Mode::Flow);
            }
            if mode_segment(ui, "Diagrams", "Tab", diagrams_active).clicked() {
                self.set_mode(Mode::Diagrams);
            }

            ui.separator();

            // Icon buttons, VS Code style: an icon + a tooltip that names the
            // action and its shortcut (recognition over recall).
            if ui
                .add(egui::Button::new(
                    crate::icons::glyph(crate::icons::OPEN, 15.0).color(theme::TEXT_SECONDARY),
                ))
                .on_hover_text("Open a repository folder…")
                .clicked()
            {
                self.show_open_popup = true;
            }
            if ui
                .add(egui::Button::new(
                    crate::icons::glyph(crate::icons::SEARCH, 15.0).color(theme::TEXT_SECONDARY),
                ))
                .on_hover_text("Quick open a file (Ctrl+P)")
                .clicked()
            {
                self.show_quick_open = true;
                self.quick_open_query.clear();
            }
            if ui
                .add(egui::Button::new(
                    crate::icons::glyph(crate::icons::EXPORT_ICON, 15.0)
                        .color(theme::TEXT_SECONDARY),
                ))
                .on_hover_text("Command palette (Ctrl+K)")
                .clicked()
            {
                self.show_command_palette = true;
                self.palette_query.clear();
            }

            ui.separator();

            // Menus get icon labels with tooltips (one font per text widget,
            // so the chevron lives in the tooltip, not the glyph run).
            let export = ui.menu_button(
                crate::icons::glyph(crate::icons::EXPORT_ICON, 15.0).color(theme::TEXT_SECONDARY),
                |ui| {
                    if ui.button("Window → PNG").clicked() {
                        self.export_png(ui.ctx());
                        ui.close();
                    }
                    if ui.button("Tour + health → Markdown").clicked() {
                        self.export_markdown();
                        ui.close();
                    }
                    if ui.button("Architecture report → Markdown").clicked() {
                        self.export_architecture_report();
                        ui.close();
                    }
                    ui.separator();
                    // v2 diagrams: the same core emitters `repowalk graph` uses, so
                    // the file the GUI writes and the file the CLI prints are equal.
                    ui.menu_button("Diagrams → Mermaid (.mmd)", |ui| {
                        if ui.button("Call graph").clicked() {
                            self.export_diagram(Diagram::CallGraph, "mermaid", "mmd");
                            ui.close();
                        }
                        if ui.button("Types").clicked() {
                            self.export_diagram(Diagram::Types, "mermaid", "mmd");
                            ui.close();
                        }
                        if ui.button("Modules").clicked() {
                            self.export_diagram(Diagram::Modules, "mermaid", "mmd");
                            ui.close();
                        }
                        if ui.button("Directory tree").clicked() {
                            self.export_diagram(
                                Diagram::Directory { imports: false },
                                "mermaid",
                                "mmd",
                            );
                            ui.close();
                        }
                    });
                    ui.menu_button("Diagrams → Graphviz DOT (.dot)", |ui| {
                        if ui.button("Call graph").clicked() {
                            self.export_diagram(Diagram::CallGraph, "dot", "dot");
                            ui.close();
                        }
                        if ui.button("Types").clicked() {
                            self.export_diagram(Diagram::Types, "dot", "dot");
                            ui.close();
                        }
                        if ui.button("Modules").clicked() {
                            self.export_diagram(Diagram::Modules, "dot", "dot");
                            ui.close();
                        }
                        if ui.button("Directory tree").clicked() {
                            self.export_diagram(
                                Diagram::Directory { imports: false },
                                "dot",
                                "dot",
                            );
                            ui.close();
                        }
                    });
                    ui.menu_button("Diagrams → SVG (.svg)", |ui| {
                        if ui.button("Call graph").clicked() {
                            self.export_diagram(Diagram::CallGraph, "svg", "svg");
                            ui.close();
                        }
                        if ui.button("Types").clicked() {
                            self.export_diagram(Diagram::Types, "svg", "svg");
                            ui.close();
                        }
                        if ui.button("Modules").clicked() {
                            self.export_diagram(Diagram::Modules, "svg", "svg");
                            ui.close();
                        }
                        if ui.button("Directory tree").clicked() {
                            self.export_diagram(
                                Diagram::Directory { imports: false },
                                "svg",
                                "svg",
                            );
                            ui.close();
                        }
                    });
                },
            );
            export.response
                .on_hover_text("Export — PNG, Markdown reports, diagrams");

            // F13: the palette is switchable at runtime; the choice applies
            // immediately and the dark values stay the source of truth.
            let view = ui.menu_button(
                crate::icons::glyph(crate::icons::GEAR, 15.0).color(theme::TEXT_SECONDARY),
                |ui| {
                    let dark = self.theme_mode == theme::ThemeMode::Dark;
                    if ui.selectable_label(dark, "Dark theme").clicked() {
                        self.theme_mode = theme::ThemeMode::Dark;
                        theme::apply_theme(ui.ctx(), self.theme_mode);
                        ui.close();
                    }
                    if ui.selectable_label(!dark, "Light theme").clicked() {
                        self.theme_mode = theme::ThemeMode::Light;
                        theme::apply_theme(ui.ctx(), self.theme_mode);
                        ui.close();
                    }
                },
            );
            view.response.on_hover_text("View (theme, Ctrl+T)");

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.weak(repowalk_core::VERSION);
                if let Some(idx) = &self.index {
                    let m = repowalk_core::metrics::compute(idx);
                    ui.weak(format!(
                        "{} files · {} LOC · {} cycles",
                        idx.files.len(),
                        m.total_loc,
                        m.cycles.len()
                    ));
                }
            });
        });
    }

    /// Bottom status bar: transient status / errors, always-visible feedback.
    fn status_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = theme::SPACE_3;
            if let Some(err) = &self.error {
                ui.colored_label(theme::DANGER, format!("✕ {err}"));
                if ui.small_button("dismiss").clicked() {
                    self.error = None;
                }
            } else if !self.status.is_empty() {
                ui.colored_label(theme::TEXT_SECONDARY, &self.status);
            } else {
                ui.colored_label(theme::TEXT_MUTED, "ready");
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .colored_label(theme::TEXT_MUTED, "? shortcuts")
                    .on_hover_text("Toggle the keyboard-shortcut overlay")
                    .clicked()
                {
                    self.show_shortcuts = !self.show_shortcuts;
                }
            });
        });
    }

    /// Modal-ish open-repo popup (progressive disclosure: path + Learn button).
    fn open_popup(&mut self, ctx: &egui::Context) {
        if !self.show_open_popup {
            return;
        }
        let mut open = true;
        egui::Window::new("Open repository")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label(egui::RichText::new("Path to a codebase:").color(theme::TEXT_SECONDARY));
                ui.add_space(theme::SPACE_2);
                let resp = ui.text_edit_singleline(&mut self.repo_input);
                let go = (resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)))
                    || ui.button("Learn").clicked();
                if go {
                    let path = PathBuf::from(self.repo_input.trim());
                    if !path.as_os_str().is_empty() {
                        self.start_indexing(path);
                        self.show_open_popup = false;
                    }
                }
            });
        self.show_open_popup = open;
    }

    /// Ctrl+K command palette (Hick's-law overflow valve; recognition over recall).
    fn command_palette(&mut self, ctx: &egui::Context) {
        if !self.show_command_palette {
            return;
        }
        let mut open = true;
        let mut run: Option<PaletteAction> = None;
        egui::Window::new("Command palette")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(380.0)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, -60.0])
            .show(ctx, |ui| {
                egui::TextEdit::singleline(&mut self.palette_query)
                    .hint_text("type a command…")
                    .show(ui);
                ui.add_space(theme::SPACE_2);
                let q = self.palette_query.to_lowercase();
                let actions = palette_actions(self);
                let mut shown = 0usize;
                for action in actions {
                    if !q.is_empty()
                        && !action.label.to_lowercase().contains(&q)
                        && !action.hint.to_lowercase().contains(&q)
                    {
                        continue;
                    }
                    shown += 1;
                    let selected = shown == 1;
                    let text = if action.hint.is_empty() {
                        action.label.to_string()
                    } else {
                        format!("{}    ({})", action.label, action.hint)
                    };
                    if ui.selectable_label(selected, text).clicked() {
                        run = Some(action);
                    }
                }
                if shown == 0 {
                    ui.weak("no matching commands");
                }
                ui.add_space(theme::SPACE_1);
                ui.colored_label(theme::TEXT_MUTED, "↑↓ navigate · ↵ run · esc close");
                if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    run = palette_actions(self).into_iter().find(|a| {
                        let qq = self.palette_query.to_lowercase();
                        qq.is_empty()
                            || a.label.to_lowercase().contains(&qq)
                            || a.hint.to_lowercase().contains(&qq)
                    });
                }
            });
        self.show_command_palette = open;
        if let Some(action) = run {
            self.show_command_palette = false;
            self.palette_query.clear();
            match action.kind {
                PaletteKind::ModeText => self.set_mode(Mode::Text),
                PaletteKind::ModeGraph => self.set_mode(Mode::Graph),
                PaletteKind::ModeFlow => self.set_mode(Mode::Flow),
                PaletteKind::ModeDiagrams => self.set_mode(Mode::Diagrams),
                PaletteKind::PanelExplorer => {
                    self.sidebar_tab = sidebar::Tab::Explorer;
                    self.show_sidebar = true;
                }
                PaletteKind::PanelSearch => {
                    self.sidebar_tab = sidebar::Tab::Search;
                    self.show_sidebar = true;
                    self.search_focus = true;
                }
                PaletteKind::QuickOpen => {
                    self.show_quick_open = true;
                    self.quick_open_query.clear();
                }
                PaletteKind::PanelUsage => self.sidebar_tab = sidebar::Tab::Usage,
                PaletteKind::PanelHealth => self.sidebar_tab = sidebar::Tab::Health,
                PaletteKind::PanelNotes => self.sidebar_tab = sidebar::Tab::Notes,
                PaletteKind::PanelOnboard => self.sidebar_tab = sidebar::Tab::Onboard,
                PaletteKind::FocusSearch => self.search_focus = true,
                PaletteKind::OpenRepo => self.show_open_popup = true,
                PaletteKind::ExportPng => self.export_png(ctx),
                PaletteKind::ExportMd => self.export_markdown(),
                PaletteKind::ExportArchitecture => self.export_architecture_report(),
                PaletteKind::ExportMermaid => {
                    self.export_diagram(Diagram::CallGraph, "mermaid", "mmd")
                }
                PaletteKind::ExportDot => self.export_diagram(Diagram::CallGraph, "dot", "dot"),
                PaletteKind::ExportSvg => self.export_diagram(Diagram::CallGraph, "svg", "svg"),
                PaletteKind::ToggleTheme => {
                    self.theme_mode = match self.theme_mode {
                        theme::ThemeMode::Dark => theme::ThemeMode::Light,
                        theme::ThemeMode::Light => theme::ThemeMode::Dark,
                    };
                    theme::apply_theme(ctx, self.theme_mode);
                }
                PaletteKind::Shortcuts => self.show_shortcuts = !self.show_shortcuts,
                PaletteKind::ClearSelection => self.clear_transient(),
            }
        }
    }

    /// Keyboard-shortcut overlay (`?`) — laid out as a two-column grid.
    fn shortcuts_window(&mut self, ctx: &egui::Context) {
        if !self.show_shortcuts {
            return;
        }
        let mut open = true;
        egui::Window::new("Keyboard shortcuts")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                egui::Grid::new("shortcut-grid")
                    .num_columns(2)
                    .spacing([theme::SPACE_6, theme::SPACE_1])
                    .show(ui, |ui| {
                        for (keys, desc) in [
                            ("Tab", "Cycle Text / Graph / Flow / Diagrams"),
                            ("Ctrl+K", "Command palette"),
                            ("/", "Focus search"),
                            ("j / k", "Move through sidebar lists"),
                            ("Enter", "Jump to / select"),
                            ("g", "Focus graph"),
                            ("Esc", "Clear selection / trace / dialogs"),
                            ("Ctrl+S", "Save the edited file"),
                            ("Ctrl+T", "Toggle light / dark theme"),
                            ("?", "Show this overlay"),
                        ] {
                            ui.monospace(egui::RichText::new(keys).color(theme::ACCENT));
                            ui.label(egui::RichText::new(desc).color(theme::TEXT_SECONDARY));
                            ui.end_row();
                        }
                    });
            });
        self.show_shortcuts = open;
    }

    /// Centered progress overlay while indexing (feedback principle).
    fn progress_overlay(&mut self, ui: &egui::Ui) {
        if self.index.is_some() || self.error.is_some() {
            return;
        }
        let (done, total, phase, variables, cfgs) = {
            let p = self.progress.lock().unwrap();
            (p.done, p.total, p.phase.clone(), p.variables, p.cfgs)
        };
        egui::Area::new(egui::Id::new("progress-overlay"))
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_min_width(220.0);
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(if phase.is_empty() {
                            "learning…".to_string()
                        } else if total > 0 {
                            format!("{phase} · {done}/{total} files…")
                        } else {
                            format!("{phase}…")
                        });
                    });
                    // Live extraction counts (DSN-4 flagged these as missing).
                    if variables > 0 || cfgs > 0 {
                        ui.add_space(theme::SPACE_1);
                        ui.label(
                            egui::RichText::new(format!(
                                "{variables} variables · {cfgs} control-flow graphs so far"
                            ))
                            .size(theme::FONT_SMALL)
                            .color(theme::TEXT_SECONDARY),
                        );
                    }
                });
            });
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.poll_progress();
        self.handle_keyboard(&ctx);
        self.consume_screenshot_event(&ctx);

        // Top toolbar strip (the VS Code title/menu row).
        egui::Frame::new()
            .fill(theme::SURFACE_E0)
            .inner_margin(egui::Margin::symmetric(
                theme::SPACE_3 as i8,
                theme::SPACE_2 as i8,
            ))
            .show(ui, |ui| {
                ui.set_min_height(theme::TOOLBAR_HEIGHT - theme::SPACE_4);
                self.toolbar(ui);
            });
        ui.separator();

        // The activity bar: the icon strip that selects the side panel.
        egui::Panel::left("activity_bar")
            .resizable(false)
            .exact_size(theme::ACTIVITY_BAR_WIDTH)
            .frame(
                egui::Frame::new()
                    .fill(theme::SURFACE_E1)
                    .inner_margin(egui::Margin::symmetric(0, theme::SPACE_2 as i8)),
            )
            .show(ui, |ui| {
                activity_bar(self, ui);
            });

        // The side panel itself (Ctrl+B hides it).
        if self.show_sidebar {
            egui::Panel::left("sidebar")
                .resizable(true)
                .default_size(theme::SIDEBAR_DEFAULT)
                .min_size(theme::SIDEBAR_MIN)
                .max_size(theme::SIDEBAR_MAX)
                .show(ui, |ui| {
                    ui.add_space(theme::SPACE_1);
                    sidebar::ui(ui, self);
                });
        }

        // Central pane: tab strip → breadcrumbs → the active document view.
        let mode = self.mode;
        egui::CentralPanel::default().show(ui, |ui| {
            // While reading diagrams, the active tab keeps the sub-tab truthful.
            if mode == Mode::Diagrams {
                let diagrams_tab = self.diagrams.tab;
                if let Some(tab) = self.tabs.get_mut(self.active_tab) {
                    tab.diagrams = diagrams_tab;
                }
            }
            tabs::tab_strip(ui, self);
            ui.allocate_ui_with_layout(
                egui::vec2(ui.available_width(), theme::BREADCRUMB_HEIGHT),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    ui.add_space(theme::SPACE_3);
                    tabs::breadcrumbs(ui, self);
                },
            );
            theme::separator(ui);
            ui.add_space(theme::SPACE_1);
            match mode {
                Mode::Text => text::ui(ui, self),
                Mode::Graph => graph::ui(ui, self),
                Mode::Flow => flow::ui(&ctx, ui, self),
                Mode::Diagrams => diagrams::ui(ui, self),
            }
        });

        // Bottom status bar.
        ui.separator();
        egui::Frame::new()
            .fill(theme::SURFACE_E0)
            .inner_margin(egui::Margin::symmetric(
                theme::SPACE_3 as i8,
                theme::SPACE_1 as i8,
            ))
            .show(ui, |ui| {
                ui.set_min_height(theme::STATUS_BAR_HEIGHT - theme::SPACE_2);
                self.status_bar(ui);
            });

        // Overlays.
        self.open_popup(&ctx);
        self.command_palette(&ctx);
        self.shortcuts_window(&ctx);
        quick_open::ui(ui, self);
        self.progress_overlay(ui);
    }
}

// ── Toolbar helpers ─────────────────────────────────────────────────────────

/// The activity bar (B5-04): one monogram button per side panel, stacked
/// vertically; the active one gets the accent border on its left edge, the way
/// VS Code highlights the icon column.
fn activity_bar(app: &mut App, ui: &mut egui::Ui) {
    let items = [
        (sidebar::Tab::Explorer, icons::FILES, "Explorer"),
        (sidebar::Tab::Search, icons::SEARCH, "Search"),
        (sidebar::Tab::Map, icons::MAP, "Map"),
        (sidebar::Tab::Usage, icons::USAGE, "Usage"),
        (sidebar::Tab::Health, icons::HEALTH, "Health"),
        (sidebar::Tab::Notes, icons::NOTES, "Notes"),
        (sidebar::Tab::Onboard, icons::OWNER_GUIDE, "Repo 101"),
    ];
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = theme::SPACE_2;
        for (tab, icon, name) in items {
            let active = app.sidebar_tab == tab;
            let size = egui::vec2(
                theme::ACTIVITY_BAR_WIDTH - 2.0 * theme::SPACE_2,
                theme::SPACE_6,
            );
            let (_, resp) = ui.allocate_exact_size(size, egui::Sense::click());
            let resp = resp.on_hover_text(format!("{name} panel"));
            if active {
                let edge = egui::Rect::from_min_max(
                    resp.rect.min,
                    egui::pos2(resp.rect.min.x + 2.0, resp.rect.max.y),
                );
                ui.painter()
                    .rect_filled(edge, egui::CornerRadius::ZERO, theme::ACCENT);
            }
            ui.painter().text(
                resp.rect.center(),
                egui::Align2::CENTER_CENTER,
                icon.to_string(),
                egui::FontId::new(
                    theme::ACTIVITY_ICON,
                    egui::FontFamily::Name(icons::FONT_NAME.into()),
                ),
                if active {
                    theme::TEXT_PRIMARY
                } else {
                    theme::TEXT_SECONDARY
                },
            );
            if resp.clicked() {
                app.sidebar_tab = tab;
            }
        }
    });
    ui.with_layout(egui::Layout::bottom_up(egui::Align::Center), |ui| {
        if ui
            .button(icons::glyph(icons::GEAR, 16.0).color(theme::TEXT_MUTED))
            .on_hover_text("Keyboard shortcuts")
            .clicked()
        {
            app.show_shortcuts = !app.show_shortcuts;
        }
    });
}

/// Segmented-control segment for Text/Graph mode (active = accent top-border).
fn mode_segment(ui: &mut egui::Ui, label: &str, shortcut: &str, active: bool) -> egui::Response {
    let text = if active {
        egui::RichText::new(label)
            .color(theme::TEXT_PRIMARY)
            .strong()
    } else {
        egui::RichText::new(label).color(theme::TEXT_SECONDARY)
    };
    let resp = ui.add(egui::Button::new(text).frame(false));
    let resp = resp.on_hover_text(format!("{label} mode ({shortcut})"));
    if active {
        let rect = resp.rect;
        ui.painter().rect_filled(
            egui::Rect::from_min_max(egui::pos2(rect.min.x, rect.max.y - 2.0), rect.max),
            egui::CornerRadius::ZERO,
            theme::ACCENT,
        );
    }
    resp
}

// ── Command palette model ───────────────────────────────────────────────────

enum PaletteKind {
    ModeText,
    ModeGraph,
    ModeFlow,
    ModeDiagrams,
    PanelExplorer,
    PanelSearch,
    QuickOpen,
    PanelUsage,
    PanelHealth,
    PanelNotes,
    PanelOnboard,
    FocusSearch,
    OpenRepo,
    ExportPng,
    ExportMd,
    ExportArchitecture,
    ExportMermaid,
    ExportDot,
    ExportSvg,
    ToggleTheme,
    Shortcuts,
    ClearSelection,
}

struct PaletteAction {
    label: &'static str,
    hint: &'static str,
    kind: PaletteKind,
}

fn palette_actions(_app: &App) -> Vec<PaletteAction> {
    vec![
        PaletteAction {
            label: "Switch to Text mode",
            hint: "Tab",
            kind: PaletteKind::ModeText,
        },
        PaletteAction {
            label: "Switch to Graph mode",
            hint: "Tab / g",
            kind: PaletteKind::ModeGraph,
        },
        PaletteAction {
            label: "Switch to Flow mode",
            hint: "Tab",
            kind: PaletteKind::ModeFlow,
        },
        PaletteAction {
            label: "Switch to Diagrams mode",
            hint: "Tab",
            kind: PaletteKind::ModeDiagrams,
        },
        PaletteAction {
            label: "Show variables panel",
            hint: "",
            kind: PaletteKind::PanelUsage,
        },
        PaletteAction {
            label: "Show Explorer",
            hint: "Ctrl+B to toggle the sidebar",
            kind: PaletteKind::PanelExplorer,
        },
        PaletteAction {
            label: "Show search panel",
            hint: "/",
            kind: PaletteKind::PanelSearch,
        },
        PaletteAction {
            label: "Show repository health panel",
            hint: "",
            kind: PaletteKind::PanelHealth,
        },
        PaletteAction {
            label: "Show notes panel",
            hint: "",
            kind: PaletteKind::PanelNotes,
        },
        PaletteAction {
            label: "Show Repo 101 (onboarding)",
            hint: "",
            kind: PaletteKind::PanelOnboard,
        },
        PaletteAction {
            label: "Focus search",
            hint: "/",
            kind: PaletteKind::FocusSearch,
        },
        PaletteAction {
            label: "Open repository…",
            hint: "",
            kind: PaletteKind::OpenRepo,
        },
        PaletteAction {
            label: "Quick open a file",
            hint: "Ctrl+P",
            kind: PaletteKind::QuickOpen,
        },
        PaletteAction {
            label: "Export window → PNG",
            hint: "",
            kind: PaletteKind::ExportPng,
        },
        PaletteAction {
            label: "Export tour+health → Markdown",
            hint: "",
            kind: PaletteKind::ExportMd,
        },
        PaletteAction {
            label: "Export architecture report → Markdown",
            hint: "",
            kind: PaletteKind::ExportArchitecture,
        },
        PaletteAction {
            label: "Export call graph → Mermaid",
            hint: "",
            kind: PaletteKind::ExportMermaid,
        },
        PaletteAction {
            label: "Export call graph → Graphviz DOT",
            hint: "",
            kind: PaletteKind::ExportDot,
        },
        PaletteAction {
            label: "Export call graph → SVG",
            hint: "",
            kind: PaletteKind::ExportSvg,
        },
        PaletteAction {
            label: "Toggle light / dark theme",
            hint: "Ctrl+T",
            kind: PaletteKind::ToggleTheme,
        },
        PaletteAction {
            label: "Toggle keyboard shortcuts",
            hint: "?",
            kind: PaletteKind::Shortcuts,
        },
        PaletteAction {
            label: "Clear selection / trace",
            hint: "Esc",
            kind: PaletteKind::ClearSelection,
        },
    ]
}
