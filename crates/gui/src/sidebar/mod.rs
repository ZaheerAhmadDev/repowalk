//! Sidebar: map · health · tour · search · symbol list. OWNER of aggregate: main agent; panels: RSK-15/16.
pub mod map;
pub mod search;
pub mod tour;

pub fn ui(ui: &mut egui::Ui, app: &mut crate::app::App) {
    map::ui(ui, app);
    tour::ui(ui, app);
    search::ui(ui, app);
}
use eframe::egui;
