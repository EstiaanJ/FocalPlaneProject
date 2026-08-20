#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod curve;
mod loader;
mod pipeline;
mod preview;

use app::CurveApp;
use pipeline::{InputColourSpace, decode_image_bytes, prepare};

fn main() -> eframe::Result {
    let source_bytes = include_bytes!(concat!(env!("OUT_DIR"), "/controlled_adobe_rgb.png"));
    let source = match decode_image_bytes(source_bytes) {
        Ok(source) => source,
        Err(error) => {
            eprintln!("Unable to load controlled curve fixture: {error}");
            return Err(eframe::Error::AppCreation(Box::new(error)));
        }
    };
    let input_colour_space = source
        .profile
        .detected_colour_space
        .unwrap_or(InputColourSpace::Srgb);
    let prepared = match prepare(&source, input_colour_space) {
        Ok(prepared) => prepared,
        Err(error) => {
            eprintln!("Unable to prepare controlled curve fixture: {error}");
            return Err(eframe::Error::AppCreation(Box::new(error)));
        }
    };
    let native_options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("Exposure Curve Tool")
            .with_inner_size([1_180.0, 850.0])
            .with_min_inner_size([760.0, 650.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Exposure Curve Tool",
        native_options,
        Box::new(move |creation_context| {
            Ok(Box::new(CurveApp::new(
                &creation_context.egui_ctx,
                source,
                prepared,
            )))
        }),
    )
}
