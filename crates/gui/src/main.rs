//! repowalk GUI entry — eframe shell. OWNER: RSK-14.
use eframe::egui;

mod app;
mod diagrams;
mod explain;
mod export;
mod flow;
mod graph;
mod health;
mod icons;
mod notes;
mod onboard;
mod quick_open;
mod sidebar;
mod tabs;
mod text;
mod theme;
mod ui_kit;
mod usage;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 800.0])
            // Dynamic layout floor: below this the shell collapses its hint
            // rows and clamps its panels, so a small screen stays usable —
            // but the window is never allowed to become unusably tiny.
            .with_min_inner_size([760.0, 480.0]),
        wgpu_options: wgpu_options(),
        ..Default::default()
    };
    eframe::run_native(
        "repowalk",
        options,
        Box::new(|cc| Ok(Box::new(app::App::new(cc)))),
    )
}

/// Backend selection: eframe's default (`PRIMARY | GL`) tries Vulkan first, and
/// some Windows Vulkan ICDs **segfault the process** during adapter enumeration
/// (observed: 4/4 crashes with vulkan/default, 4/4 stable with dx12/gl). On
/// Windows we therefore drop Vulkan from the default set; `WGPU_BACKEND=...`
/// still forces any backend explicitly, and Linux keeps the stock behavior.
fn wgpu_options() -> egui_wgpu::WgpuConfiguration {
    let mut setup = egui_wgpu::WgpuSetupCreateNew::without_display_handle();
    if cfg!(windows) {
        setup
            .instance_descriptor
            .backends
            .set(egui_wgpu::wgpu::Backends::VULKAN, false);
    }
    egui_wgpu::WgpuConfiguration {
        wgpu_setup: egui_wgpu::WgpuSetup::CreateNew(setup),
        ..Default::default()
    }
}
