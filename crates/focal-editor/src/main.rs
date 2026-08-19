#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod image_io;
mod preview;

use app::FocalEditorApp;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("FocalPlane — Focal Editor")
            .with_inner_size([1_400.0, 860.0])
            .with_min_inner_size([900.0, 600.0]),
        ..Default::default()
    };

    eframe::run_native(
        "FocalPlane — Focal Editor",
        options,
        Box::new(|creation_context| Ok(Box::new(FocalEditorApp::new(&creation_context.egui_ctx)))),
    )
}
