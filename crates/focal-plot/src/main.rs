#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod loader;

use app::BetterPlotsApp;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("FocalPlane — Better Plots")
            .with_inner_size([1_280.0, 720.0])
            .with_min_inner_size([800.0, 480.0]),
        ..Default::default()
    };

    eframe::run_native(
        "FocalPlane — Better Plots",
        options,
        Box::new(|creation_context| Ok(Box::new(BetterPlotsApp::new(&creation_context.egui_ctx)))),
    )
}
