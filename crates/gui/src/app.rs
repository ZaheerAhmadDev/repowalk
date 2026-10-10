//! AppState, layout, mode toggle, keyboard, selection sync (plan §4.2-4.3).
//! OWNER: RSK-14. Types below are the shared contract for all panels.
use eframe::egui;
use repowalk_core::model::Index;
use std::sync::Arc;

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

pub struct App {
    pub index: Option<Arc<Index>>,
    pub mode: Mode,
    pub selection: Option<Selection>,
    pub trace: Option<(String, String)>, // (A, B) flow trace
    pub query: String,
}

impl App {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        App {
            index: None,
            mode: Mode::Text,
            selection: None,
            trace: None,
            query: String::new(),
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::SidePanel::left("sidebar").resizable(true).default_width(260.0).show(ctx, |ui| {
            sidebar::ui(ui, self);
        });
        egui::TopBottomPanel::top("tabs").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.mode, Mode::Text, "Text");
                ui.selectable_value(&mut self.mode, Mode::Graph, "Graph");
                ui.label(format!("repowalk {}", repowalk_core::VERSION));
            });
        });
        egui::CentralPanel::default().show(ctx, |ui| match self.mode {
            Mode::Text => text::ui(ui, self),
            Mode::Graph => graph::ui(ui, self),
        });
    }
}
