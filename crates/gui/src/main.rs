//! repowalk GUI entry — eframe shell. OWNER: RSK-14.
use eframe::egui;

mod app;
mod explain;
mod export;
mod graph;
mod sidebar;
mod text;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1280.0, 800.0]),
        ..Default::default()
    };
    eframe::run_native(
        "repowalk",
        options,
        Box::new(|cc| Ok(Box::new(app::App::new(cc)))),
    )
}
