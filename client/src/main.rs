mod api;
mod app;
mod components;
mod config;
mod i18n;
mod pages;
mod state;
mod theme;

use app::LifePaperApp;
use eframe::egui;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([960.0, 600.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Life Paper",
        options,
        Box::new(|creation_context| Ok(Box::new(LifePaperApp::new(creation_context)))),
    )
}
