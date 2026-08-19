#![allow(
    clippy::assigning_clones,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)]

use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        Arc,
        mpsc::{Receiver, Sender},
    },
};

use eframe::egui::{
    self, Color32, CornerRadius, Pos2, Rect, Sense, Stroke, StrokeKind, TextureHandle, Vec2,
};
use focal_core::Image;
use focal_plot::vectorscope::{CIE1931_LOCUS, DensityScale, ScopeSpace, render_trace, ring_colour};
use serde::{Deserialize, Serialize};

use crate::{
    image_io::{
        self, DecodedImage, LoadRequest, LoadResult, Thumbnail, ThumbnailRequest, ThumbnailResult,
    },
    preview::{self, PreviewEvent, PreviewRequest, PreviewWorker},
    scope::{self, ScopeRequest, ScopeResult},
};

const EDIT_STATE_VERSION: u32 = 1;
const PIPELINE_VERSION: u32 = 1;
const PREVIEW_BACKGROUND: Color32 = Color32::from_rgb(12, 13, 15);
const PANEL_BACKGROUND: Color32 = Color32::from_rgb(24, 26, 29);
const ACCENT: Color32 = Color32::from_rgb(117, 181, 230);
const LEFT_RAIL_WIDTH: f32 = 190.0;
const RIGHT_RAIL_WIDTH: f32 = 330.0;
const FILMSTRIP_HEIGHT: f32 = 132.0;
const TOOLBAR_HEIGHT: f32 = 38.0;
// A deliberately generous hit target keeps the splitters usable at dense
// desktop sizes. The painted centre line remains subtle, but the whole strip
// can be grabbed with the pointer.
const RESIZER_THICKNESS: f32 = 10.0;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum ScopeTab {
    #[default]
    Histogram,
    Cie1931,
    Ryb,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
struct EditSidecar {
    format: String,
    edit_state_version: u32,
    pipeline_version: u32,
    source_path: String,
    exposure_stops: f32,
    contrast: f32,
}

struct FilmStripItem {
    path: PathBuf,
    thumbnail: Option<TextureHandle>,
    dimensions: Option<[usize; 2]>,
    thumbnail_requested: bool,
}

pub struct FocalEditorApp {
    load_sender: Sender<LoadRequest>,
    load_receiver: Receiver<LoadResult>,
    thumbnail_sender: Sender<ThumbnailRequest>,
    thumbnail_receiver: Receiver<ThumbnailResult>,
    preview_worker: PreviewWorker,
    preview_receiver: Receiver<PreviewEvent>,
    scope_sender: Sender<ScopeRequest>,
    scope_receiver: Receiver<ScopeResult>,
    next_generation: u64,
    latest_load_generation: u64,
    latest_generation: u64,
    thumbnail_generation: u64,
    pending_thumbnails: usize,
    loading: bool,
    rendering: bool,
    scoping: bool,
    render_progress: f32,
    source_path: Option<PathBuf>,
    source: Option<DecodedImage>,
    source_core: Option<Arc<Image>>,
    output: Option<Image>,
    output_generation: Option<u64>,
    source_texture: Option<TextureHandle>,
    output_texture: Option<TextureHandle>,
    pending_transparency: Option<(PathBuf, DecodedImage)>,
    sidecar_path: Option<PathBuf>,
    exposure_stops: f32,
    contrast: f32,
    source_histogram: Option<Histogram>,
    output_histogram: Option<Histogram>,
    scope_tab: ScopeTab,
    cie_scope_texture: Option<TextureHandle>,
    ryb_scope_texture: Option<TextureHandle>,
    film_strip: Vec<FilmStripItem>,
    left_rail_width: f32,
    right_rail_width: f32,
    filmstrip_height: f32,
    navigator_height: f32,
    histogram_height: f32,
    status: String,
}

impl FocalEditorApp {
    #[must_use]
    pub fn new(context: &egui::Context) -> Self {
        configure_visuals(context);
        let (load_sender, load_receiver) = image_io::spawn_loader();
        let (thumbnail_sender, thumbnail_receiver) = image_io::spawn_thumbnail_loader();
        let (preview_worker, preview_receiver) = preview::spawn();
        let (scope_sender, scope_receiver) = scope::spawn();
        let mut app = Self {
            load_sender,
            load_receiver,
            thumbnail_sender,
            thumbnail_receiver,
            preview_worker,
            preview_receiver,
            scope_sender,
            scope_receiver,
            next_generation: 0,
            latest_load_generation: 0,
            latest_generation: 0,
            thumbnail_generation: 0,
            pending_thumbnails: 0,
            loading: false,
            rendering: false,
            scoping: false,
            render_progress: 0.0,
            source_path: None,
            source: None,
            source_core: None,
            output: None,
            output_generation: None,
            source_texture: None,
            output_texture: None,
            pending_transparency: None,
            sidecar_path: None,
            exposure_stops: 0.0,
            contrast: 0.0,
            source_histogram: None,
            output_histogram: None,
            scope_tab: ScopeTab::default(),
            cie_scope_texture: None,
            ryb_scope_texture: None,
            film_strip: Vec::new(),
            left_rail_width: LEFT_RAIL_WIDTH,
            right_rail_width: RIGHT_RAIL_WIDTH,
            filmstrip_height: FILMSTRIP_HEIGHT,
            navigator_height: 170.0,
            histogram_height: 185.0,
            status: "Ready — open a PNG or JPEG to begin".to_owned(),
        };

        if let Some(path) = std::env::args_os().nth(1).map(PathBuf::from) {
            app.open_path(path);
        }
        app
    }

    fn open_path(&mut self, path: PathBuf) {
        self.preview_worker.cancel();
        self.next_generation = self.next_generation.saturating_add(1);
        self.latest_load_generation = self.next_generation;
        self.prepare_film_strip(&path);
        self.loading = true;
        self.rendering = false;
        self.output_generation = None;
        self.pending_transparency = None;
        self.status = format!("Opening {}…", path.display());
        if self
            .load_sender
            .send(LoadRequest {
                generation: self.latest_load_generation,
                path,
            })
            .is_err()
        {
            self.loading = false;
            self.status = "The image loader is unavailable".to_owned();
        }
    }

    fn prepare_film_strip(&mut self, selected: &PathBuf) {
        let mut paths = discover_sibling_images(selected);
        if !paths.iter().any(|path| path == selected) {
            paths.push(selected.clone());
        }
        paths.sort();
        let current_paths = self
            .film_strip
            .iter()
            .map(|item| item.path.as_path())
            .collect::<Vec<_>>();
        if film_strip_paths_match(&current_paths, &paths) {
            return;
        }

        self.thumbnail_generation = self.latest_load_generation;
        self.pending_thumbnails = 0;
        self.film_strip = reconcile_film_strip(std::mem::take(&mut self.film_strip), paths);
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
            if !self.load_result_is_current(result.generation) {
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

        while let Ok(result) = self.thumbnail_receiver.try_recv() {
            if result.generation != self.thumbnail_generation {
                continue;
            }
            self.pending_thumbnails = self.pending_thumbnails.saturating_sub(1);
            let Ok(thumbnail) = result.image else {
                continue;
            };
            let Some(item) = self
                .film_strip
                .iter_mut()
                .find(|item| item.path == result.path)
            else {
                continue;
            };
            item.dimensions = Some([thumbnail.width as usize, thumbnail.height as usize]);
            item.thumbnail = Some(context.load_texture(
                format!("focal-editor-thumbnail-{}", result.path.display()),
                thumbnail_color_image(&thumbnail),
                egui::TextureOptions::LINEAR,
            ));
        }

        while let Ok(event) = self.preview_receiver.try_recv() {
            match event {
                PreviewEvent::Progress {
                    generation,
                    progress,
                } if generation == self.latest_generation => {
                    self.render_progress = progress.fraction;
                }
                PreviewEvent::Complete { generation, image }
                    if generation == self.latest_generation =>
                {
                    self.rendering = false;
                    self.render_progress = 1.0;
                    match image {
                        Ok(image) => {
                            self.output_histogram = Some(Histogram::from_pixels(image.pixels()));
                            self.output_texture = Some(context.load_texture(
                                format!("focal-editor-after-{generation}"),
                                rgba_image(image.pixels(), image.width(), image.height()),
                                egui::TextureOptions::LINEAR,
                            ));
                            self.scoping = self
                                .scope_sender
                                .send(ScopeRequest {
                                    generation,
                                    image: image.clone(),
                                })
                                .is_ok();
                            self.output = Some(image);
                            self.output_generation = Some(generation);
                            self.status = "Preview updated".to_owned();
                        }
                        Err(focal_core::PipelineError::Cancelled { .. }) => {}
                        Err(error) => self.status = format!("Preview failed: {error}"),
                    }
                }
                PreviewEvent::Progress { .. } | PreviewEvent::Complete { .. } => {}
            }
        }

        while let Ok(result) = self.scope_receiver.try_recv() {
            if result.generation != self.latest_generation {
                continue;
            }
            self.cie_scope_texture = Some(context.load_texture(
                format!("focal-editor-cie-scope-{}", result.generation),
                render_trace(&result.cie1931, 1.0, 0.55, DensityScale::Linear, false),
                egui::TextureOptions::LINEAR,
            ));
            self.ryb_scope_texture = Some(context.load_texture(
                format!("focal-editor-ryb-scope-{}", result.generation),
                render_trace(&result.ryb, 1.0, 0.55, DensityScale::Logarithmic, false),
                egui::TextureOptions::LINEAR,
            ));
            self.scoping = false;
        }

        if background_work_needs_repaint(
            self.loading,
            self.rendering,
            self.scoping,
            self.pending_thumbnails,
        ) {
            context.request_repaint_after(std::time::Duration::from_millis(30));
        }
    }

    fn load_result_is_current(&self, generation: u64) -> bool {
        generation == self.latest_load_generation
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
        // Keep the After pane useful while the first background render is in
        // flight. The completed render replaces this texture with the actual
        // FocalCore result.
        self.output_texture = self.source_texture.clone();
        self.source_core = image.to_core_image().ok().map(Arc::new);
        self.source = Some(image);
        self.output = None;
        self.output_generation = None;
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
        let Some(source) = self.source_core.as_ref() else {
            return;
        };
        self.next_generation = self.next_generation.saturating_add(1);
        self.latest_generation = self.next_generation;
        self.rendering = true;
        self.output_generation = None;
        self.scoping = false;
        self.render_progress = 0.0;
        self.status = "Rendering preview…".to_owned();
        let request = PreviewRequest {
            generation: self.latest_generation,
            source: Arc::clone(source),
            snapshot: preview::snapshot_with_adjustments(self.exposure_stops, self.contrast),
        };
        if self.preview_worker.submit(request).is_err() {
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
        if !self.can_export() {
            self.status = "Wait for the current render before exporting".to_owned();
            return;
        }
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

    fn can_export(&self) -> bool {
        !self.rendering
            && self.output.is_some()
            && self.output_generation == Some(self.latest_generation)
    }
}

impl eframe::App for FocalEditorApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let context = ui.ctx().clone();
        self.poll_background_work(&context);
        self.confirm_transparency(&context);

        ui.allocate_ui_with_layout(
            Vec2::new(ui.available_width(), TOOLBAR_HEIGHT),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| self.show_toolbar(ui),
        );
        let filmstrip_height = self
            .filmstrip_height
            .clamp(90.0, (ui.available_height() - 160.0).max(90.0));
        let content_height =
            (ui.available_height() - filmstrip_height - RESIZER_THICKNESS).max(100.0);
        ui.allocate_ui_with_layout(
            Vec2::new(ui.available_width(), content_height),
            egui::Layout::left_to_right(egui::Align::Min),
            |ui| {
                let controls_width = self
                    .left_rail_width
                    .clamp(140.0, (ui.available_width() - 300.0).max(140.0));
                ui.allocate_ui_with_layout(
                    Vec2::new(controls_width, ui.available_height()),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| self.show_left_panel(ui, &context),
                );
                let left_handle = resize_handle(ui, ResizeDirection::Horizontal);
                if left_handle.dragged() {
                    self.left_rail_width =
                        (self.left_rail_width + left_handle.drag_delta().x).clamp(140.0, 520.0);
                }
                let right_width = self
                    .right_rail_width
                    .clamp(240.0, (ui.available_width() - 180.0).max(240.0));
                let centre_width =
                    (ui.available_width() - right_width - RESIZER_THICKNESS).max(120.0);
                ui.allocate_ui_with_layout(
                    Vec2::new(centre_width, ui.available_height()),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| self.show_main_panel(ui),
                );
                let right_handle = resize_handle(ui, ResizeDirection::Horizontal);
                if right_handle.dragged() {
                    self.right_rail_width =
                        (self.right_rail_width - right_handle.drag_delta().x).clamp(240.0, 560.0);
                }
                ui.allocate_ui_with_layout(
                    Vec2::new(right_width, ui.available_height()),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| self.show_right_panel(ui, &context),
                );
            },
        );
        let filmstrip_handle = resize_handle(ui, ResizeDirection::Vertical);
        if filmstrip_handle.dragged() {
            self.filmstrip_height =
                (self.filmstrip_height - filmstrip_handle.drag_delta().y).clamp(90.0, 300.0);
        }
        ui.allocate_ui_with_layout(
            Vec2::new(ui.available_width(), filmstrip_height),
            egui::Layout::top_down(egui::Align::Min),
            |ui| self.show_film_strip(ui, &context),
        );
    }
}

impl FocalEditorApp {
    fn show_toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
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
                .add_enabled(self.can_export(), egui::Button::new("Export"))
                .on_hover_text("Render the current preview to an 8-bit sRGB PNG")
                .clicked()
            {
                self.export_png();
            }
            ui.add_space(12.0);
            let (progress, label) = if self.loading {
                (0.25, "Loading…")
            } else if self.rendering {
                (self.render_progress, "Rendering…")
            } else {
                (1.0, "Ready")
            };
            ui.add(
                egui::ProgressBar::new(progress)
                    .desired_width(132.0)
                    .text(label)
                    .animate(progress < 1.0),
            );
            ui.add_space(8.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(egui::RichText::new("FOCALPLANE").strong());
                ui.add_space(12.0);
                ui.label(egui::RichText::new(&self.status).small().weak());
            });
        });
    }

    fn show_left_panel(&mut self, ui: &mut egui::Ui, context: &egui::Context) {
        ui.heading("Navigator");
        ui.add_space(4.0);
        let navigator_height = self
            .navigator_height
            .clamp(90.0, (ui.available_height() - 120.0).max(90.0));
        let (rect, _) = ui.allocate_exact_size(
            Vec2::new(ui.available_width(), navigator_height),
            Sense::hover(),
        );
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, CornerRadius::same(4), PREVIEW_BACKGROUND);
        if let Some(texture) = &self.source_texture {
            let image_rect = fit_rect(
                rect.shrink(5.0),
                texture.size_vec2().x / texture.size_vec2().y,
            );
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
                "No photo",
                egui::FontId::proportional(12.0),
                Color32::from_gray(140),
            );
        }
        painter.rect_stroke(
            rect,
            CornerRadius::same(4),
            Stroke::new(1.0, Color32::from_gray(62)),
            StrokeKind::Inside,
        );
        ui.label(
            egui::RichText::new("Zoom controls will be added later")
                .small()
                .weak(),
        );
        let navigator_handle = resize_handle(ui, ResizeDirection::Vertical);
        if navigator_handle.dragged() {
            self.navigator_height =
                (self.navigator_height + navigator_handle.drag_delta().y).clamp(90.0, 420.0);
        }
        ui.heading("Presets");
        ui.add_space(4.0);
        if ui.selectable_label(true, "Digital Neutral").clicked() {
            self.exposure_stops = 0.0;
            self.contrast = 0.0;
            self.request_preview(context);
        }
        ui.label(egui::RichText::new("No saved presets yet").small().weak());
        ui.label(
            egui::RichText::new("Presets will remain separate from photo-specific edit state.")
                .small()
                .weak(),
        );
    }

    fn show_right_panel(&mut self, ui: &mut egui::Ui, context: &egui::Context) {
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.scope_tab, ScopeTab::Histogram, "Histograms");
            ui.selectable_value(&mut self.scope_tab, ScopeTab::Cie1931, "CIE 1931");
            ui.selectable_value(&mut self.scope_tab, ScopeTab::Ryb, "RYB");
        });
        ui.label(
            egui::RichText::new(match self.scope_tab {
                ScopeTab::Histogram => "Display-encoded RGB",
                ScopeTab::Cie1931 | ScopeTab::Ryb => {
                    "Decoded sRGB assumption · FocalPlot / darktable-inspired"
                }
            })
            .small()
            .weak(),
        );
        let available = ui.available_size();
        let (histogram_height, controls_height) = split_panel_heights(
            available.y,
            self.histogram_height,
            80.0,
            120.0,
            RESIZER_THICKNESS,
        );
        self.histogram_height = histogram_height;
        let (histogram_rect, _) =
            ui.allocate_exact_size(Vec2::new(available.x, histogram_height), Sense::hover());
        ui.scope_builder(
            egui::UiBuilder::new().max_rect(histogram_rect),
            |ui| match self.scope_tab {
                ScopeTab::Histogram => {
                    let count = usize::from(self.source_histogram.is_some())
                        + usize::from(self.output_histogram.is_some());
                    let chart_height =
                        ((histogram_height - count as f32 * 18.0) / count.max(1) as f32).max(16.0);
                    if let Some(histogram) = &self.source_histogram {
                        draw_histogram(ui, histogram, "Input", chart_height);
                    }
                    if let Some(histogram) = &self.output_histogram {
                        draw_histogram(ui, histogram, "Output", chart_height);
                    }
                }
                ScopeTab::Cie1931 => {
                    draw_scope(ui, self.cie_scope_texture.as_ref(), ScopeSpace::Cie1931);
                }
                ScopeTab::Ryb => {
                    draw_scope(ui, self.ryb_scope_texture.as_ref(), ScopeSpace::Ryb);
                }
            },
        );
        let histogram_handle = resize_handle(ui, ResizeDirection::Vertical);
        if histogram_handle.dragged() {
            self.histogram_height += histogram_handle.drag_delta().y;
        }
        let (controls_rect, _) =
            ui.allocate_exact_size(Vec2::new(available.x, controls_height), Sense::hover());
        ui.scope_builder(egui::UiBuilder::new().max_rect(controls_rect), |ui| {
            egui::ScrollArea::vertical()
                .id_salt("focal-editor-controls")
                .auto_shrink([false, false])
                .show(ui, |ui| self.show_controls(ui, context));
        });
    }

    fn show_controls(&mut self, ui: &mut egui::Ui, context: &egui::Context) {
        ui.heading("Controls");
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
    }

    fn show_film_strip(&mut self, ui: &mut egui::Ui, _context: &egui::Context) {
        ui.horizontal(|ui| {
            ui.heading("Film Strip");
            if !self.film_strip.is_empty() {
                ui.label(egui::RichText::new(format!("{} photos", self.film_strip.len())).weak());
            }
        });
        let mut selected = None;
        let mut visible_indices = Vec::new();
        egui::ScrollArea::horizontal()
            .id_salt("focal-editor-film-strip")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    for (index, item) in self.film_strip.iter_mut().enumerate() {
                        let is_selected = self.source_path.as_ref() == Some(&item.path);
                        let response = film_strip_item(ui, item, is_selected);
                        if response.rect.intersects(ui.clip_rect()) {
                            visible_indices.push(index);
                        }
                        if response.clicked() {
                            selected = Some(item.path.clone());
                        }
                    }
                });
            });
        let mut thumbnail_requests = Vec::new();
        if let (Some(first), Some(last)) = (
            visible_indices.first().copied(),
            visible_indices.last().copied(),
        ) {
            let range = prefetch_range(self.film_strip.len(), first, last);
            for item in &mut self.film_strip[range] {
                if thumbnail_needs_request(item) {
                    item.thumbnail_requested = true;
                    thumbnail_requests.push(item.path.clone());
                }
            }
        }
        for path in thumbnail_requests {
            if self
                .thumbnail_sender
                .send(ThumbnailRequest {
                    generation: self.thumbnail_generation,
                    path,
                })
                .is_ok()
            {
                self.pending_thumbnails = self.pending_thumbnails.saturating_add(1);
            }
        }
        if let Some(path) = selected
            && self.source_path.as_ref() != Some(&path)
        {
            self.open_path(path);
        }
    }

    fn show_main_panel(&self, ui: &mut egui::Ui) {
        // The two previews intentionally consume the entire centre pane. Any
        // letterboxing is caused only by preserving the source aspect ratio;
        // there is no additional card padding, label row, or decorative
        // border to steal pixels from the image.
        let available = ui.available_size();
        let before_size = Vec2::new((available.x * 0.5).floor(), available.y);
        let after_size = Vec2::new(available.x - before_size.x, available.y);
        let before_pixels = self
            .source
            .as_ref()
            .map(|image| (image.pixels.as_slice(), [image.width, image.height]));
        let after_pixels = self
            .output
            .as_ref()
            .map(|image| (image.pixels(), [image.width(), image.height()]))
            .or(before_pixels);

        ui.allocate_ui_with_layout(
            available,
            egui::Layout::left_to_right(egui::Align::Min),
            |ui| {
                ui.allocate_ui_with_layout(
                    before_size,
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        Self::show_preview(
                            ui,
                            before_size,
                            self.source_texture.as_ref(),
                            before_pixels,
                            "Before",
                            "Open an image to begin",
                        );
                    },
                );
                ui.allocate_ui_with_layout(
                    after_size,
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        Self::show_preview(
                            ui,
                            after_size,
                            self.output_texture
                                .as_ref()
                                .or(self.source_texture.as_ref()),
                            after_pixels,
                            "After",
                            "The rendered preview will appear here",
                        );
                    },
                );
            },
        );
    }

    fn show_preview(
        ui: &mut egui::Ui,
        size: Vec2,
        texture: Option<&TextureHandle>,
        pixels: Option<(&[[f32; 3]], [u32; 2])>,
        label: &str,
        empty: &str,
    ) {
        let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, CornerRadius::ZERO, PREVIEW_BACKGROUND);
        let image_rect = texture.map(|texture| {
            fit_rect(
                rect,
                texture.size_vec2().x / texture.size_vec2().y.max(0.001),
            )
        });
        if let (Some(texture), Some(image_rect)) = (texture, image_rect) {
            painter.image(
                texture.id(),
                image_rect,
                Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                Color32::WHITE,
            );
            if response.hovered() {
                let font = egui::FontId::proportional(14.0);
                let label_position = rect.left_top() + Vec2::splat(8.0);
                let galley = painter.layout_no_wrap(label.to_owned(), font.clone(), Color32::WHITE);
                let label_rect = Rect::from_min_size(label_position, galley.size());
                let overlap_luminance = pixels.and_then(|(pixels, dimensions)| {
                    overlay_luminance(pixels, dimensions, image_rect, label_rect)
                });
                let (text_colour, shadow_colour) = match overlap_luminance {
                    Some(luminance) if luminance < 0.4 => (Color32::WHITE, Some(Color32::BLACK)),
                    Some(_) => (Color32::BLACK, Some(Color32::WHITE)),
                    None => (Color32::WHITE, None),
                };
                if let Some(shadow_colour) = shadow_colour {
                    for (offset, colour) in soft_text_shadow(shadow_colour) {
                        painter.text(
                            label_position + offset,
                            egui::Align2::LEFT_TOP,
                            label,
                            font.clone(),
                            colour,
                        );
                    }
                }
                painter.text(
                    label_position,
                    egui::Align2::LEFT_TOP,
                    label,
                    font,
                    text_colour,
                );
            }
        } else if response.hovered() {
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                empty,
                egui::FontId::proportional(14.0),
                Color32::from_rgb(150, 156, 164),
            );
        }
    }
}

#[derive(Clone, Copy)]
enum ResizeDirection {
    Horizontal,
    Vertical,
}

fn resize_handle(ui: &mut egui::Ui, direction: ResizeDirection) -> egui::Response {
    let size = match direction {
        ResizeDirection::Horizontal => Vec2::new(RESIZER_THICKNESS, ui.available_height()),
        ResizeDirection::Vertical => Vec2::new(ui.available_width(), RESIZER_THICKNESS),
    };
    let (rect, response) = ui.allocate_exact_size(size, Sense::drag());
    let colour = if response.dragged() || response.hovered() {
        ACCENT
    } else {
        Color32::from_rgb(48, 51, 56)
    };
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same(2), colour);
    let centre = rect.center();
    match direction {
        ResizeDirection::Horizontal => painter.line_segment(
            [
                Pos2::new(centre.x, rect.top()),
                Pos2::new(centre.x, rect.bottom()),
            ],
            Stroke::new(1.0, Color32::from_rgb(100, 105, 112)),
        ),
        ResizeDirection::Vertical => painter.line_segment(
            [
                Pos2::new(rect.left(), centre.y),
                Pos2::new(rect.right(), centre.y),
            ],
            Stroke::new(1.0, Color32::from_rgb(100, 105, 112)),
        ),
    };
    response
        .on_hover_cursor(match direction {
            ResizeDirection::Horizontal => egui::CursorIcon::ResizeHorizontal,
            ResizeDirection::Vertical => egui::CursorIcon::ResizeVertical,
        })
        .on_hover_text("Drag to resize")
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

fn draw_histogram(ui: &mut egui::Ui, histogram: &Histogram, label: &str, height: f32) {
    ui.label(egui::RichText::new(label).small());
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), height), Sense::hover());
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

fn draw_scope(ui: &mut egui::Ui, texture: Option<&TextureHandle>, space: ScopeSpace) {
    const RING_SEGMENTS: usize = 180;
    let (rect, _) = ui.allocate_exact_size(ui.available_size(), Sense::hover());
    let side = rect.width().min(rect.height());
    let plot = Rect::from_center_size(rect.center(), Vec2::splat(side));
    let painter = ui.painter_at(rect);
    painter.rect_filled(plot, CornerRadius::ZERO, Color32::from_rgb(3, 4, 5));

    match space {
        ScopeSpace::Cie1931 => {
            for step in 1..8 {
                let fraction = step as f32 / 8.0;
                let colour = Color32::from_rgba_premultiplied(110, 118, 124, 48);
                painter.line_segment(
                    [
                        Pos2::new(plot.left() + plot.width() * fraction, plot.top()),
                        Pos2::new(plot.left() + plot.width() * fraction, plot.bottom()),
                    ],
                    Stroke::new(1.0, colour),
                );
                painter.line_segment(
                    [
                        Pos2::new(plot.left(), plot.top() + plot.height() * fraction),
                        Pos2::new(plot.right(), plot.top() + plot.height() * fraction),
                    ],
                    Stroke::new(1.0, colour),
                );
            }
            let to_screen = |point: [f32; 2]| {
                Pos2::new(
                    plot.left() + point[0] / 0.8 * plot.width(),
                    plot.bottom() - point[1] / 0.9 * plot.height(),
                )
            };
            for (index, segment) in CIE1931_LOCUS.windows(2).enumerate() {
                let hue = index as f32 / (CIE1931_LOCUS.len() - 1) as f32;
                painter.line_segment(
                    [to_screen(segment[0]), to_screen(segment[1])],
                    Stroke::new(1.2, ring_colour(hue).gamma_multiply(0.78)),
                );
            }
            painter.line_segment(
                [
                    to_screen(*CIE1931_LOCUS.last().unwrap_or(&CIE1931_LOCUS[0])),
                    to_screen(CIE1931_LOCUS[0]),
                ],
                Stroke::new(1.2, Color32::from_rgb(220, 145, 215)),
            );
        }
        ScopeSpace::Ryb => {
            let centre = plot.center();
            let radius = side * 0.48;
            for fraction in [0.33_f32, 0.66, 1.0] {
                painter.circle_stroke(
                    centre,
                    radius * fraction,
                    Stroke::new(1.0, Color32::from_gray(70)),
                );
            }
            for index in 0..RING_SEGMENTS {
                let a = index as f32 / RING_SEGMENTS as f32;
                let b = (index + 1) as f32 / RING_SEGMENTS as f32;
                let angle_a = -std::f32::consts::FRAC_PI_2 - std::f32::consts::TAU * a;
                let angle_b = -std::f32::consts::FRAC_PI_2 - std::f32::consts::TAU * b;
                painter.line_segment(
                    [
                        centre + Vec2::angled(angle_a) * radius,
                        centre + Vec2::angled(angle_b) * radius,
                    ],
                    Stroke::new(1.2, ring_colour((a + b) * 0.5).gamma_multiply(0.72)),
                );
            }
        }
    }
    if let Some(texture) = texture {
        painter.image(
            texture.id(),
            plot,
            Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
            Color32::WHITE,
        );
    } else {
        painter.text(
            plot.center(),
            egui::Align2::CENTER_CENTER,
            "Scope updates after the preview",
            egui::FontId::proportional(12.0),
            Color32::from_gray(130),
        );
    }
}

fn split_panel_heights(
    available: f32,
    desired_top: f32,
    minimum_top: f32,
    minimum_bottom: f32,
    handle: f32,
) -> (f32, f32) {
    let usable = (available - handle).max(0.0);
    if usable < minimum_top + minimum_bottom {
        let top = (usable * 0.5).max(0.0);
        return (top, usable - top);
    }
    let top = desired_top.clamp(minimum_top, usable - minimum_bottom);
    (top, usable - top)
}

const fn background_work_needs_repaint(
    loading: bool,
    rendering: bool,
    scoping: bool,
    pending_thumbnails: usize,
) -> bool {
    loading || rendering || scoping || pending_thumbnails > 0
}

fn film_strip_item(ui: &mut egui::Ui, item: &FilmStripItem, selected: bool) -> egui::Response {
    let desired_size = Vec2::new(112.0, 78.0);
    let (rect, response) = ui.allocate_exact_size(desired_size, Sense::click());
    let stroke = if selected {
        Stroke::new(2.0, ACCENT)
    } else {
        Stroke::new(1.0, Color32::from_gray(62))
    };
    ui.painter()
        .rect_filled(rect, CornerRadius::same(3), Color32::from_rgb(24, 25, 27));
    if let (Some(texture), Some(dimensions)) = (&item.thumbnail, item.dimensions) {
        let image_rect = fit_rect(
            rect.shrink(3.0),
            dimensions[0] as f32 / dimensions[1].max(1) as f32,
        );
        ui.painter().image(
            texture.id(),
            image_rect,
            Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
            Color32::WHITE,
        );
    } else {
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            "Loading…",
            egui::FontId::proportional(11.0),
            Color32::from_gray(120),
        );
    }
    ui.painter()
        .rect_stroke(rect, CornerRadius::same(3), stroke, StrokeKind::Inside);
    response.on_hover_text(
        item.path
            .file_name()
            .and_then(|name| name.to_str())
            .map_or_else(|| item.path.display().to_string(), str::to_owned),
    )
}

fn discover_sibling_images(selected: &std::path::Path) -> Vec<PathBuf> {
    let Some(directory) = selected.parent() else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(directory) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_file()
                && path
                    .extension()
                    .and_then(|extension| extension.to_str())
                    .is_some_and(|extension| {
                        matches!(
                            extension.to_ascii_lowercase().as_str(),
                            "png" | "jpg" | "jpeg"
                        )
                    })
        })
        .collect()
}

fn reconcile_film_strip(existing: Vec<FilmStripItem>, paths: Vec<PathBuf>) -> Vec<FilmStripItem> {
    let mut existing = existing
        .into_iter()
        .map(|item| (item.path.clone(), item))
        .collect::<HashMap<_, _>>();
    paths
        .into_iter()
        .map(|path| {
            if let Some(item) = existing.remove(&path) {
                item
            } else {
                FilmStripItem {
                    path,
                    thumbnail: None,
                    dimensions: None,
                    thumbnail_requested: false,
                }
            }
        })
        .collect()
}

fn film_strip_paths_match(current: &[&std::path::Path], discovered: &[PathBuf]) -> bool {
    current
        .iter()
        .copied()
        .eq(discovered.iter().map(PathBuf::as_path))
}

fn thumbnail_needs_request(item: &FilmStripItem) -> bool {
    item.thumbnail.is_none() && item.dimensions.is_none() && !item.thumbnail_requested
}

fn prefetch_range(
    total: usize,
    first_visible: usize,
    last_visible: usize,
) -> std::ops::Range<usize> {
    if total == 0 || first_visible >= total || last_visible < first_visible {
        return 0..0;
    }
    let last_visible = last_visible.min(total - 1);
    let visible_count = last_visible - first_visible + 1;
    let target_count = visible_count.saturating_mul(2).min(total);
    let spare = target_count - visible_count;
    let before = (spare / 2).min(first_visible);
    let start = first_visible - before;
    let mut end = (last_visible + 1 + (spare - before)).min(total);
    let current_count = end - start;
    let adjusted_start = start.saturating_sub(target_count - current_count);
    end = (adjusted_start + target_count).min(total);
    adjusted_start..end
}

fn thumbnail_color_image(thumbnail: &Thumbnail) -> egui::ColorImage {
    egui::ColorImage::from_rgba_unmultiplied(
        [thumbnail.width as usize, thumbnail.height as usize],
        &thumbnail.rgba,
    )
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

/// Estimates display luminance only where the panel-anchored label intersects
/// the fitted image. `None` means the label is entirely over letterboxing.
fn overlay_luminance(
    pixels: &[[f32; 3]],
    dimensions: [u32; 2],
    image_rect: Rect,
    label_rect: Rect,
) -> Option<f32> {
    let width = dimensions[0] as usize;
    let height = dimensions[1] as usize;
    let overlap = image_rect.intersect(label_rect);
    if width == 0
        || height == 0
        || pixels.is_empty()
        || overlap.width() <= 0.0
        || overlap.height() <= 0.0
    {
        return None;
    }
    let to_source_x = |x: f32| {
        ((x - image_rect.left()) / image_rect.width() * width as f32)
            .floor()
            .clamp(0.0, width.saturating_sub(1) as f32) as usize
    };
    let to_source_y = |y: f32| {
        ((y - image_rect.top()) / image_rect.height() * height as f32)
            .floor()
            .clamp(0.0, height.saturating_sub(1) as f32) as usize
    };
    let min_x = to_source_x(overlap.left());
    let min_y = to_source_y(overlap.top());
    let max_x = to_source_x(overlap.right()).max(min_x);
    let max_y = to_source_y(overlap.bottom()).max(min_y);
    let step_x = ((max_x - min_x + 1) / 32).max(1);
    let step_y = ((max_y - min_y + 1) / 32).max(1);
    let mut total = 0.0;
    let mut count = 0_u32;
    for y in (min_y..=max_y).step_by(step_y) {
        for x in (min_x..=max_x).step_by(step_x) {
            let Some(pixel) = pixels.get(y.saturating_mul(width).saturating_add(x)) else {
                continue;
            };
            total += 0.2126 * pixel[0].clamp(0.0, 1.0)
                + 0.7152 * pixel[1].clamp(0.0, 1.0)
                + 0.0722 * pixel[2].clamp(0.0, 1.0);
            count = count.saturating_add(1);
        }
    }
    if count == 0 {
        None
    } else {
        Some(total / count as f32)
    }
}

fn soft_text_shadow(colour: Color32) -> [(Vec2, Color32); 9] {
    let with_alpha =
        |alpha| Color32::from_rgba_unmultiplied(colour.r(), colour.g(), colour.b(), alpha);
    let centre = Vec2::new(1.25, 1.5);
    [
        (centre + Vec2::new(-1.0, -1.0), with_alpha(18)),
        (centre + Vec2::new(0.0, -1.0), with_alpha(30)),
        (centre + Vec2::new(1.0, -1.0), with_alpha(18)),
        (centre + Vec2::new(-1.0, 0.0), with_alpha(30)),
        (centre, with_alpha(88)),
        (centre + Vec2::new(1.0, 0.0), with_alpha(30)),
        (centre + Vec2::new(-1.0, 1.0), with_alpha(18)),
        (centre + Vec2::new(0.0, 1.0), with_alpha(30)),
        (centre + Vec2::new(1.0, 1.0), with_alpha(18)),
    ]
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
    fn overlay_luminance_uses_the_label_area_and_handles_small_images() {
        let image = Rect::from_min_size(Pos2::ZERO, Vec2::splat(100.0));
        let label = Rect::from_min_size(Pos2::new(8.0, 8.0), Vec2::new(40.0, 16.0));
        assert!(overlay_luminance(&[[0.9, 0.9, 0.9]], [1, 1], image, label).unwrap() > 0.4);
        assert!(overlay_luminance(&[[0.1, 0.1, 0.1]], [1, 1], image, label).unwrap() < 0.4);
        // A stale or incomplete decoded buffer must not make the UI panic.
        assert!(overlay_luminance(&[], [0, 0], image, label).is_none());
    }

    #[test]
    fn panel_label_ignores_luminance_when_it_is_in_letterboxing() {
        let wide_image = Rect::from_min_max(Pos2::new(0.0, 40.0), Pos2::new(200.0, 100.0));
        let label = Rect::from_min_size(Pos2::new(8.0, 8.0), Vec2::new(40.0, 16.0));

        assert!(overlay_luminance(&[[0.0; 3]], [1, 1], wide_image, label).is_none());
    }

    #[test]
    fn soft_text_shadow_keeps_the_opposite_colour_and_fades_outward() {
        let samples = soft_text_shadow(Color32::WHITE);
        assert_eq!(
            samples[4].1,
            Color32::from_rgba_unmultiplied(255, 255, 255, 88)
        );
        assert!(samples[0].1.a() < samples[4].1.a());
        assert!(samples.iter().all(|(_, colour)| {
            let [red, green, blue, _alpha] = colour.to_srgba_unmultiplied();
            red == 255 && green == 255 && blue == 255
        }));
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

    #[test]
    fn right_panel_split_always_reserves_the_handle_and_controls() {
        let (histograms, controls) = split_panel_heights(500.0, 460.0, 80.0, 120.0, 10.0);
        assert!((histograms - 370.0).abs() < f32::EPSILON);
        assert!((controls - 120.0).abs() < f32::EPSILON);
        assert!((histograms + controls + 10.0 - 500.0).abs() < f32::EPSILON);

        let (histograms, controls) = split_panel_heights(180.0, 120.0, 80.0, 120.0, 10.0);
        assert!((histograms + controls + 10.0 - 180.0).abs() < f32::EPSILON);
    }

    #[test]
    fn pending_scope_analysis_keeps_ui_polling_after_render_finishes() {
        assert!(background_work_needs_repaint(false, false, true, 0));
        assert!(!background_work_needs_repaint(false, false, false, 0));
    }

    #[test]
    fn pending_thumbnails_keep_ui_polling_after_other_work_finishes() {
        assert!(background_work_needs_repaint(false, false, false, 1));
        assert!(!background_work_needs_repaint(false, false, false, 0));
    }

    #[test]
    fn reconciling_the_same_filmstrip_preserves_cached_previews_without_requests() {
        let paths = vec![PathBuf::from("a.jpg"), PathBuf::from("b.jpg")];
        let existing = paths
            .iter()
            .cloned()
            .map(|path| FilmStripItem {
                path,
                thumbnail: None,
                dimensions: Some([160, 90]),
                thumbnail_requested: true,
            })
            .collect();

        let items = reconcile_film_strip(existing, paths);

        assert_eq!(items.len(), 2);
        assert_eq!(items[0].dimensions, Some([160, 90]));
    }

    #[test]
    fn reconciliation_marks_only_uncached_items_as_needing_thumbnails() {
        let existing = vec![FilmStripItem {
            path: PathBuf::from("cached.jpg"),
            thumbnail: None,
            dimensions: Some([160, 90]),
            thumbnail_requested: true,
        }];
        let paths = vec![PathBuf::from("cached.jpg"), PathBuf::from("new.jpg")];

        let items = reconcile_film_strip(existing, paths);

        assert!(!thumbnail_needs_request(&items[0]));
        assert!(thumbnail_needs_request(&items[1]));
    }

    #[test]
    fn selecting_a_sibling_does_not_rebuild_an_unchanged_filmstrip() {
        let discovered = vec![PathBuf::from("a.jpg"), PathBuf::from("b.jpg")];
        let current = discovered.iter().map(PathBuf::as_path).collect::<Vec<_>>();

        assert!(film_strip_paths_match(&current, &discovered));
    }

    #[test]
    fn filmstrip_prefetches_twice_the_visible_thumbnail_count() {
        assert_eq!(prefetch_range(100, 0, 9), 0..20);
        assert_eq!(prefetch_range(100, 20, 29), 15..35);
        assert_eq!(prefetch_range(100, 90, 99), 80..100);
        assert_eq!(prefetch_range(12, 0, 9), 0..12);
    }

    #[test]
    fn preview_requests_do_not_invalidate_an_in_flight_image_load() {
        let context = egui::Context::default();
        let mut app = FocalEditorApp::new(&context);
        app.latest_load_generation = 4;
        app.latest_generation = 9;

        assert!(app.load_result_is_current(4));
        assert!(!app.load_result_is_current(3));
    }

    #[test]
    fn export_requires_pixels_from_the_current_completed_render() {
        let context = egui::Context::default();
        let mut app = FocalEditorApp::new(&context);
        app.latest_generation = 8;
        app.output = Some(
            Image::new(
                1,
                1,
                vec![[0.5; 3]],
                focal_core::ImageContract::SRGB_DISPLAY,
            )
            .unwrap(),
        );
        app.output_generation = Some(7);
        assert!(!app.can_export());

        app.output_generation = Some(8);
        assert!(app.can_export());

        app.rendering = true;
        assert!(!app.can_export());
    }

    #[test]
    fn loading_main_image_does_not_replace_cached_filmstrip_thumbnail() {
        let context = egui::Context::default();
        let cached = context.load_texture(
            "cached-thumbnail",
            egui::ColorImage::new([1, 1], vec![Color32::WHITE]),
            egui::TextureOptions::LINEAR,
        );
        let cached_id = cached.id();
        let mut app = FocalEditorApp::new(&context);
        app.film_strip.push(FilmStripItem {
            path: PathBuf::from("photo.jpg"),
            thumbnail: Some(cached),
            dimensions: Some([1, 1]),
            thumbnail_requested: true,
        });
        app.install_image(
            PathBuf::from("photo.jpg"),
            DecodedImage {
                width: 1,
                height: 1,
                rgba: vec![128, 128, 128, 255],
                pixels: vec![[128.0 / 255.0; 3]],
                has_transparency: false,
            },
            &context,
        );

        assert_eq!(
            app.film_strip[0].thumbnail.as_ref().unwrap().id(),
            cached_id
        );
    }
}
