//! AppState, layout shell, mode toggle, keyboard, selection sync (plan §4.2-4.3).
//! Redesigned around the `theme` token system: toolbar (segmented control +
//! export menu), sidebar, central pane, status bar. OWNER: RSK-14.
use eframe::egui;
use repowalk_core::analysis::Analysis;
use repowalk_core::model::Index;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use crate::theme;
use crate::{diagrams, flow, graph, sidebar, text, usage};

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
    pub graph_state: Option<GraphState>,
    pub flow: crate::flow::FlowState,
    /// v2 analysis artifact; `None` until the first `learn` completes.
    pub analysis: Option<Arc<Analysis>>,
    pub sidebar_tab: sidebar::Tab,
    pub usage: usage::UsageState,
    pub diagrams: diagrams::DiagramsState,
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
            graph_state: None,
            flow: Default::default(),
            analysis: None,
            sidebar_tab: sidebar::Tab::Map,
            usage: Default::default(),
            diagrams: Default::default(),
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
                }
                if let Some(ctx) = &ctx {
                    ctx.request_repaint();
                }
            };
            let result = repowalk_core::learn::learn(&repo, &notify);
            let mut p = shared.lock().unwrap();
            p.finished = true;
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
            if let Some(e) = p.error.take() {
                self.error = Some(e);
                self.status.clear();
            }
            p.finished = false;
        }
    }

    /// One selection drives every pane (plan §4.2).
    pub fn select(&mut self, sel: Selection) {
        match &sel {
            Selection::File(path) => {
                self.open_file_with(path.clone(), None);
            }
            Selection::Symbol(id) => {
                if let Some(idx) = &self.index {
                    if let Some(sym) = idx.symbols.iter().find(|s| s.id == *id) {
                        let file = sym.file.clone();
                        let line = sym.start_line;
                        self.open_file_with(file, Some(line));
                    }
                }
            }
        }
        self.selection = Some(sel);
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
        let (tab, slash, esc, question, g, enter, j, k, ctrl_s, ctrl_k) = ctx.input(|i| {
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
        if tab {
            self.mode = match self.mode {
                Mode::Text => Mode::Graph,
                Mode::Graph => Mode::Flow,
                Mode::Flow => Mode::Diagrams,
                Mode::Diagrams => Mode::Text,
            };
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
            self.mode = Mode::Graph;
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
                self.mode = Mode::Text;
            }
            if mode_segment(ui, "Graph", "Tab", graph_active).clicked() {
                self.mode = Mode::Graph;
            }
            if mode_segment(ui, "Flow", "Tab", flow_active).clicked() {
                self.mode = Mode::Flow;
            }
            if mode_segment(ui, "Diagrams", "Tab", diagrams_active).clicked() {
                self.mode = Mode::Diagrams;
            }

            ui.separator();

            if toolbar_button(ui, "Open…", "Open a repository folder").clicked() {
                self.show_open_popup = true;
            }
            if toolbar_button(ui, "⌘K", "Command palette (Ctrl+K)").clicked() {
                self.show_command_palette = true;
                self.palette_query.clear();
            }

            ui.separator();

            ui.menu_button("Export ▾", |ui| {
                if ui.button("Window → PNG").clicked() {
                    self.export_png(ui.ctx());
                    ui.close();
                }
                if ui.button("Tour + health → Markdown").clicked() {
                    self.export_markdown();
                    ui.close();
                }
            });

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

    /// Modal-ish open-repo popup (progressive disclosure: path + Index button).
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
                    || ui.button("Index").clicked();
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
                PaletteKind::ModeText => self.mode = Mode::Text,
                PaletteKind::ModeGraph => self.mode = Mode::Graph,
                PaletteKind::ModeFlow => self.mode = Mode::Flow,
                PaletteKind::ModeDiagrams => self.mode = Mode::Diagrams,
                PaletteKind::PanelUsage => self.sidebar_tab = sidebar::Tab::Usage,
                PaletteKind::PanelHealth => self.sidebar_tab = sidebar::Tab::Health,
                PaletteKind::PanelNotes => self.sidebar_tab = sidebar::Tab::Notes,
                PaletteKind::PanelOnboard => self.sidebar_tab = sidebar::Tab::Onboard,
                PaletteKind::FocusSearch => self.search_focus = true,
                PaletteKind::OpenRepo => self.show_open_popup = true,
                PaletteKind::ExportPng => self.export_png(ctx),
                PaletteKind::ExportMd => self.export_markdown(),
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
        let (done, total, phase) = {
            let p = self.progress.lock().unwrap();
            (p.done, p.total, p.phase.clone())
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

        // Top toolbar strip.
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

        // Left sidebar.
        egui::Panel::left("sidebar")
            .resizable(true)
            .default_size(theme::SIDEBAR_DEFAULT)
            .min_size(theme::SIDEBAR_MIN)
            .max_size(theme::SIDEBAR_MAX)
            .show(ui, |ui| {
                ui.add_space(theme::SPACE_1);
                sidebar::ui(ui, self);
            });

        // Central pane: Text | Graph.
        match self.mode {
            Mode::Text => {
                egui::CentralPanel::default().show(ui, |ui| {
                    ui.add_space(theme::SPACE_1);
                    text::ui(ui, self);
                });
            }
            Mode::Graph => {
                egui::CentralPanel::default().show(ui, |ui| {
                    ui.add_space(theme::SPACE_1);
                    graph::ui(ui, self);
                });
            }
            Mode::Flow => {
                egui::CentralPanel::default().show(ui, |ui| {
                    ui.add_space(theme::SPACE_1);
                    flow::ui(&ctx, ui, self);
                });
            }
            Mode::Diagrams => {
                egui::CentralPanel::default().show(ui, |ui| {
                    ui.add_space(theme::SPACE_1);
                    diagrams::ui(ui, self);
                });
            }
        }

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
        self.progress_overlay(ui);
    }
}

// ── Toolbar helpers ─────────────────────────────────────────────────────────

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

/// Plain toolbar button with optional hover tooltip.
fn toolbar_button(ui: &mut egui::Ui, label: &str, tooltip: &str) -> egui::Response {
    ui.add(egui::Button::new(
        egui::RichText::new(label).color(theme::TEXT_SECONDARY),
    ))
    .on_hover_text(tooltip)
}

// ── Command palette model ───────────────────────────────────────────────────

enum PaletteKind {
    ModeText,
    ModeGraph,
    ModeFlow,
    ModeDiagrams,
    PanelUsage,
    PanelHealth,
    PanelNotes,
    PanelOnboard,
    FocusSearch,
    OpenRepo,
    ExportPng,
    ExportMd,
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
