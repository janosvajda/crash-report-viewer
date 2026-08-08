mod app;
mod domain;
mod services;
mod ui;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 820.0])
            .with_min_inner_size([920.0, 600.0])
            .with_drag_and_drop(true),
        centered: true,
        // On macOS 26 AppKit can abort while tearing down its internal
        // NSTouchBar observer after winit returns. Let the event loop own
        // process shutdown instead of returning through that broken path.
        run_and_return: false,
        persist_window: false,
        ..Default::default()
    };

    eframe::run_native(
        "CrashLens",
        options,
        Box::new(|cc| Ok(Box::new(app::CrashLens::new(cc)))),
    )
}
