//! AppState, layout, mode toggle, keyboard, selection sync (plan §4.2-4.3).
//! OWNER: RSK-14. Types below are the shared contract for all panels.
use eframe::egui;
use repowalk_core::model::Index;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use crate::{graph, sidebar, text};

/// (edges, symbols, edge_calls, edge_imports) signature guarding graph rebuilds.
pub type GraphState = ((usize, usize, bool, bool), crate::graph::render::WalkGraph);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Text,
    Graph,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Selection {
    Symbol(String), // symbol id
    File(String),   // repo-relative path
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortBy {
    FanIn,
    Loc,
    Name,
}

/// What the A→B trace picker is waiting for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TracePick {
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
    pub index: Option<Arc<Index>>,
}

pub struct App {
    pub repo: Option<PathBuf>,
    pub repo_input: String,
    pub index: Option<Arc<Index>>,
    pub mode: Mode,
    pub selection: Option<Selection>,
    pub trace: Option<(String, String)>, // (A, B) flow trace
    pub trace_pick: TracePick,
    pub pending_trace_a: Option<String>,
    pub query: String,
    pub sort: SortBy,
    pub edge_calls: bool,
    pub edge_imports: bool,
    pub show_shortcuts: bool,
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
}

/// The egui context handle used by the indexing thread to request repaints.
static APP_CTX: std::sync::OnceLock<egui::Context> = std::sync::OnceLock::new();

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        cc.egui_ctx.set_theme(egui::Theme::Dark);
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
        };
        if let Some(repo) = repo {
            app.start_indexing(repo);
        }
        app
    }

    /// Kick off a background indexing run (plan §3.1: progress overlay with file counts).
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
        self.status = "indexing…".to_string();
        std::thread::spawn(move || {
            let notify = |done: usize| {
                if let Ok(mut p) = shared.lock() {
                    p.done = done;
                }
                if let Some(ctx) = &ctx {
                    ctx.request_repaint();
                }
            };
            let result = repowalk_core::pipeline::build_index(&repo, &notify);
            let mut p = shared.lock().unwrap();
            p.finished = true;
            match result {
                Ok(idx) => {
                    p.total = idx.files.len();
                    p.index = Some(Arc::new(idx));
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
            if let Some(idx) = p.index.take() {
                self.status = format!("indexed {} files", idx.files.len());
                self.index = Some(idx);
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

    fn open_file_with(&mut self, path: String, line: Option<usize>) {
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

    /// Deterministic module → color assignment from a fixed 8-color palette (plan §4.4).
    pub fn module_color(module: &str) -> egui::Color32 {
        const PALETTE: [egui::Color32; 8] = [
            egui::Color32::from_rgb(97, 175, 239),  // blue
            egui::Color32::from_rgb(152, 195, 121), // green
            egui::Color32::from_rgb(229, 192, 123), // yellow
            egui::Color32::from_rgb(224, 108, 117), // red
            egui::Color32::from_rgb(198, 120, 221), // purple
            egui::Color32::from_rgb(86, 182, 194),  // cyan
            egui::Color32::from_rgb(209, 154, 102), // orange
            egui::Color32::from_rgb(171, 178, 191), // gray
        ];
        let mut h: u64 = 5381;
        for b in module.bytes() {
            h = h.wrapping_mul(33).wrapping_add(b as u64);
        }
        PALETTE[(h % PALETTE.len() as u64) as usize]
    }

    pub fn module_of(path: &str) -> String {
        path.rsplit_once('/')
            .map(|(d, _)| d.to_string())
            .unwrap_or_else(|| "(root)".to_string())
    }

    fn handle_keyboard(&mut self, ctx: &egui::Context) {
        let (tab, slash, esc, question, g, enter, j, k, ctrl_s) = ctx.input(|i| {
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
            )
        });
        if tab {
            self.mode = match self.mode {
                Mode::Text => Mode::Graph,
                Mode::Graph => Mode::Text,
            };
        }
        if slash {
            self.search_focus = true;
        }
        if esc {
            self.clear_transient();
            self.show_shortcuts = false;
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
                egui::Event::Screenshot { image, user_data, .. } => {
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
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.poll_progress();
        self.handle_keyboard(&ctx);
        self.consume_screenshot_event(&ctx);

        egui::Panel::left("sidebar")
            .resizable(true)
            .default_size(260.0)
            .show(ui, |ui| {
                sidebar::ui(ui, self);
            });

        egui::Panel::top("tabs").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.mode, Mode::Text, "Text");
                ui.selectable_value(&mut self.mode, Mode::Graph, "Graph");
                ui.separator();
                ui.menu_button("export ▾", |ui| {
                    if ui.button("window → PNG").clicked() {
                        self.export_png(ui.ctx());
                        ui.close();
                    }
                    if ui.button("tour + health → Markdown").clicked() {
                        self.export_markdown();
                        ui.close();
                    }
                });
                ui.separator();
                if let Some(idx) = &self.index {
                    let m = repowalk_core::metrics::compute(idx);
                    ui.label(format!(
                        "{} files · {} LOC · {} cycles",
                        idx.files.len(),
                        m.total_loc,
                        m.cycles.len()
                    ));
                    ui.separator();
                    ui.weak(repowalk_core::VERSION);
                }
                if !self.status.is_empty() {
                    ui.separator();
                    ui.weak(&self.status);
                }
            });
        });

        if let Some(err) = self.error.clone() {
            egui::Panel::bottom("errors").show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.colored_label(egui::Color32::from_rgb(224, 108, 117), err);
                    if ui.button("dismiss").clicked() {
                        self.error = None;
                    }
                });
            });
        }

        match self.mode {
            Mode::Text => {
                egui::CentralPanel::default_margins().show(ui, |ui| {
                    text::ui(ui, self);
                });
            }
            Mode::Graph => {
                egui::CentralPanel::default_margins().show(ui, |ui| {
                    graph::ui(ui, self);
                });
            }
        }

        if self.show_shortcuts {
            egui::Window::new("Keyboard shortcuts").show(ui.ctx(), |ui| {
                ui.monospace(
                    "Tab    toggle Text ⇄ Graph\n\
                     /      focus search\n\
                     j/k    move through sidebar lists\n\
                     Enter  jump to / select\n\
                     g      focus graph\n\
                     Esc    clear selection / trace\n\
                     Ctrl+S save the edited file\n\
                     ?      show this overlay",
                );
            });
        }

        if self.index.is_none() && self.error.is_none() {
            let done = self.progress.lock().unwrap().done;
            egui::Area::new(egui::Id::new("progress-overlay"))
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ui.ctx(), |ui| {
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        ui.label(format!("indexing {} files…", if done > 0 { done.to_string() } else { "…".to_string() }));
                    });
                });
        }
    }
}
