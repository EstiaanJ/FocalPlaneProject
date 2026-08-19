#![allow(
    clippy::assigning_clones,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)]

use std::{
    path::PathBuf,
    sync::mpsc::{Receiver, Sender},
};

use eframe::egui::{
    self, Color32, CornerRadius, Pos2, Rect, Sense, Stroke, StrokeKind, TextureHandle, Vec2,
};
use focal_core::Image;
use serde::{Deserialize, Serialize};

use crate::{
    image_io::{self, DecodedImage, LoadRequest, LoadResult},
    preview::{self, PreviewRequest, PreviewResult},
};

const EDIT_STATE_VERSION: u32 = 1;
const PIPELINE_VERSION: u32 = 1;
const PREVIEW_BACKGROUND: Color32 = Color32::from_rgb(12, 13, 15);
const PANEL_BACKGROUND: Color32 = Color32::from_rgb(24, 26, 29);
const ACCENT: Color32 = Color32::from_rgb(117, 181, 230);

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
struct EditSidecar {
    format: String,
    edit_state_version: u32,
    pipeline_version: u32,
    source_path: String,
    exposure_stops: f32,
    contrast: f32,
}

pub struct FocalEditorApp {
    load_sender: Sender<LoadRequest>,
    load_receiver: Receiver<LoadResult>,
    preview_sender: Sender<PreviewRequest>,
    preview_receiver: Receiver<PreviewResult>,
    next_generation: u64,
    latest_generation: u64,
    loading: bool,
    rendering: bool,
    source_path: Option<PathBuf>,
    source: Option<DecodedImage>,
    output: Option<Image>,
    source_texture: Option<TextureHandle>,
    output_texture: Option<TextureHandle>,
    pending_transparency: Option<(PathBuf, DecodedImage)>,
    sidecar_path: Option<PathBuf>,
    exposure_stops: f32,
    contrast: f32,
    source_histogram: Option<Histogram>,
    output_histogram: Option<Histogram>,
    status: String,
}

impl FocalEditorApp {
    #[must_use]
    pub fn new(context: &egui::Context) -> Self {
        configure_visuals(context);
        let (load_sender, load_receiver) = image_io::spawn_loader();
        let (preview_sender, preview_receiver) = preview::spawn();
        let mut app = Self {
            load_sender,
            load_receiver,
            preview_sender,
            preview_receiver,
            next_generation: 0,
            latest_generation: 0,
            loading: false,
            rendering: false,
            source_path: None,
            source: None,
            output: None,
            source_texture: None,
            output_texture: None,
            pending_transparency: None,
            sidecar_path: None,
            exposure_stops: 0.0,
            contrast: 0.0,
            source_histogram: None,
            output_histogram: None,
            status: "Ready — open a PNG or JPEG to begin".to_owned(),
        };

        if let Some(path) = std::env::args_os().nth(1).map(PathBuf::from) {
            app.open_path(path);
        }
        app
    }

    fn open_path(&mut self, path: PathBuf) {
        self.next_generation = self.next_generation.saturating_add(1);
        self.latest_generation = self.next_generation;
        self.loading = true;
        self.rendering = false;
        self.pending_transparency = None;
        self.status = format!("Opening {}…", path.display());
        if self
            .load_sender
            .send(LoadRequest {
                generation: self.latest_generation,
                path,
            })
            .is_err()
        {
            self.loading = false;
            self.status = "The image loader is unavailable".to_owned();
        }
    }

    fn open_dialog(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("Images", &["png", "jpg", "jpeg"])
            .pick_file()
        {
            self.open_path(path);
        }
    }

    fn poll_background_work(&mut self, context: &egui::Context) {
        while let Ok(result) = self.load_receiver.try_recv() {
            if result.generation != self.latest_generation {
                continue;
            }
            self.loading = false;
            match result.image {
                Ok(image) if image.has_transparency => {
                    self.pending_transparency = Some((result.path, image));
                    self.status =
                        "The image contains transparency; confirmation is required".to_owned();
                }
                Ok(image) => self.install_image(result.path, image, context),
                Err(error) => self.status = error.to_string(),
            }
        }

        while let Ok(result) = self.preview_receiver.try_recv() {
            if result.generation != self.latest_generation {
                continue;
            }
            self.rendering = false;
            match result.image {
                Ok(image) => {
                    self.output_histogram = Some(Histogram::from_pixels(image.pixels()));
                    self.output_texture = Some(context.load_texture(
                        format!("focal-editor-after-{}", result.generation),
                        rgba_image(image.pixels(), image.width(), image.height()),
                        egui::TextureOptions::LINEAR,
                    ));
                    self.output = Some(image);
                    self.status = "Preview updated".to_owned();
                }
                Err(error) => self.status = format!("Preview failed: {error}"),
            }
        }

        if self.loading || self.rendering {
            context.request_repaint_after(std::time::Duration::from_millis(30));
        }
    }

    fn install_image(&mut self, path: PathBuf, image: DecodedImage, context: &egui::Context) {
        let display_path = path.display().to_string();
        self.source_path = Some(path);
        self.sidecar_path = None;
        self.source_histogram = Some(Histogram::from_pixels(&image.pixels));
        self.source_texture = Some(context.load_texture(
            format!("focal-editor-before-{}", self.latest_generation),
            egui::ColorImage::from_rgba_unmultiplied(
                [image.width as usize, image.height as usize],
                &image.rgba,
            ),
            egui::TextureOptions::LINEAR,
        ));
        self.source = Some(image);
        self.output = None;
        self.output_texture = None;
        self.exposure_stops = 0.0;
        self.contrast = 0.0;
        self.status = format!("Loaded {display_path}");
        self.request_preview(context);
    }

    fn confirm_transparency(&mut self, context: &egui::Context) {
        let Some((path, _)) = self.pending_transparency.as_ref() else {
            return;
        };
        let mut decision = None;
        egui::Window::new("Transparency is not supported")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
            .show(context, |ui| {
                ui.label(format!(
                    "{} contains transparent pixels. Focal Editor will flatten them onto black in linear light.",
                    path.display()
                ));
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("Flatten and open").clicked() {
                        decision = Some(true);
                    }
                    if ui.button("Cancel").clicked() {
                        decision = Some(false);
                    }
                });
            });

        if let Some(accept) = decision {
            let pending = self.pending_transparency.take();
            if accept {
                if let Some((path, image)) = pending {
                    self.install_image(path, image.flatten_onto_black(), context);
                }
            } else {
                self.status = "Open cancelled; the source was not modified".to_owned();
            }
        }
    }

    fn request_preview(&mut self, context: &egui::Context) {
        let Some(source) = self.source.as_ref() else {
            return;
        };
        let Ok(image) = source.to_core_image() else {
            self.status =
                "The decoded image does not satisfy FocalCore's image contract".to_owned();
            return;
        };
        self.next_generation = self.next_generation.saturating_add(1);
        self.latest_generation = self.next_generation;
        self.rendering = true;
        self.status = "Rendering preview…".to_owned();
        let request = PreviewRequest {
            generation: self.latest_generation,
            image,
            exposure_stops: self.exposure_stops,
            contrast: self.contrast,
        };
        if self.preview_sender.send(request).is_err() {
            self.rendering = false;
            self.status = "The preview worker is unavailable".to_owned();
        }
        context.request_repaint();
    }

    fn save_sidecar(&mut self) {
        let Some(source_path) = self.source_path.as_ref() else {
            self.status = "Open an image before saving".to_owned();
            return;
        };
        let default_name = format!(
            "{}.focal.json",
            source_path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or("untitled")
        );
        let path = self.sidecar_path.clone().or_else(|| {
            rfd::FileDialog::new()
                .add_filter("Focal edit sidecar", &["json"])
                .set_file_name(default_name)
                .save_file()
        });
        let Some(path) = path else {
            return;
        };
        let state = EditSidecar {
            format: "focal-editor-sidecar".to_owned(),
            edit_state_version: EDIT_STATE_VERSION,
            pipeline_version: PIPELINE_VERSION,
            source_path: source_path.display().to_string(),
            exposure_stops: self.exposure_stops,
            contrast: self.contrast,
        };
        match serde_json::to_string_pretty(&state)
            .map_err(|error| error.to_string())
            .and_then(|json| {
                std::fs::write(&path, format!("{json}\n")).map_err(|error| error.to_string())
            }) {
            Ok(()) => {
                self.sidecar_path = Some(path.clone());
                self.status = format!("Saved {}", path.display());
            }
            Err(error) => self.status = format!("Could not save {}: {error}", path.display()),
        }
    }

    fn export_png(&mut self) {
        let (Some(source_path), Some(output)) = (self.source_path.as_ref(), self.output.as_ref())
        else {
            self.status = "Render a preview before exporting".to_owned();
            return;
        };
        let default_name = format!(
            "{}-edited.png",
            source_path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or("untitled")
        );
        let Some(path) = rfd::FileDialog::new()
            .add_filter("PNG image", &["png"])
            .set_file_name(default_name)
            .save_file()
        else {
            return;
        };
        let pixels = rgba_bytes(output.pixels());
        let result = image::ImageBuffer::<image::Rgba<u8>, _>::from_raw(
            output.width(),
            output.height(),
            pixels,
        )
        .ok_or_else(|| "the rendered buffer dimensions are invalid".to_owned())
        .and_then(|image| image.save(&path).map_err(|error| error.to_string()));
        match result {
            Ok(()) => self.status = format!("Exported {}", path.display()),
            Err(error) => self.status = format!("Could not export {}: {error}", path.display()),
        }
    }
}

impl eframe::App for FocalEditorApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let context = ui.ctx().clone();
        self.poll_background_work(&context);
        self.confirm_transparency(&context);

        self.show_toolbar(ui);
        ui.separator();
        let content_height = (ui.available_height() - 28.0).max(100.0);
        ui.allocate_ui_with_layout(
            Vec2::new(ui.available_width(), content_height),
            egui::Layout::left_to_right(egui::Align::Min),
            |ui| {
                let controls_width = 270.0_f32.min(ui.available_width() * 0.35);
                ui.allocate_ui_with_layout(
                    Vec2::new(controls_width, ui.available_height()),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| self.show_controls(ui, &context),
                );
                ui.separator();
                ui.allocate_ui_with_layout(
                    Vec2::new(ui.available_width(), ui.available_height()),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| self.show_main_panel(ui),
                );
            },
        );
        ui.separator();
        ui.allocate_ui_with_layout(
            Vec2::new(ui.available_width(), 22.0),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.label(&self.status);
                if self.loading || self.rendering {
                    ui.spinner();
                }
                if let Some(path) = &self.source_path {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(path.display().to_string());
                    });
                }
            },
        );
    }
}

impl FocalEditorApp {
    fn show_toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading("Focal Editor");
            ui.separator();
            if ui.button("Open image…").clicked() {
                self.open_dialog();
            }
            let has_image = self.source.is_some();
            if ui
                .add_enabled(has_image, egui::Button::new("Save"))
                .on_hover_text("Save editable parameters as a JSON sidecar")
                .clicked()
            {
                self.save_sidecar();
            }
            if ui
                .add_enabled(self.output.is_some(), egui::Button::new("Export"))
                .on_hover_text("Render the current preview to an 8-bit sRGB PNG")
                .clicked()
            {
                self.export_png();
            }
            ui.separator();
            ui.label("Save keeps the edit; Export renders a new image");
        });
    }

    fn show_controls(&mut self, ui: &mut egui::Ui, context: &egui::Context) {
        ui.heading("Adjustments");
        ui.label(egui::RichText::new("Global controls").weak());
        ui.label(
            egui::RichText::new("Current prototype input: decoded sRGB")
                .small()
                .weak(),
        );
        ui.add_space(8.0);
        let exposure_changed = parameter_row(
            ui,
            "Exposure",
            &mut self.exposure_stops,
            -8.0..=8.0,
            0.01,
            "Stops of exposure compensation",
        );
        let contrast_changed = parameter_row(
            ui,
            "Contrast",
            &mut self.contrast,
            -100.0..=100.0,
            0.1,
            "Temporary FocalCore contrast control",
        );
        if exposure_changed || contrast_changed {
            self.request_preview(context);
        }

        ui.separator();
        ui.heading("Histogram");
        ui.label(egui::RichText::new("Input and output luma distribution").weak());
        if let Some(histogram) = &self.source_histogram {
            draw_histogram(ui, histogram, "Input");
        }
        if let Some(histogram) = &self.output_histogram {
            draw_histogram(ui, histogram, "Output");
        }
        ui.separator();
        ui.heading("Scopes");
        ui.label(egui::RichText::new(
            "FocalPlot scope widgets will be integrated after their analysis boundary is extracted.",
        ).weak());
        ui.add_space(4.0);
        ui.label(egui::RichText::new(
            "The first editor slice keeps this panel reserved so scopes do not become a second processing pipeline.",
        ).small().weak());
    }

    fn show_main_panel(&self, ui: &mut egui::Ui) {
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Before").strong());
                ui.add_space(ui.available_width() * 0.5 - 60.0);
                ui.label(egui::RichText::new("After").strong().color(ACCENT));
            });
            ui.add_space(4.0);
            let panel_height = (ui.available_height() * 0.72).max(250.0);
            ui.allocate_ui_with_layout(
                Vec2::new(ui.available_width(), panel_height),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    let preview_width = ((ui.available_width() - 6.0) * 0.5).max(160.0);
                    Self::show_preview(
                        ui,
                        preview_width,
                        self.source_texture.as_ref(),
                        "Open an image to begin",
                    );
                    ui.add_space(6.0);
                    Self::show_preview(
                        ui,
                        preview_width,
                        self.output_texture.as_ref(),
                        "The rendered preview will appear here",
                    );
                },
            );
            ui.add_space(10.0);
            ui.separator();
            ui.label(egui::RichText::new("FocalPlot scopes").strong());
            ui.label(egui::RichText::new(
                "Scopes are intentionally reserved for the reusable FocalPlot widget integration.",
            ).weak());
        });
    }

    fn show_preview(ui: &mut egui::Ui, width: f32, texture: Option<&TextureHandle>, empty: &str) {
        let height = ui.available_height().max(180.0);
        let (rect, response) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, CornerRadius::same(4), PREVIEW_BACKGROUND);
        if let Some(texture) = texture {
            let image_aspect = texture.size_vec2().x / texture.size_vec2().y;
            let image_rect = fit_rect(rect.shrink(8.0), image_aspect);
            painter.image(
                texture.id(),
                image_rect,
                Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                Color32::WHITE,
            );
        } else {
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                if response.hovered() {
                    "No preview"
                } else {
                    empty
                },
                egui::FontId::proportional(14.0),
                Color32::from_rgb(150, 156, 164),
            );
        }
        painter.rect_stroke(
            rect,
            CornerRadius::same(4),
            Stroke::new(1.0, Color32::from_rgb(55, 59, 65)),
            StrokeKind::Inside,
        );
    }
}

fn parameter_row(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    speed: f32,
    tooltip: &str,
) -> bool {
    let before = *value;
    ui.horizontal(|ui| {
        if ui
            .small_button("↺")
            .on_hover_text(format!("Reset {label}"))
            .clicked()
        {
            *value = 0.0;
        }
        ui.label(label).on_hover_text(tooltip);
    });
    ui.horizontal(|ui| {
        ui.add(egui::Slider::new(value, range.clone()).show_value(false));
        ui.add(egui::DragValue::new(value).speed(speed).range(range));
    });
    (*value - before).abs() > f32::EPSILON
}

#[derive(Clone, Debug, PartialEq)]
struct Histogram {
    channels: [[u32; 256]; 3],
    maximum: u32,
}

impl Histogram {
    fn from_pixels(pixels: &[[f32; 3]]) -> Self {
        let mut channels = [[0_u32; 256]; 3];
        for pixel in pixels {
            for (channel, value) in pixel.iter().enumerate() {
                let index = (value.clamp(0.0, 1.0) * 255.0).round() as usize;
                channels[channel][index] = channels[channel][index].saturating_add(1);
            }
        }
        let maximum = channels.iter().flatten().copied().max().unwrap_or(0);
        Self { channels, maximum }
    }
}

fn draw_histogram(ui: &mut egui::Ui, histogram: &Histogram, label: &str) {
    ui.label(egui::RichText::new(label).small());
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 90.0), Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, CornerRadius::same(2), PREVIEW_BACKGROUND);
    if histogram.maximum > 0 {
        let colours = [
            Color32::from_rgba_unmultiplied(240, 90, 90, 180),
            Color32::from_rgba_unmultiplied(100, 220, 120, 180),
            Color32::from_rgba_unmultiplied(100, 160, 245, 180),
        ];
        let bin_width = rect.width() / 256.0;
        for bin in 0..256 {
            for (channel, colour) in colours.iter().enumerate() {
                let height = rect.height() * histogram.channels[channel][bin] as f32
                    / histogram.maximum as f32;
                if height > 0.0 {
                    let x = rect.left() + bin as f32 * bin_width;
                    painter.line_segment(
                        [
                            Pos2::new(x, rect.bottom()),
                            Pos2::new(x, rect.bottom() - height),
                        ],
                        Stroke::new(1.0, *colour),
                    );
                }
            }
        }
    }
}

fn rgba_image(pixels: &[[f32; 3]], width: u32, height: u32) -> egui::ColorImage {
    egui::ColorImage::from_rgba_unmultiplied([width as usize, height as usize], &rgba_bytes(pixels))
}

fn rgba_bytes(pixels: &[[f32; 3]]) -> Vec<u8> {
    pixels
        .iter()
        .flat_map(|pixel| {
            pixel
                .iter()
                .map(|value| (value.clamp(0.0, 1.0) * 255.0).round() as u8)
                .chain(std::iter::once(u8::MAX))
        })
        .collect()
}

fn fit_rect(bounds: Rect, aspect: f32) -> Rect {
    let safe_aspect = aspect.max(0.001);
    let bounds_aspect = bounds.width() / bounds.height().max(0.001);
    let size = if bounds_aspect > safe_aspect {
        Vec2::new(bounds.height() * safe_aspect, bounds.height())
    } else {
        Vec2::new(bounds.width(), bounds.width() / safe_aspect)
    };
    Rect::from_center_size(bounds.center(), size)
}

fn configure_visuals(context: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    visuals.window_fill = PANEL_BACKGROUND;
    visuals.panel_fill = Color32::from_rgb(18, 20, 22);
    visuals.extreme_bg_color = PREVIEW_BACKGROUND;
    visuals.selection.bg_fill = ACCENT;
    context.set_visuals(visuals);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn histogram_counts_each_channel_and_clips_display_range() {
        let histogram = Histogram::from_pixels(&[[0.0, 0.5, 1.0], [2.0, -1.0, 0.5]]);
        assert_eq!(histogram.channels[0][0], 1);
        assert_eq!(histogram.channels[0][255], 1);
        assert_eq!(histogram.channels[1][0], 1);
        assert_eq!(histogram.channels[1][128], 1);
        assert_eq!(histogram.channels[2][128], 1);
        assert_eq!(histogram.channels[2][255], 1);
    }

    #[test]
    fn sidecar_round_trips_absolute_adjustments() {
        let state = EditSidecar {
            format: "focal-editor-sidecar".to_owned(),
            edit_state_version: EDIT_STATE_VERSION,
            pipeline_version: PIPELINE_VERSION,
            source_path: "/tmp/photo.png".to_owned(),
            exposure_stops: 1.25,
            contrast: -12.0,
        };
        let json = serde_json::to_string(&state).unwrap();
        assert_eq!(serde_json::from_str::<EditSidecar>(&json).unwrap(), state);
    }
}
