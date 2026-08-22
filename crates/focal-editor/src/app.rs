#![allow(
    clippy::assigning_clones,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::fn_params_excessive_bools,
    clippy::struct_excessive_bools,
    clippy::too_many_lines
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
use focal_core::{CancellationToken, ClippingWarnings, CropSettings, Image, PIPELINE_VERSION};
use focal_plot::vectorscope::{
    CIE1931_LOCUS, DensityScale, ScopeSpace, VectorscopeAnalysis, render_trace, ring_colour,
};
use serde::{Deserialize, Serialize};

use crate::{
    image_io::{
        self, DecodedImage, ExportRequest, ExportResult, LoadOperation, LoadRequest, LoadResult,
        Thumbnail, ThumbnailRequest, ThumbnailResult,
    },
    preview::{self, Adjustments, PreviewEvent, PreviewRequest, PreviewSampling, PreviewWorker},
    scope::{self, ScopeRequest, ScopeResult},
};

const EDIT_STATE_VERSION: u32 = 3;
const PREVIEW_BACKGROUND: Color32 = Color32::from_rgb(12, 13, 15);
const PANEL_BACKGROUND: Color32 = Color32::from_rgb(24, 26, 29);
const ACCENT: Color32 = Color32::from_rgb(117, 181, 230);
const LEFT_RAIL_WIDTH: f32 = 190.0;
const RIGHT_RAIL_WIDTH: f32 = 330.0;
const FILMSTRIP_HEIGHT: f32 = 132.0;
const TOOLBAR_HEIGHT: f32 = 38.0;
const PROCESSING_BAR_HEIGHT: f32 = 32.0;
const PREVIEW_MAX_PIXELS: usize = 1_000_000;
const LOUPE_SIZE: f32 = 180.0;
const LOUPE_ZOOM: f32 = 4.0;
const HIGHLIGHT_CLIP_COLOUR: Color32 = Color32::from_rgb(255, 48, 48);
const LOWLIGHT_CLIP_COLOUR: Color32 = Color32::from_rgb(48, 128, 255);
const PROCESSING_LOADING_COLOUR: Color32 = Color32::from_rgb(220, 169, 72);
const PROCESSING_READY_COLOUR: Color32 = Color32::from_rgb(89, 170, 112);
const PROCESSING_RENDERING_COLOUR: Color32 = Color32::from_rgb(117, 181, 230);
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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum EditorTab {
    #[default]
    MainPhoto,
    BeforeAfter,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum CropMode {
    #[default]
    Inactive,
    Editing,
    Applied,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum AspectLock {
    #[default]
    Free,
    Locked,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct PreviewView {
    zoom: f32,
    pan: Vec2,
}

impl Default for PreviewView {
    fn default() -> Self {
        Self {
            zoom: 1.0,
            pan: Vec2::ZERO,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
struct EditSidecar {
    format: String,
    edit_state_version: u32,
    pipeline_version: u32,
    source_path: String,
    exposure_stops: f32,
    contrast: f32,
    warmth: f32,
    tint: f32,
    local_contrast_amount: f32,
    local_contrast_radius: f32,
    saturation: f32,
    noise_luminance: f32,
    noise_colour: f32,
    crop: Option<CropSettings>,
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
    export_sender: Sender<ExportRequest>,
    export_receiver: Receiver<ExportResult>,
    preview_worker: PreviewWorker,
    preview_receiver: Receiver<PreviewEvent>,
    scope_worker: scope::ScopeWorker,
    scope_receiver: Receiver<ScopeResult>,
    next_generation: u64,
    latest_load_generation: u64,
    latest_generation: u64,
    thumbnail_generation: u64,
    pending_thumbnails: usize,
    thumbnail_cancellation: CancellationToken,
    load_cancellation: Option<CancellationToken>,
    loading: bool,
    rendering: bool,
    exporting: bool,
    export_generation: Option<u64>,
    export_cancellation: Option<CancellationToken>,
    scoping: bool,
    render_progress: f32,
    source_path: Option<PathBuf>,
    source: Option<DecodedImage>,
    source_core: Option<Arc<Image>>,
    preview_sampling: PreviewSampling,
    texture_sampling: PreviewSampling,
    output: Option<Image>,
    output_clipping: Option<ClippingWarnings>,
    before_preview: Option<Image>,
    output_generation: Option<u64>,
    source_texture: Option<TextureHandle>,
    output_texture: Option<TextureHandle>,
    pending_transparency: Option<(PathBuf, DecodedImage)>,
    sidecar_path: Option<PathBuf>,
    exposure_stops: f32,
    contrast: f32,
    warmth: f32,
    tint: f32,
    local_contrast_amount: f32,
    local_contrast_radius: f32,
    saturation: f32,
    noise_luminance: f32,
    noise_colour: f32,
    crop: Option<CropSettings>,
    crop_mode: CropMode,
    crop_aspect_x: f32,
    crop_aspect_y: f32,
    crop_aspect_lock: AspectLock,
    source_histogram: Option<Histogram>,
    output_histogram: Option<Histogram>,
    scope_tab: ScopeTab,
    scope_density_scale: DensityScale,
    histogram_density_scale: DensityScale,
    latest_scope_result: Option<ScopeResult>,
    editor_tab: EditorTab,
    preview_view: PreviewView,
    loupe_enabled: bool,
    show_highlight_clipping: bool,
    show_lowlight_clipping: bool,
    white_balance_picker: bool,
    copied_edits: Option<Adjustments>,
    paste_after_load: bool,
    cie_scope_texture: Option<TextureHandle>,
    ryb_scope_texture: Option<TextureHandle>,
    film_strip: Vec<FilmStripItem>,
    left_rail_width: f32,
    right_rail_width: f32,
    filmstrip_height: f32,
    navigator_height: f32,
    histogram_height: f32,
    last_export_directory: Option<PathBuf>,
    status: String,
}

impl FocalEditorApp {
    #[must_use]
    pub fn new(context: &egui::Context) -> Self {
        configure_visuals(context);
        let (load_sender, load_receiver) = image_io::spawn_loader();
        let (thumbnail_sender, thumbnail_receiver) = image_io::spawn_thumbnail_loader();
        let (export_sender, export_receiver) = image_io::spawn_exporter();
        let (preview_worker, preview_receiver) = preview::spawn();
        let (scope_worker, scope_receiver) = scope::spawn();
        let mut app = Self {
            load_sender,
            load_receiver,
            thumbnail_sender,
            thumbnail_receiver,
            export_sender,
            export_receiver,
            preview_worker,
            preview_receiver,
            scope_worker,
            scope_receiver,
            next_generation: 0,
            latest_load_generation: 0,
            latest_generation: 0,
            thumbnail_generation: 0,
            pending_thumbnails: 0,
            thumbnail_cancellation: CancellationToken::new(),
            load_cancellation: None,
            loading: false,
            rendering: false,
            exporting: false,
            export_generation: None,
            export_cancellation: None,
            scoping: false,
            render_progress: 0.0,
            source_path: None,
            source: None,
            source_core: None,
            preview_sampling: PreviewSampling::full(1, 1),
            texture_sampling: PreviewSampling::full(1, 1),
            output: None,
            output_clipping: None,
            before_preview: None,
            output_generation: None,
            source_texture: None,
            output_texture: None,
            pending_transparency: None,
            sidecar_path: None,
            exposure_stops: 0.0,
            contrast: 0.0,
            warmth: 0.0,
            tint: 0.0,
            local_contrast_amount: 0.0,
            local_contrast_radius: 80.0,
            saturation: 0.0,
            noise_luminance: 0.0,
            noise_colour: 0.0,
            crop: None,
            crop_mode: CropMode::Inactive,
            crop_aspect_x: 3.0,
            crop_aspect_y: 2.0,
            crop_aspect_lock: AspectLock::Free,
            source_histogram: None,
            output_histogram: None,
            scope_tab: ScopeTab::default(),
            scope_density_scale: DensityScale::Logarithmic,
            histogram_density_scale: DensityScale::Linear,
            latest_scope_result: None,
            editor_tab: EditorTab::default(),
            preview_view: PreviewView::default(),
            loupe_enabled: false,
            show_highlight_clipping: false,
            show_lowlight_clipping: false,
            white_balance_picker: false,
            copied_edits: None,
            paste_after_load: false,
            cie_scope_texture: None,
            ryb_scope_texture: None,
            film_strip: Vec::new(),
            left_rail_width: LEFT_RAIL_WIDTH,
            right_rail_width: RIGHT_RAIL_WIDTH,
            filmstrip_height: FILMSTRIP_HEIGHT,
            navigator_height: 170.0,
            histogram_height: 185.0,
            last_export_directory: None,
            status: "Ready — open a PNG, JPEG, TIFF, or X-T5 RAF to begin".to_owned(),
        };

        if let Some(path) = std::env::args_os()
            .nth(1)
            .map(PathBuf::from)
            .filter(|path| is_supported_image_path(path))
        {
            app.open_path(path);
        }
        app
    }

    fn open_path(&mut self, path: PathBuf) {
        self.preview_worker.cancel();
        self.scope_worker.cancel();
        self.invalidate_load();
        self.invalidate_export();
        self.paste_after_load = false;
        self.next_generation = self.next_generation.saturating_add(1);
        self.latest_load_generation = self.next_generation;
        // Invalidate every result belonging to the previously selected image.
        // A cancelled worker can still have a completion event in flight.
        self.latest_generation = self.next_generation;
        self.prepare_film_strip(&path);
        self.loading = true;
        self.rendering = false;
        self.output_generation = None;
        self.output = None;
        self.output_clipping = None;
        self.before_preview = None;
        self.pending_transparency = None;
        self.status = format!("Opening {}…", path.display());
        if self
            .load_sender
            .send(LoadRequest {
                generation: self.latest_load_generation,
                operation: LoadOperation::Decode(path),
                cancellation: {
                    let cancellation = CancellationToken::new();
                    self.load_cancellation = Some(cancellation.clone());
                    cancellation
                },
            })
            .is_err()
        {
            self.invalidate_load();
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

        self.thumbnail_cancellation.cancel();
        self.thumbnail_cancellation = CancellationToken::new();
        self.thumbnail_generation = self.latest_load_generation;
        self.pending_thumbnails = 0;
        self.film_strip = reconcile_film_strip(std::mem::take(&mut self.film_strip), paths);
    }

    fn open_dialog(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("Fujifilm RAF", &["raf", "RAF"])
            .add_filter("Rendered images", &["png", "jpg", "jpeg", "tif", "tiff"])
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
            self.load_cancellation = None;
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
                mark_thumbnail_failed(&mut self.film_strip, &result.path);
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
                        Ok(frame) => {
                            self.texture_sampling = frame.sampling;
                            let clipping = frame.clipping;
                            let image = frame.after;
                            self.source_texture = Some(context.load_texture(
                                format!("focal-editor-before-{generation}"),
                                rgba_image(
                                    frame.before.pixels(),
                                    frame.before.width(),
                                    frame.before.height(),
                                ),
                                preview_texture_options(&frame.before, self.source.as_ref()),
                            ));
                            self.before_preview = Some(frame.before);
                            self.source_histogram = Some(Histogram::from_pixels(
                                self.before_preview
                                    .as_ref()
                                    .expect("the before preview was just installed")
                                    .pixels(),
                            ));
                            self.output_histogram = Some(Histogram::from_pixels(image.pixels()));
                            self.scoping = self
                                .scope_worker
                                .submit(ScopeRequest {
                                    generation,
                                    image: image.clone(),
                                })
                                .is_ok();
                            self.output = Some(image);
                            self.output_clipping = clipping;
                            self.output_generation = Some(generation);
                            self.rebuild_output_texture(context, generation);
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
            self.install_scope_textures(context, &result);
            self.latest_scope_result = Some(result);
            self.scoping = false;
        }

        while let Ok(result) = self.export_receiver.try_recv() {
            if !self.export_result_is_current(result.generation) {
                continue;
            }
            self.exporting = false;
            self.export_generation = None;
            self.export_cancellation = None;
            match result.result {
                Ok(()) => {
                    self.last_export_directory =
                        result.path.parent().map(std::path::Path::to_path_buf);
                    self.status = format!("Exported {}", result.path.display());
                }
                Err(error) => {
                    self.status = format!("Could not export {}: {error}", result.path.display());
                }
            }
        }

        if background_work_needs_repaint(
            self.loading,
            self.rendering,
            self.scoping,
            self.exporting,
            self.pending_thumbnails,
        ) {
            context.request_repaint_after(std::time::Duration::from_millis(30));
        }
    }

    fn install_scope_textures(&mut self, context: &egui::Context, result: &ScopeResult) {
        let cie1931 = plot_analysis(&result.cie1931);
        let ryb = plot_analysis(&result.ryb);
        self.cie_scope_texture = Some(
            context.load_texture(
                format!("focal-editor-cie-scope-{}", result.generation),
                render_trace(&cie1931, 1.0, 0.55, DensityScale::Linear, false)
                    .expect("editor-owned CIE scope analysis is structurally valid"),
                egui::TextureOptions::LINEAR,
            ),
        );
        self.ryb_scope_texture = Some(
            context.load_texture(
                format!("focal-editor-ryb-scope-{}", result.generation),
                render_trace(&ryb, 1.0, 0.55, self.scope_density_scale, false)
                    .expect("editor-owned RYB scope analysis is structurally valid"),
                egui::TextureOptions::LINEAR,
            ),
        );
    }

    fn rebuild_output_texture(&mut self, context: &egui::Context, generation: u64) {
        let Some(image) = self.output.as_ref() else {
            self.output_texture = None;
            return;
        };
        self.output_texture = Some(context.load_texture(
            format!(
                "focal-editor-after-{generation}-{}-{}",
                self.show_highlight_clipping, self.show_lowlight_clipping
            ),
            rgba_image_with_clipping(
                image.pixels(),
                image.width(),
                image.height(),
                self.output_clipping.as_ref(),
                self.show_highlight_clipping,
                self.show_lowlight_clipping,
            ),
            preview_texture_options(image, self.source.as_ref()),
        ));
    }

    fn load_result_is_current(&self, generation: u64) -> bool {
        generation == self.latest_load_generation
    }

    fn install_image(&mut self, path: PathBuf, image: DecodedImage, context: &egui::Context) {
        let display_path = path.display().to_string();
        let paste_after_load = self.paste_after_load;
        self.paste_after_load = false;
        self.source_path = Some(path);
        self.sidecar_path = None;
        self.source_histogram = None;
        self.output_histogram = None;
        self.latest_scope_result = None;
        self.cie_scope_texture = None;
        self.ryb_scope_texture = None;
        self.scoping = false;
        self.source_texture = None;
        self.output_texture = None;
        self.output_clipping = None;
        self.before_preview = None;
        self.source_core = image.to_core_image().ok().map(Arc::new);
        let [preview_width, preview_height] =
            bounded_preview_dimensions(image.width, image.height, PREVIEW_MAX_PIXELS);
        self.preview_sampling = PreviewSampling::full(preview_width, preview_height);
        self.texture_sampling = self.preview_sampling;
        self.source = Some(image);
        self.output = None;
        self.output_generation = None;
        self.exposure_stops = 0.0;
        self.contrast = 0.0;
        self.warmth = 0.0;
        self.tint = 0.0;
        self.local_contrast_amount = 0.0;
        self.local_contrast_radius = 80.0;
        self.saturation = 0.0;
        self.noise_luminance = 0.0;
        self.noise_colour = 0.0;
        self.crop = None;
        self.crop_mode = CropMode::Inactive;
        self.crop_aspect_lock = AspectLock::Free;
        self.preview_view = PreviewView::default();
        if paste_after_load && let Some(adjustments) = self.copied_edits {
            self.set_adjustments(adjustments);
            self.status = format!("Loaded {display_path}; pasted edits");
        } else {
            self.status = format!("Loaded {display_path}");
        }
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
                    self.loading = true;
                    self.status = "Flattening transparency onto black…".to_owned();
                    if self
                        .load_sender
                        .send(LoadRequest {
                            generation: self.latest_load_generation,
                            operation: LoadOperation::FlattenOntoBlack { path, image },
                            cancellation: {
                                let cancellation = CancellationToken::new();
                                self.load_cancellation = Some(cancellation.clone());
                                cancellation
                            },
                        })
                        .is_err()
                    {
                        self.invalidate_load();
                        self.status = "The image loader is unavailable".to_owned();
                    }
                }
            } else {
                self.status = "Open cancelled; the source was not modified".to_owned();
            }
        }
    }

    fn request_preview(&mut self, context: &egui::Context) {
        self.invalidate_export();
        let Some(source) = self.preview_render_source() else {
            return;
        };
        let source = Arc::clone(source);
        self.next_generation = self.next_generation.saturating_add(1);
        self.latest_generation = self.next_generation;
        self.rendering = true;
        self.output_generation = None;
        self.scoping = false;
        self.render_progress = 0.0;
        self.status = "Rendering preview…".to_owned();
        let request = PreviewRequest {
            generation: self.latest_generation,
            source,
            sampling: self.preview_sampling,
            snapshot: preview::snapshot_with_adjustments(self.adjustments()),
        };
        if self.preview_worker.submit(request).is_err() {
            self.rendering = false;
            self.status = "The preview worker is unavailable".to_owned();
        }
        context.request_repaint();
    }

    fn reset_preview_sampling_to_full(&mut self) {
        if let Some(source) = self.source.as_ref() {
            let dimensions = preview_content_dimensions(
                [source.width, source.height],
                self.crop_mode,
                self.crop,
            );
            let [width, height] =
                bounded_preview_dimensions(dimensions[0], dimensions[1], PREVIEW_MAX_PIXELS);
            self.preview_sampling = PreviewSampling::full(width, height);
        }
    }

    fn preview_render_source(&self) -> Option<&Arc<Image>> {
        self.source_core.as_ref()
    }

    fn full_resolution_export_source(&self) -> Option<&Arc<Image>> {
        self.source_core.as_ref()
    }

    fn adjustments(&self) -> Adjustments {
        Adjustments {
            warmth: self.warmth,
            tint: self.tint,
            exposure_stops: self.exposure_stops,
            contrast: self.contrast,
            local_contrast_amount: self.local_contrast_amount,
            local_contrast_radius: self.local_contrast_radius,
            saturation: self.saturation,
            noise_luminance: self.noise_luminance,
            noise_colour: self.noise_colour,
            crop: (self.crop_mode != CropMode::Editing)
                .then_some(self.crop)
                .flatten(),
        }
    }

    fn copy_edits(&mut self) {
        if self.source.is_none() {
            self.status = "Open an image before copying edits".to_owned();
            return;
        }
        self.copied_edits = Some(self.adjustments());
        self.status = "Edits copied for this session".to_owned();
    }

    fn set_adjustments(&mut self, adjustments: Adjustments) {
        self.warmth = adjustments.warmth;
        self.tint = adjustments.tint;
        self.exposure_stops = adjustments.exposure_stops;
        self.contrast = adjustments.contrast;
        self.local_contrast_amount = adjustments.local_contrast_amount;
        self.local_contrast_radius = adjustments.local_contrast_radius;
        self.saturation = adjustments.saturation;
        self.noise_luminance = adjustments.noise_luminance;
        self.noise_colour = adjustments.noise_colour;
        self.crop = adjustments.crop.map(|crop| {
            self.source.as_ref().map_or(crop, |source| {
                crop.shrink_to_safe(source.width as f32 / source.height.max(1) as f32)
            })
        });
        self.crop_mode = if self.crop.is_some() {
            CropMode::Applied
        } else {
            CropMode::Inactive
        };
    }

    fn paste_edits(&mut self, context: &egui::Context) {
        let Some(adjustments) = self.copied_edits else {
            self.status = "Copy edits from an image before pasting".to_owned();
            return;
        };
        if self.source.is_none() {
            self.status = "Open an image before pasting edits".to_owned();
            return;
        }
        self.set_adjustments(adjustments);
        self.reset_preview_sampling_to_full();
        self.request_preview(context);
    }

    fn apply_white_balance_sample(&mut self, sample: [f32; 3], context: &egui::Context) {
        let Some((warmth, tint)) = white_balance_from_sample(sample) else {
            self.status = "The picked pixel is too dark to determine a white balance".to_owned();
            return;
        };
        self.warmth = warmth;
        self.tint = tint;
        self.white_balance_picker = false;
        self.status = "White balance picked".to_owned();
        self.request_preview(context);
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
            warmth: self.warmth,
            tint: self.tint,
            local_contrast_amount: self.local_contrast_amount,
            local_contrast_radius: self.local_contrast_radius,
            saturation: self.saturation,
            noise_luminance: self.noise_luminance,
            noise_colour: self.noise_colour,
            crop: self.crop,
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
        let (Some(source_path), Some(_)) = (self.source_path.as_ref(), self.output.as_ref()) else {
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
        self.export_to_path(&path);
    }

    fn export_beside_last(&mut self) {
        let Some(directory) = self.last_export_directory.as_ref() else {
            return;
        };
        let Some(source_path) = self.source_path.as_ref() else {
            return;
        };
        self.export_to_path(&default_export_path(directory, source_path));
    }

    fn export_to_path(&mut self, path: &std::path::Path) {
        if !self.can_export() {
            self.status = "Wait for the current render before exporting".to_owned();
            return;
        }
        let Some(source) = self.full_resolution_export_source() else {
            self.status = "Open an image before exporting".to_owned();
            return;
        };
        let source = (**source).clone();
        self.status = "Rendering full-resolution export…".to_owned();
        self.invalidate_export();
        let cancellation = CancellationToken::new();
        self.exporting = true;
        self.export_generation = Some(self.latest_generation);
        self.export_cancellation = Some(cancellation.clone());
        if self
            .export_sender
            .send(ExportRequest {
                generation: self.latest_generation,
                path: path.to_path_buf(),
                source,
                snapshot: preview::snapshot_with_adjustments(self.adjustments()),
                cancellation,
            })
            .is_err()
        {
            self.invalidate_export();
            self.status = "The export worker is unavailable".to_owned();
        }
    }

    fn invalidate_export(&mut self) {
        if let Some(cancellation) = self.export_cancellation.take() {
            cancellation.cancel();
        }
        self.export_generation = None;
        self.exporting = false;
    }

    fn invalidate_load(&mut self) {
        if let Some(cancellation) = self.load_cancellation.take() {
            cancellation.cancel();
        }
        self.loading = false;
    }

    fn export_result_is_current(&self, generation: u64) -> bool {
        self.export_generation == Some(generation)
    }

    fn can_export(&self) -> bool {
        !self.loading
            && !self.rendering
            && !self.exporting
            && self.output.is_some()
            && self.output_generation == Some(self.latest_generation)
    }
}

impl eframe::App for FocalEditorApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let context = ui.ctx().clone();
        self.poll_background_work(&context);
        self.confirm_transparency(&context);
        let (toggle_loupe, escape_pressed) = context.input(|input| {
            (
                input.key_pressed(egui::Key::L),
                input.key_pressed(egui::Key::Escape),
            )
        });
        if toggle_loupe {
            self.loupe_enabled = !self.loupe_enabled;
        }
        if escape_pressed {
            self.loupe_enabled = false;
            self.white_balance_picker = false;
        }
        if self.crop_mode == CropMode::Editing
            && context.input(|input| input.key_pressed(egui::Key::Enter))
        {
            self.crop_mode = CropMode::Applied;
            self.reset_preview_sampling_to_full();
            self.request_preview(&context);
        }

        let (toolbar_rect, _) = ui.allocate_exact_size(
            Vec2::new(ui.available_width(), TOOLBAR_HEIGHT),
            Sense::hover(),
        );
        ui.painter()
            .rect_filled(toolbar_rect, CornerRadius::ZERO, PANEL_BACKGROUND);
        let mut toolbar_ui = ui.new_child(egui::UiBuilder::new().max_rect(toolbar_rect));
        toolbar_ui.set_clip_rect(toolbar_rect);
        self.show_toolbar(&mut toolbar_ui, toolbar_rect);
        let filmstrip_height = self
            .filmstrip_height
            .clamp(90.0, (ui.available_height() - 160.0).max(90.0));
        let content_height =
            (ui.available_height() - filmstrip_height - RESIZER_THICKNESS).max(100.0);
        let picked_white_balance = ui
            .allocate_ui_with_layout(
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
                        centre_panel_width(ui.available_width(), right_width, RESIZER_THICKNESS);
                    let picked = ui
                        .allocate_ui_with_layout(
                            Vec2::new(centre_width, ui.available_height()),
                            egui::Layout::top_down(egui::Align::Min),
                            |ui| self.show_main_panel(ui, &context),
                        )
                        .inner;
                    let right_handle = resize_handle(ui, ResizeDirection::Horizontal);
                    if right_handle.dragged() {
                        self.right_rail_width = (self.right_rail_width
                            - right_handle.drag_delta().x)
                            .clamp(240.0, 560.0);
                    }
                    ui.allocate_ui_with_layout(
                        Vec2::new(right_width, ui.available_height()),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| self.show_right_panel(ui, &context),
                    );
                    picked
                },
            )
            .inner;
        if let Some(sample) = picked_white_balance {
            self.apply_white_balance_sample(sample, &context);
        }
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
    fn show_toolbar(&mut self, ui: &mut egui::Ui, bounds: Rect) {
        let [left_rect, tab_rect, right_rect] = toolbar_regions(bounds);

        let mut left = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(left_rect)
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        left.set_clip_rect(left_rect);
        if left.button("Open image…").clicked() {
            self.open_dialog();
        }
        let has_image = self.source.is_some();
        if left
            .add_enabled(has_image, egui::Button::new("Save"))
            .on_hover_text("Save editable parameters as a JSON sidecar")
            .clicked()
        {
            self.save_sidecar();
        }
        if left
            .add_enabled(self.can_export(), egui::Button::new("Export"))
            .on_hover_text("Render the current preview to an 8-bit sRGB PNG")
            .clicked()
        {
            self.export_png();
        }
        if left
            .add_enabled(
                self.can_export() && self.last_export_directory.is_some(),
                egui::Button::new("Export again"),
            )
            .on_hover_text("Export to the folder used by the previous export this session")
            .clicked()
        {
            self.export_beside_last();
        }
        left.add_space(12.0);
        if self.loupe_enabled {
            left.label(egui::RichText::new("Loupe: L").small().weak());
        }

        ui.painter().rect_filled(
            tab_rect.shrink2(Vec2::new(2.0, 4.0)),
            CornerRadius::same(4),
            PANEL_BACKGROUND,
        );
        let mut tabs = ui.new_child(egui::UiBuilder::new().max_rect(tab_rect).layout(
            egui::Layout::left_to_right(egui::Align::Center).with_main_align(egui::Align::Center),
        ));
        tabs.set_clip_rect(tab_rect);
        tabs.selectable_value(&mut self.editor_tab, EditorTab::MainPhoto, "Main");
        tabs.selectable_value(
            &mut self.editor_tab,
            EditorTab::BeforeAfter,
            "Before and After",
        );

        let mut right = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(right_rect)
                .layout(egui::Layout::right_to_left(egui::Align::Center)),
        );
        right.set_clip_rect(right_rect);
        right.label(egui::RichText::new("FOCALPLANE").strong());
        right.add_space(12.0);
        right.label(egui::RichText::new(&self.status).small().weak());
    }

    fn show_left_panel(&mut self, ui: &mut egui::Ui, context: &egui::Context) {
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
            self.warmth = 0.0;
            self.tint = 0.0;
            self.local_contrast_amount = 0.0;
            self.local_contrast_radius = 80.0;
            self.saturation = 0.0;
            self.noise_luminance = 0.0;
            self.noise_colour = 0.0;
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
            if self.scope_tab == ScopeTab::Histogram {
                ui.selectable_value(
                    &mut self.histogram_density_scale,
                    DensityScale::Linear,
                    "Linear",
                );
                ui.selectable_value(
                    &mut self.histogram_density_scale,
                    DensityScale::Logarithmic,
                    "Log",
                );
            }
            if self.scope_tab == ScopeTab::Ryb {
                let previous = self.scope_density_scale;
                ui.selectable_value(
                    &mut self.scope_density_scale,
                    DensityScale::Linear,
                    "Linear",
                );
                ui.selectable_value(
                    &mut self.scope_density_scale,
                    DensityScale::Logarithmic,
                    "Log",
                );
                if previous != self.scope_density_scale
                    && let Some(result) = self.latest_scope_result.clone()
                {
                    self.install_scope_textures(context, &result);
                }
            }
        });
        ui.label(
            egui::RichText::new(match self.scope_tab {
                ScopeTab::Histogram => "Display-encoded RGB",
                ScopeTab::Cie1931 | ScopeTab::Ryb => {
                    "Display-encoded sRGB · FocalCore analysis / FocalPlot presentation"
                }
            })
            .small()
            .weak(),
        );
        let available = ui.available_size();
        let processing_height = available.y.min(PROCESSING_BAR_HEIGHT);
        let scope_controls_height = (available.y - processing_height).max(0.0);
        let (histogram_height, controls_height) = split_panel_heights(
            scope_controls_height,
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
                    draw_histogram_pair(
                        ui,
                        self.source_histogram.as_ref(),
                        self.output_histogram.as_ref(),
                        histogram_height,
                        self.histogram_density_scale,
                    );
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
        show_processing_bar(
            ui,
            processing_height,
            self.loading,
            self.rendering,
            self.exporting,
            self.render_progress,
        );
    }

    #[allow(clippy::too_many_lines)]
    fn show_crop_controls(&mut self, ui: &mut egui::Ui, context: &egui::Context) {
        ui.label(egui::RichText::new("Crop and straighten").strong());
        ui.horizontal(|ui| {
            let label = match self.crop_mode {
                CropMode::Inactive => "Crop",
                CropMode::Editing => "Apply crop",
                CropMode::Applied => "Edit crop",
            };
            if ui.button(label).clicked() {
                match self.crop_mode {
                    CropMode::Inactive => {
                        self.crop = Some(CropSettings::full_image());
                        self.crop_mode = CropMode::Editing;
                        self.editor_tab = EditorTab::MainPhoto;
                    }
                    CropMode::Editing => {
                        self.crop_mode = CropMode::Applied;
                        self.reset_preview_sampling_to_full();
                        self.request_preview(context);
                    }
                    CropMode::Applied => {
                        self.crop_mode = CropMode::Editing;
                        self.request_preview(context);
                    }
                }
            }
            if ui
                .add_enabled(self.crop.is_some(), egui::Button::new("Reset"))
                .clicked()
            {
                self.crop = None;
                self.crop_mode = CropMode::Inactive;
                self.request_preview(context);
            }
        });
        if self.crop_mode == CropMode::Editing {
            if self.crop_aspect_lock == AspectLock::Free
                && let (Some(crop), Some(source)) = (self.crop, self.source.as_ref())
            {
                self.crop_aspect_x = (crop.right - crop.left) * source.width as f32
                    / ((crop.bottom - crop.top) * source.height.max(1) as f32);
                self.crop_aspect_y = 1.0;
            }
            let mut rotation = self.crop.map_or(0.0, |crop| crop.rotation_degrees);
            if ui
                .add(egui::Slider::new(&mut rotation, -45.0..=45.0).text("Straighten °"))
                .changed()
            {
                let aspect = self
                    .source
                    .as_ref()
                    .map_or(1.0, |image| image.width as f32 / image.height.max(1) as f32);
                if let Some(crop) = self.crop.as_mut() {
                    crop.rotation_degrees = rotation;
                    *crop = crop.shrink_to_safe(aspect);
                }
            }
            ui.label(
                egui::RichText::new(
                    "The crop is automatically kept inside the original image while straightening.",
                )
                .small()
                .weak(),
            );
            let mut ratio_changed = false;
            let original_ratio = self.source.as_ref().map_or([3.0, 2.0], |source| {
                [source.width as f32, source.height as f32]
            });
            egui::ComboBox::from_label("Aspect ratio")
                .selected_text(if self.crop_aspect_lock == AspectLock::Locked {
                    format!("{} : {}", self.crop_aspect_x, self.crop_aspect_y)
                } else {
                    "Free".to_owned()
                })
                .show_ui(ui, |ui| {
                    if ui
                        .selectable_label(self.crop_aspect_lock == AspectLock::Free, "Free")
                        .clicked()
                    {
                        self.crop_aspect_lock = AspectLock::Free;
                    }
                    for (label, x, y) in [
                        ("Original", original_ratio[0], original_ratio[1]),
                        ("1 : 1", 1.0, 1.0),
                        ("3 : 2", 3.0, 2.0),
                        ("4 : 3", 4.0, 3.0),
                        ("16 : 9", 16.0, 9.0),
                    ] {
                        if ui.selectable_label(false, label).clicked() {
                            self.crop_aspect_x = x;
                            self.crop_aspect_y = y;
                            self.crop_aspect_lock = AspectLock::Locked;
                            ratio_changed = true;
                        }
                    }
                });
            ui.horizontal(|ui| {
                ratio_changed |= ui
                    .add(egui::DragValue::new(&mut self.crop_aspect_x).range(0.1..=100.0))
                    .changed();
                if ui
                    .selectable_label(self.crop_aspect_lock == AspectLock::Locked, "🔗")
                    .on_hover_text("Lock or unlock the crop aspect ratio")
                    .clicked()
                {
                    self.crop_aspect_lock = match self.crop_aspect_lock {
                        AspectLock::Free => AspectLock::Locked,
                        AspectLock::Locked => AspectLock::Free,
                    };
                    ratio_changed = true;
                }
                ratio_changed |= ui
                    .add(egui::DragValue::new(&mut self.crop_aspect_y).range(0.1..=100.0))
                    .changed();
            });
            if ratio_changed
                && self.crop_aspect_lock == AspectLock::Locked
                && let (Some(crop), Some(source)) = (self.crop.as_mut(), self.source.as_ref())
            {
                constrain_crop_aspect(
                    crop,
                    source.width as f32 / source.height.max(1) as f32,
                    self.crop_aspect_x / self.crop_aspect_y,
                );
            }
        }
    }

    fn show_controls(&mut self, ui: &mut egui::Ui, context: &egui::Context) {
        let input_label = self.source.as_ref().map_or("No image", |source| {
            if source.input_contract == focal_core::ImageContract::ADOBE_RGB_CURVE {
                "ICC-managed input → Adobe RGB working input"
            } else {
                "Unprofiled input interpreted as sRGB"
            }
        });
        ui.label(egui::RichText::new(input_label).small().weak());
        ui.add_space(8.0);
        self.show_crop_controls(ui, context);
        ui.add_space(8.0);
        ui.label(egui::RichText::new("Display aids").strong());
        let mut clipping_changed = false;
        clipping_changed |= ui
            .checkbox(&mut self.show_highlight_clipping, "Highlight clipping")
            .on_hover_text("Mark pixels with one or more channels at the display maximum")
            .changed();
        clipping_changed |= ui
            .checkbox(&mut self.show_lowlight_clipping, "Lowlight clipping")
            .on_hover_text("Mark pixels whose display lightness reaches black")
            .changed();
        if clipping_changed && let Some(generation) = self.output_generation {
            self.rebuild_output_texture(context, generation);
        }
        ui.add_space(8.0);
        ui.label(egui::RichText::new("White balance").strong());
        if white_balance_picker_button(ui, self.white_balance_picker)
            .on_hover_text("Sample white balance from a neutral area of the photo")
            .clicked()
        {
            self.white_balance_picker = !self.white_balance_picker;
        }
        let warmth_changed = parameter_row(
            ui,
            "Warmth",
            &mut self.warmth,
            -100.0..=100.0,
            0.0,
            0.1,
            "Decoded-image blue-to-amber balance; not a Kelvin temperature",
        );
        let tint_changed = parameter_row(
            ui,
            "Tint",
            &mut self.tint,
            -100.0..=100.0,
            0.0,
            0.1,
            "Decoded-image green-to-magenta balance",
        );
        ui.add_space(8.0);
        ui.label(egui::RichText::new("Tone").strong());
        let exposure_changed = parameter_row(
            ui,
            "Exposure",
            &mut self.exposure_stops,
            -8.0..=8.0,
            0.0,
            0.01,
            "Stops of exposure compensation",
        );
        let contrast_changed = parameter_row(
            ui,
            "Contrast",
            &mut self.contrast,
            -100.0..=100.0,
            0.0,
            0.1,
            "Temporary FocalCore contrast control",
        );
        let mut local_contrast_changed = false;
        let mut local_radius_changed = false;
        egui::CollapsingHeader::new("Local Contrast")
            .default_open(true)
            .show(ui, |ui| {
                local_contrast_changed = parameter_row(
                    ui,
                    "amount",
                    &mut self.local_contrast_amount,
                    -100.0..=100.0,
                    0.0,
                    0.1,
                    "Strength of lightness detail around the selected radius",
                );
                local_radius_changed = parameter_row(
                    ui,
                    "radius",
                    &mut self.local_contrast_radius,
                    1.0..=256.0,
                    80.0,
                    1.0,
                    "Lightness-detail radius in preview pixels",
                );
            });
        let saturation_changed = parameter_row(
            ui,
            "Saturation",
            &mut self.saturation,
            -100.0..=100.0,
            0.0,
            0.1,
            "HSV saturation with highlight and highly saturated colour protection",
        );
        ui.add_space(8.0);
        ui.label(egui::RichText::new("Noise reduction").strong());
        let noise_luminance_changed = parameter_row(
            ui,
            "Luminance",
            &mut self.noise_luminance,
            0.0..=100.0,
            0.0,
            0.1,
            "Edge-aware smoothing of decoded-image brightness noise",
        );
        let noise_colour_changed = parameter_row(
            ui,
            "Colour",
            &mut self.noise_colour,
            0.0..=100.0,
            0.0,
            0.1,
            "Edge-aware smoothing of decoded-image colour noise",
        );
        if warmth_changed
            || tint_changed
            || exposure_changed
            || contrast_changed
            || local_contrast_changed
            || local_radius_changed
            || saturation_changed
            || noise_luminance_changed
            || noise_colour_changed
        {
            self.request_preview(context);
        }
    }

    fn show_film_strip(&mut self, ui: &mut egui::Ui, _context: &egui::Context) {
        ui.horizontal(|ui| {
            if !self.film_strip.is_empty() {
                ui.label(egui::RichText::new(format!("{} photos", self.film_strip.len())).weak());
            }
        });
        let mut selected = None;
        let mut copy_requested = false;
        let mut paste_target = None;
        let has_image = self.source.is_some();
        let has_copied_edits = self.copied_edits.is_some();
        let mut visible_indices = Vec::new();
        egui::ScrollArea::horizontal()
            .id_salt("focal-editor-film-strip")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    for index in 0..self.film_strip.len() {
                        let is_selected = self
                            .source_path
                            .as_ref()
                            .is_some_and(|path| path == &self.film_strip[index].path);
                        let (response, path) = {
                            let item = &mut self.film_strip[index];
                            let response = film_strip_item(ui, item, is_selected);
                            (response, item.path.clone())
                        };
                        if response.rect.intersects(ui.clip_rect()) {
                            visible_indices.push(index);
                        }
                        if response.clicked() {
                            selected = Some(path.clone());
                        }
                        response.context_menu(|ui| {
                            if ui
                                .add_enabled(has_image, egui::Button::new("Copy edits"))
                                .clicked()
                            {
                                copy_requested = true;
                                ui.close();
                            }
                            if ui
                                .add_enabled(
                                    has_image && has_copied_edits,
                                    egui::Button::new("Paste edits"),
                                )
                                .clicked()
                            {
                                paste_target = Some(path.clone());
                                ui.close();
                            }
                        });
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
                    path: path.clone(),
                    cancellation: self.thumbnail_cancellation.clone(),
                })
                .is_ok()
            {
                self.pending_thumbnails = self.pending_thumbnails.saturating_add(1);
            } else if let Some(item) = self.film_strip.iter_mut().find(|item| item.path == path) {
                item.thumbnail_requested = false;
            }
        }
        if copy_requested {
            self.copy_edits();
        }
        if let Some(path) = paste_target {
            self.paste_to_path(path, ui.ctx());
        }
        if let Some(path) = selected
            && self.source_path.as_ref() != Some(&path)
        {
            self.open_path(path);
        }
    }

    fn paste_to_path(&mut self, path: PathBuf, context: &egui::Context) {
        if self.source_path.as_ref() == Some(&path) {
            self.paste_edits(context);
        } else {
            self.open_path(path);
            self.paste_after_load = true;
        }
    }

    fn show_main_panel(&mut self, ui: &mut egui::Ui, context: &egui::Context) -> Option<[f32; 3]> {
        // The two previews intentionally consume the entire centre pane. Any
        // letterboxing is caused only by preserving the source aspect ratio;
        // there is no additional card padding, label row, or decorative
        // border to steal pixels from the image.
        let available = ui.available_size();
        let sampling_size = if self.editor_tab == EditorTab::BeforeAfter {
            Vec2::new((available.x * 0.5).floor(), available.y)
        } else {
            available
        };
        if let Some(source) = self.source.as_ref() {
            let dimensions = preview_content_dimensions(
                [source.width, source.height],
                self.crop_mode,
                self.crop,
            );
            let desired = preview_sampling_for_view(
                Rect::from_min_size(Pos2::ZERO, sampling_size),
                dimensions,
                self.preview_view,
                context.pixels_per_point(),
                PREVIEW_MAX_PIXELS,
            );
            if desired != self.preview_sampling {
                self.preview_sampling = desired;
                self.request_preview(context);
            }
        }
        if self.editor_tab == EditorTab::MainPhoto {
            let preview_aspect = self.source.as_ref().map(|source| {
                let dimensions = preview_content_dimensions(
                    [source.width, source.height],
                    self.crop_mode,
                    self.crop,
                );
                dimensions[0] as f32 / dimensions[1].max(1) as f32
            });
            let after_pixels = self
                .output
                .as_ref()
                .map(|image| (image.pixels(), [image.width(), image.height()]))
                .or_else(|| {
                    self.source
                        .as_ref()
                        .map(|image| (image.pixels.as_slice(), [image.width, image.height]))
                });
            let editable_crop = (self.crop_mode == CropMode::Editing)
                .then_some(self.crop.as_mut())
                .flatten();
            let locked_aspect = self.crop_aspect_lock == AspectLock::Locked;
            let locked_aspect = locked_aspect.then_some(self.crop_aspect_x / self.crop_aspect_y);
            let main_texture = if self.crop_mode == CropMode::Editing {
                self.source_texture.as_ref()
            } else {
                self.output_texture
                    .as_ref()
                    .or(self.source_texture.as_ref())
            };
            let picker_pixels = self
                .before_preview
                .as_ref()
                .map(|image| (image.pixels(), [image.width(), image.height()]));
            return Self::show_preview(
                ui,
                available,
                main_texture,
                preview_aspect,
                self.texture_sampling,
                after_pixels,
                "Main",
                "Open an image to begin",
                &mut self.preview_view,
                self.loupe_enabled,
                self.white_balance_picker,
                picker_pixels,
                editable_crop,
                locked_aspect,
            );
        }
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
        let picker_pixels = self
            .before_preview
            .as_ref()
            .map(|image| (image.pixels(), [image.width(), image.height()]));

        ui.allocate_ui_with_layout(
            available,
            egui::Layout::left_to_right(egui::Align::Min),
            |ui| {
                let picked_before = ui
                    .allocate_ui_with_layout(
                        before_size,
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            Self::show_preview(
                                ui,
                                before_size,
                                self.source_texture.as_ref(),
                                self.source.as_ref().map(|source| {
                                    let dimensions = preview_content_dimensions(
                                        [source.width, source.height],
                                        self.crop_mode,
                                        self.crop,
                                    );
                                    dimensions[0] as f32 / dimensions[1].max(1) as f32
                                }),
                                self.texture_sampling,
                                before_pixels,
                                "Before",
                                "Open an image to begin",
                                &mut self.preview_view,
                                self.loupe_enabled,
                                self.white_balance_picker,
                                picker_pixels,
                                None,
                                None,
                            )
                        },
                    )
                    .inner;
                let picked_after = ui
                    .allocate_ui_with_layout(
                        after_size,
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            Self::show_preview(
                                ui,
                                after_size,
                                self.output_texture
                                    .as_ref()
                                    .or(self.source_texture.as_ref()),
                                self.source.as_ref().map(|source| {
                                    let dimensions = preview_content_dimensions(
                                        [source.width, source.height],
                                        self.crop_mode,
                                        self.crop,
                                    );
                                    dimensions[0] as f32 / dimensions[1].max(1) as f32
                                }),
                                self.texture_sampling,
                                after_pixels,
                                "After",
                                "The rendered preview will appear here",
                                &mut self.preview_view,
                                self.loupe_enabled,
                                self.white_balance_picker,
                                picker_pixels,
                                None,
                                None,
                            )
                        },
                    )
                    .inner;
                picked_before.or(picked_after)
            },
        )
        .inner
    }

    #[allow(clippy::too_many_arguments, clippy::too_many_lines)]
    fn show_preview(
        ui: &mut egui::Ui,
        size: Vec2,
        texture: Option<&TextureHandle>,
        source_aspect: Option<f32>,
        texture_sampling: PreviewSampling,
        pixels: Option<(&[[f32; 3]], [u32; 2])>,
        label: &str,
        empty: &str,
        view: &mut PreviewView,
        loupe_enabled: bool,
        white_balance_picker: bool,
        picker_pixels: Option<(&[[f32; 3]], [u32; 2])>,
        mut editable_crop: Option<&mut CropSettings>,
        locked_aspect: Option<f32>,
    ) -> Option<[f32; 3]> {
        let (rect, response) = ui.allocate_exact_size(size, Sense::click_and_drag());
        let mut picked = None;
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, CornerRadius::ZERO, PREVIEW_BACKGROUND);
        if white_balance_picker && response.hovered() {
            ui.set_cursor_icon(egui::CursorIcon::Crosshair);
        }
        if response.hovered() {
            let scroll = ui.input(|input| input.smooth_scroll_delta.y);
            if scroll.abs() > f32::EPSILON {
                view.zoom = (view.zoom * (scroll * 0.002).exp()).clamp(1.0, 16.0);
                if (view.zoom - 1.0).abs() < f32::EPSILON {
                    view.pan = Vec2::ZERO;
                }
            }
        }
        if response.dragged() && editable_crop.is_none() {
            view.pan += response.drag_delta();
        }
        let display_aspect = texture.map(|texture| {
            source_aspect
                .unwrap_or_else(|| texture.size_vec2().x / texture.size_vec2().y.max(0.001))
        });
        if let Some(aspect) = display_aspect {
            constrain_preview_pan(rect, aspect, view);
        }
        let image_rect = texture.map(|texture| {
            transformed_image_rect(
                rect,
                display_aspect
                    .unwrap_or_else(|| texture.size_vec2().x / texture.size_vec2().y.max(0.001)),
                *view,
            )
        });
        if let (Some(texture), Some(image_rect)) = (texture, image_rect) {
            let texture_rect = sampled_texture_rect(image_rect, texture_sampling);
            painter.image(
                texture.id(),
                texture_rect,
                Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                Color32::WHITE,
            );
            if let Some(crop) = editable_crop.as_mut() {
                let drag_id = response.id.with("crop-drag-session");
                if response.drag_started()
                    && let Some(start) = response.interact_pointer_pos()
                {
                    let session = CropDragSession {
                        kind: crop_drag_kind(start, image_rect, crop),
                        initial: **crop,
                        start,
                    };
                    ui.data_mut(|data| data.insert_temp(drag_id, session));
                }
                if response.dragged()
                    && let (Some(session), Some(current)) = (
                        ui.data(|data| data.get_temp::<CropDragSession>(drag_id)),
                        response.interact_pointer_pos(),
                    )
                {
                    let previous_crop = **crop;
                    **crop = session.initial;
                    let start = normalised_image_position(session.start, image_rect);
                    let current = normalised_image_position(current, image_rect);
                    let local_current = unrotate_crop_position(
                        current,
                        &session.initial,
                        image_rect.aspect_ratio(),
                    );
                    match session.kind {
                        CropDragKind::New => {
                            **crop = crop_from_drag(
                                start,
                                current,
                                image_rect.aspect_ratio(),
                                locked_aspect,
                            );
                            crop.rotation_degrees = 0.0;
                        }
                        CropDragKind::Left => {
                            crop.left = local_current[0].min(crop.right - 0.001);
                        }
                        CropDragKind::Right => {
                            crop.right = local_current[0].max(crop.left + 0.001);
                        }
                        CropDragKind::Top => {
                            crop.top = local_current[1].min(crop.bottom - 0.001);
                        }
                        CropDragKind::Bottom => {
                            crop.bottom = local_current[1].max(crop.top + 0.001);
                        }
                        CropDragKind::Rotate => {
                            let centre = crop_screen_rect(image_rect, &session.initial).center();
                            let start_vector = session.start - centre;
                            let current_vector = current_position(current, image_rect) - centre;
                            let start_angle = start_vector.y.atan2(start_vector.x);
                            let current_angle = current_vector.y.atan2(current_vector.x);
                            crop.rotation_degrees = (session.initial.rotation_degrees
                                + (current_angle - start_angle).to_degrees())
                            .clamp(-45.0, 45.0);
                            **crop = crop.shrink_to_safe(image_rect.aspect_ratio());
                        }
                    }
                    if session.kind != CropDragKind::New
                        && let Some(target_aspect) = locked_aspect
                    {
                        constrain_crop_aspect(crop, image_rect.aspect_ratio(), target_aspect);
                    }
                    if !crop.is_safe_for_aspect(image_rect.aspect_ratio()) {
                        **crop = previous_crop;
                    }
                }
                if response.drag_stopped() {
                    ui.data_mut(|data| {
                        data.remove::<CropDragSession>(drag_id);
                    });
                }
                draw_crop_overlay(&painter, image_rect, crop);
            }
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
            if white_balance_picker
                && response.clicked()
                && let (Some(pointer), Some((pixels, dimensions))) =
                    (response.interact_pointer_pos(), picker_pixels)
                && texture_rect.contains(pointer)
            {
                picked = sample_pixel_at(pointer, texture_rect, pixels, dimensions);
            }
            if loupe_enabled
                && response.hovered()
                && let Some(pointer) = response.hover_pos()
                && texture_rect.contains(pointer)
            {
                draw_loupe(&painter, rect, texture_rect, texture, pointer);
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
        picked
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
    default: f32,
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
            reset_parameter(value, default);
        }
        ui.label(label).on_hover_text(tooltip);
    });
    ui.horizontal(|ui| {
        ui.add(egui::Slider::new(value, range.clone()).show_value(false));
        ui.add(egui::DragValue::new(value).speed(speed).range(range));
    });
    (*value - before).abs() > f32::EPSILON
}

fn reset_parameter(value: &mut f32, default: f32) {
    *value = default;
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

fn draw_histogram(
    ui: &mut egui::Ui,
    histogram: &Histogram,
    label: &str,
    height: f32,
    scale: DensityScale,
) {
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
                let count = histogram.channels[channel][bin] as f32;
                let fraction = histogram_height_fraction(count, histogram.maximum as f32, scale);
                let height = rect.height() * fraction;
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

fn histogram_height_fraction(count: f32, maximum: f32, scale: DensityScale) -> f32 {
    match scale {
        DensityScale::Linear => count / maximum,
        DensityScale::Logarithmic => count.ln_1p() / maximum.ln_1p(),
    }
}

fn draw_histogram_pair(
    ui: &mut egui::Ui,
    input: Option<&Histogram>,
    output: Option<&Histogram>,
    available_height: f32,
    scale: DensityScale,
) {
    let count = usize::from(input.is_some()) + usize::from(output.is_some());
    let chart_height = ((available_height - count as f32 * 18.0) / count.max(1) as f32).max(16.0);
    if let Some(histogram) = input {
        draw_histogram(ui, histogram, "Input", chart_height, scale);
    }
    if let Some(histogram) = output {
        draw_histogram(ui, histogram, "Output", chart_height, scale);
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

fn show_processing_bar(
    ui: &mut egui::Ui,
    height: f32,
    loading: bool,
    rendering: bool,
    exporting: bool,
    progress: f32,
) {
    let (rect, _) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), height.max(0.0)),
        Sense::hover(),
    );
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, CornerRadius::ZERO, PANEL_BACKGROUND);
    if height < 1.0 {
        return;
    }
    let (fraction, label) = processing_bar_state(loading, rendering, exporting, progress);
    let colour = processing_bar_colour(loading, rendering || exporting);
    let bar_rect = rect.shrink2(Vec2::new(8.0, 10.0));
    let bar = egui::ProgressBar::new(fraction)
        .desired_width(bar_rect.width())
        .fill(colour)
        .text(label)
        .animate(fraction < 1.0);
    ui.scope_builder(egui::UiBuilder::new().max_rect(bar_rect), |ui| {
        ui.add(bar);
    });
}

fn processing_bar_state(
    loading: bool,
    rendering: bool,
    exporting: bool,
    progress: f32,
) -> (f32, &'static str) {
    if loading {
        (0.25, "Loading…")
    } else if rendering {
        (progress.clamp(0.0, 1.0), "Processing…")
    } else if exporting {
        (0.25, "Processing…")
    } else {
        (1.0, "Ready")
    }
}

const fn processing_bar_colour(loading: bool, rendering: bool) -> Color32 {
    if loading {
        PROCESSING_LOADING_COLOUR
    } else if rendering {
        PROCESSING_RENDERING_COLOUR
    } else {
        PROCESSING_READY_COLOUR
    }
}

fn centre_panel_width(available: f32, right_width: f32, splitter: f32) -> f32 {
    // The left splitter has already been allocated by the caller. Only the
    // splitter immediately before the right rail remains in this width.
    (available - right_width - splitter).max(0.0)
}

fn white_balance_picker_button(ui: &mut egui::Ui, selected: bool) -> egui::Response {
    let button = egui::Button::new("").min_size(Vec2::splat(28.0));
    let button = if selected {
        button.fill(ACCENT.gamma_multiply(0.35))
    } else {
        button
    };
    let response = ui.add(button).on_hover_cursor(egui::CursorIcon::Crosshair);
    let colour = if response.hovered() || selected {
        ACCENT
    } else {
        ui.visuals().text_color()
    };
    let stroke = Stroke::new(1.6, colour);
    let centre = response.rect.center();
    let diagonal = Vec2::new(
        std::f32::consts::FRAC_1_SQRT_2,
        -std::f32::consts::FRAC_1_SQRT_2,
    );
    let perpendicular = Vec2::new(-diagonal.y, diagonal.x);
    let body_start = centre - diagonal * 5.0;
    let body_end = centre + diagonal * 5.0;
    let painter = ui.painter();
    painter.line_segment([body_start, body_end], stroke);
    painter.line_segment(
        [
            body_end - perpendicular * 4.0,
            body_end + perpendicular * 4.0,
        ],
        stroke,
    );
    painter.line_segment([body_start - diagonal * 4.0, body_start], stroke);
    painter.circle_stroke(body_start, 3.0, stroke);
    response
}

const fn background_work_needs_repaint(
    loading: bool,
    rendering: bool,
    scoping: bool,
    exporting: bool,
    pending_thumbnails: usize,
) -> bool {
    loading || rendering || scoping || exporting || pending_thumbnails > 0
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
        .filter(|path| path.is_file() && is_supported_image_path(path))
        .collect()
}

fn is_supported_image_path(path: &std::path::Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "png" | "jpg" | "jpeg" | "tif" | "tiff" | "raf"
            )
        })
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

fn mark_thumbnail_failed(items: &mut [FilmStripItem], path: &std::path::Path) {
    if let Some(item) = items.iter_mut().find(|item| item.path == path) {
        item.thumbnail_requested = false;
    }
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

fn rgba_image_with_clipping(
    pixels: &[[f32; 3]],
    width: u32,
    height: u32,
    clipping: Option<&ClippingWarnings>,
    show_highlights: bool,
    show_lowlights: bool,
) -> egui::ColorImage {
    egui::ColorImage::from_rgba_unmultiplied(
        [width as usize, height as usize],
        &rgba_bytes_with_clipping_masks(pixels, clipping, show_highlights, show_lowlights),
    )
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

fn rgba_bytes_with_clipping(
    pixels: &[[f32; 3]],
    show_highlights: bool,
    show_lowlights: bool,
) -> Vec<u8> {
    pixels
        .iter()
        .flat_map(|pixel| {
            let highlight = display_pixel_is_highlight_clipped(*pixel);
            let lowlight = display_pixel_is_lowlight_clipped(*pixel);
            let warning = match (show_highlights && highlight, show_lowlights && lowlight) {
                (true, true) => Some(Color32::from_rgb(255, 48, 255)),
                (true, false) => Some(HIGHLIGHT_CLIP_COLOUR),
                (false, true) => Some(LOWLIGHT_CLIP_COLOUR),
                (false, false) => None,
            };
            warning
                .map_or_else(
                    || pixel.map(|value| (value.clamp(0.0, 1.0) * 255.0).round() as u8),
                    |colour| [colour.r(), colour.g(), colour.b()],
                )
                .into_iter()
                .chain(std::iter::once(u8::MAX))
        })
        .collect()
}

fn display_pixel_is_highlight_clipped(pixel: [f32; 3]) -> bool {
    pixel.iter().any(|value| *value >= 1.0)
}

fn display_pixel_is_lowlight_clipped(pixel: [f32; 3]) -> bool {
    let non_negative = pixel.map(|value| value.max(0.0));
    let lightness =
        0.212_6 * non_negative[0] + 0.715_2 * non_negative[1] + 0.072_2 * non_negative[2];
    lightness <= 0.0
}

fn rgba_bytes_with_clipping_masks(
    pixels: &[[f32; 3]],
    clipping: Option<&ClippingWarnings>,
    show_highlights: bool,
    show_lowlights: bool,
) -> Vec<u8> {
    let Some(clipping) = clipping else {
        return rgba_bytes_with_clipping(pixels, show_highlights, show_lowlights);
    };
    if clipping.width() as usize * clipping.height() as usize != pixels.len()
        || clipping.highlights().len() != pixels.len()
        || clipping.lowlights().len() != pixels.len()
    {
        return rgba_bytes_with_clipping(pixels, show_highlights, show_lowlights);
    }

    pixels
        .iter()
        .enumerate()
        .flat_map(|(index, pixel)| {
            let warning = match (
                show_highlights && clipping.highlights()[index],
                show_lowlights && clipping.lowlights()[index],
            ) {
                (true, true) => Some(Color32::from_rgb(255, 48, 255)),
                (true, false) => Some(HIGHLIGHT_CLIP_COLOUR),
                (false, true) => Some(LOWLIGHT_CLIP_COLOUR),
                (false, false) => None,
            };
            warning
                .map_or_else(
                    || pixel.map(|value| (value.clamp(0.0, 1.0) * 255.0).round() as u8),
                    |colour| [colour.r(), colour.g(), colour.b()],
                )
                .into_iter()
                .chain(std::iter::once(u8::MAX))
        })
        .collect()
}

fn plot_analysis(analysis: &focal_core::scope::VectorscopeAnalysis) -> VectorscopeAnalysis {
    VectorscopeAnalysis {
        space: match analysis.space {
            focal_core::scope::ScopeSpace::Ryb => ScopeSpace::Ryb,
            focal_core::scope::ScopeSpace::Cie1931 => ScopeSpace::Cie1931,
        },
        resolution: analysis.resolution,
        density: analysis.density.clone(),
        colours: analysis.colours.clone(),
        sampled_pixels: analysis.sampled_pixels,
    }
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

fn transformed_image_rect(bounds: Rect, aspect: f32, view: PreviewView) -> Rect {
    let fitted = fit_rect(bounds, aspect);
    Rect::from_center_size(
        fitted.center() + view.pan,
        fitted.size() * view.zoom.clamp(1.0, 16.0),
    )
}

fn constrain_preview_pan(bounds: Rect, aspect: f32, view: &mut PreviewView) {
    let fitted = fit_rect(bounds, aspect);
    let scaled = fitted.size() * view.zoom.clamp(1.0, 16.0);
    let maximum = ((scaled - bounds.size()) * 0.5).max(Vec2::ZERO);
    view.pan.x = view.pan.x.clamp(-maximum.x, maximum.x);
    view.pan.y = view.pan.y.clamp(-maximum.y, maximum.y);
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)]
fn preview_sampling_for_view(
    bounds: Rect,
    source_dimensions: [u32; 2],
    view: PreviewView,
    pixels_per_point: f32,
    maximum_pixels: usize,
) -> PreviewSampling {
    let aspect = source_dimensions[0] as f32 / source_dimensions[1].max(1) as f32;
    let image = transformed_image_rect(bounds, aspect, view);
    let visible = image.intersect(bounds);
    let left = ((visible.left() - image.left()) / image.width()).clamp(0.0, 1.0);
    let top = ((visible.top() - image.top()) / image.height()).clamp(0.0, 1.0);
    let right = ((visible.right() - image.left()) / image.width()).clamp(left, 1.0);
    let bottom = ((visible.bottom() - image.top()) / image.height()).clamp(top, 1.0);
    let display_width = (visible.width() * pixels_per_point).round().max(1.0) as u32;
    let display_height = (visible.height() * pixels_per_point).round().max(1.0) as u32;
    let native_width = ((right - left) * source_dimensions[0] as f32)
        .ceil()
        .max(1.0) as u32;
    let native_height = ((bottom - top) * source_dimensions[1] as f32)
        .ceil()
        .max(1.0) as u32;
    let mut width = display_width.min(native_width);
    let mut height = display_height.min(native_height);
    let pixel_count = width as usize * height as usize;
    if pixel_count > maximum_pixels {
        let scale = (maximum_pixels as f64 / pixel_count as f64).sqrt();
        width = (f64::from(width) * scale).floor().max(1.0) as u32;
        height = (f64::from(height) * scale).floor().max(1.0) as u32;
    }
    PreviewSampling {
        left,
        top,
        right,
        bottom,
        width,
        height,
    }
}

fn sampled_texture_rect(image: Rect, sampling: PreviewSampling) -> Rect {
    Rect::from_min_max(
        Pos2::new(
            image.left() + sampling.left * image.width(),
            image.top() + sampling.top * image.height(),
        ),
        Pos2::new(
            image.left() + sampling.right * image.width(),
            image.top() + sampling.bottom * image.height(),
        ),
    )
}

fn sample_pixel_at(
    position: Pos2,
    image_rect: Rect,
    pixels: &[[f32; 3]],
    dimensions: [u32; 2],
) -> Option<[f32; 3]> {
    let [width, height] = dimensions;
    if !image_rect.contains(position)
        || width == 0
        || height == 0
        || pixels.len() < width as usize * height as usize
    {
        return None;
    }
    let [x, y] = normalised_image_position(position, image_rect);
    let index =
        (y * height as f32).floor() as usize * width as usize + (x * width as f32).floor() as usize;
    pixels.get(index).copied()
}

fn loupe_rect(bounds: Rect, pointer: Pos2, size: f32) -> Rect {
    let size = size.max(1.0).min(bounds.width().min(bounds.height()));
    let candidate = Rect::from_center_size(pointer, Vec2::splat(size));
    let shift_x = if candidate.left() < bounds.left() {
        bounds.left() - candidate.left()
    } else if candidate.right() > bounds.right() {
        bounds.right() - candidate.right()
    } else {
        0.0
    };
    let shift_y = if candidate.top() < bounds.top() {
        bounds.top() - candidate.top()
    } else if candidate.bottom() > bounds.bottom() {
        bounds.bottom() - candidate.bottom()
    } else {
        0.0
    };
    candidate.translate(Vec2::new(shift_x, shift_y))
}

fn loupe_uv_rect(pointer: Pos2, displayed_image: Rect, loupe: Rect, zoom: f32) -> Rect {
    let centre = normalised_image_position(pointer, displayed_image);
    let span_x = (loupe.width() / displayed_image.width().max(1.0) / zoom.max(1.0)).clamp(0.0, 1.0);
    let span_y =
        (loupe.height() / displayed_image.height().max(1.0) / zoom.max(1.0)).clamp(0.0, 1.0);
    let left = (centre[0] - span_x * 0.5).clamp(0.0, 1.0 - span_x);
    let top = (centre[1] - span_y * 0.5).clamp(0.0, 1.0 - span_y);
    Rect::from_min_max(Pos2::new(left, top), Pos2::new(left + span_x, top + span_y))
}

fn draw_loupe(
    painter: &egui::Painter,
    bounds: Rect,
    image: Rect,
    texture: &TextureHandle,
    pointer_pos: Pos2,
) {
    let loupe = loupe_rect(bounds, pointer_pos, LOUPE_SIZE);
    painter.rect_filled(loupe, CornerRadius::same(4), Color32::BLACK);
    painter.image(
        texture.id(),
        loupe,
        loupe_uv_rect(pointer_pos, image, loupe, LOUPE_ZOOM),
        Color32::WHITE,
    );
    painter.rect_stroke(
        loupe,
        CornerRadius::same(4),
        Stroke::new(2.0, Color32::WHITE),
        StrokeKind::Outside,
    );
    painter.line_segment(
        [
            loupe.center() - Vec2::new(6.0, 0.0),
            loupe.center() + Vec2::new(6.0, 0.0),
        ],
        Stroke::new(1.0, Color32::from_white_alpha(180)),
    );
    painter.line_segment(
        [
            loupe.center() - Vec2::new(0.0, 6.0),
            loupe.center() + Vec2::new(0.0, 6.0),
        ],
        Stroke::new(1.0, Color32::from_white_alpha(180)),
    );
}

/// Maps a display-encoded sRGB sample into the editor's two bounded opponent
/// controls. The picker intentionally samples the immutable Before
/// preview, so it cannot feed an already-adjusted pixel back into the edit.
fn white_balance_from_sample(sample: [f32; 3]) -> Option<(f32, f32)> {
    const MINIMUM: f32 = 1.0e-4;
    if sample
        .iter()
        .any(|value| !value.is_finite() || *value < 0.0)
    {
        return None;
    }
    let [red, green, blue] = sample;
    if red.max(green).max(blue) < MINIMUM {
        return None;
    }
    let warmth = (-50.0 * ((red + MINIMUM) / (blue + MINIMUM)).log2()).clamp(-100.0, 100.0);
    let magenta = (red + blue) * 0.5;
    let tint = (-100.0 * ((magenta + MINIMUM) / (green + MINIMUM)).log2()).clamp(-100.0, 100.0);
    Some((warmth, tint))
}

fn toolbar_regions(bounds: Rect) -> [Rect; 3] {
    const TAB_AREA_WIDTH: f32 = 230.0;
    let tab_rect = Rect::from_center_size(
        bounds.center(),
        Vec2::new(TAB_AREA_WIDTH.min(bounds.width()), bounds.height()),
    );
    [
        Rect::from_min_max(bounds.min, Pos2::new(tab_rect.left(), bounds.bottom())),
        tab_rect,
        Rect::from_min_max(Pos2::new(tab_rect.right(), bounds.top()), bounds.max),
    ]
}

fn normalised_image_position(position: Pos2, image: Rect) -> [f32; 2] {
    [
        ((position.x - image.left()) / image.width()).clamp(0.0, 1.0),
        ((position.y - image.top()) / image.height()).clamp(0.0, 1.0),
    ]
}

fn current_position(normalised: [f32; 2], image: Rect) -> Pos2 {
    Pos2::new(
        image.left() + normalised[0] * image.width(),
        image.top() + normalised[1] * image.height(),
    )
}

fn unrotate_crop_position(point: [f32; 2], crop: &CropSettings, image_aspect: f32) -> [f32; 2] {
    let centre = [
        (crop.left + crop.right) * 0.5,
        (crop.top + crop.bottom) * 0.5,
    ];
    let angle = -crop.rotation_degrees.to_radians();
    let (sin, cos) = angle.sin_cos();
    let x = (point[0] - centre[0]) * image_aspect;
    let y = point[1] - centre[1];
    [
        (cos * x - sin * y) / image_aspect + centre[0],
        sin * x + cos * y + centre[1],
    ]
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CropDragKind {
    New,
    Left,
    Right,
    Top,
    Bottom,
    Rotate,
}

#[derive(Clone, Copy)]
struct CropDragSession {
    kind: CropDragKind,
    initial: CropSettings,
    start: Pos2,
}

fn crop_drag_kind(start: Pos2, image: Rect, crop: &CropSettings) -> CropDragKind {
    const HIT_RADIUS: f32 = 12.0;
    let geometry = crop_screen_geometry(image, crop);
    let candidates = [
        (CropDragKind::Left, geometry.side_handles[3]),
        (CropDragKind::Right, geometry.side_handles[1]),
        (CropDragKind::Top, geometry.side_handles[0]),
        (CropDragKind::Bottom, geometry.side_handles[2]),
        (CropDragKind::Rotate, geometry.rotation_handle),
    ];
    candidates
        .into_iter()
        .find_map(|(kind, position)| (position.distance(start) <= HIT_RADIUS).then_some(kind))
        .unwrap_or(CropDragKind::New)
}

fn crop_screen_rect(image: Rect, crop: &CropSettings) -> Rect {
    Rect::from_min_max(
        Pos2::new(
            image.left() + crop.left * image.width(),
            image.top() + crop.top * image.height(),
        ),
        Pos2::new(
            image.left() + crop.right * image.width(),
            image.top() + crop.bottom * image.height(),
        ),
    )
}

struct CropScreenGeometry {
    corners: [Pos2; 4],
    side_handles: [Pos2; 4],
    rotation_handle: Pos2,
}

fn crop_screen_geometry(image: Rect, crop: &CropSettings) -> CropScreenGeometry {
    let rect = crop_screen_rect(image, crop);
    let centre = rect.center();
    let angle = crop.rotation_degrees.to_radians();
    let (sin, cos) = angle.sin_cos();
    let rotate = |point: Pos2| {
        let delta = point - centre;
        centre + Vec2::new(cos * delta.x - sin * delta.y, sin * delta.x + cos * delta.y)
    };
    let corners = [
        rotate(rect.left_top()),
        rotate(rect.right_top()),
        rotate(rect.right_bottom()),
        rotate(rect.left_bottom()),
    ];
    let midpoint = |a: Pos2, b: Pos2| a + (b - a) * 0.5;
    let side_handles = [
        midpoint(corners[0], corners[1]),
        midpoint(corners[1], corners[2]),
        midpoint(corners[2], corners[3]),
        midpoint(corners[3], corners[0]),
    ];
    let outward = (side_handles[0] - centre).normalized();
    let outside_candidate = side_handles[0] + outward * 28.0;
    let inside_candidate = side_handles[0] - outward * 28.0;
    let rotation_handle = if image.contains(outside_candidate) {
        outside_candidate
    } else {
        Pos2::new(
            inside_candidate.x.clamp(image.left(), image.right()),
            inside_candidate.y.clamp(image.top(), image.bottom()),
        )
    };
    CropScreenGeometry {
        corners,
        side_handles,
        rotation_handle,
    }
}

fn draw_crop_overlay(painter: &egui::Painter, image: Rect, crop: &CropSettings) {
    let geometry = crop_screen_geometry(image, crop);
    let [top_left, top_right, bottom_right, bottom_left] = geometry.corners;
    let shade = Color32::from_black_alpha(145);
    for outside in [
        vec![image.left_top(), image.right_top(), top_right, top_left],
        vec![
            image.right_top(),
            image.right_bottom(),
            bottom_right,
            top_right,
        ],
        vec![
            image.right_bottom(),
            image.left_bottom(),
            bottom_left,
            bottom_right,
        ],
        vec![image.left_bottom(), image.left_top(), top_left, bottom_left],
    ] {
        painter.add(egui::Shape::convex_polygon(outside, shade, Stroke::NONE));
    }
    painter.add(egui::Shape::closed_line(
        geometry.corners.to_vec(),
        Stroke::new(1.5, Color32::WHITE),
    ));
    for handle in geometry.side_handles {
        painter.rect_filled(
            Rect::from_center_size(handle, Vec2::splat(8.0)),
            CornerRadius::same(1),
            Color32::WHITE,
        );
    }
    painter.line_segment(
        [geometry.side_handles[0], geometry.rotation_handle],
        Stroke::new(1.5, Color32::WHITE),
    );
    painter.circle_filled(geometry.rotation_handle, 5.0, Color32::WHITE);
}

fn constrain_crop_aspect(crop: &mut CropSettings, image_aspect: f32, target_aspect: f32) {
    if image_aspect <= 0.0 || target_aspect <= 0.0 {
        return;
    }
    let centre_x = (crop.left + crop.right) * 0.5;
    let centre_y = (crop.top + crop.bottom) * 0.5;
    let available_width = crop.right - crop.left;
    let available_height = crop.bottom - crop.top;
    let normalised_ratio = target_aspect / image_aspect;
    let (width, height) = if available_width / available_height > normalised_ratio {
        (available_height * normalised_ratio, available_height)
    } else {
        (available_width, available_width / normalised_ratio)
    };
    crop.left = (centre_x - width * 0.5).clamp(0.0, 1.0 - width);
    crop.top = (centre_y - height * 0.5).clamp(0.0, 1.0 - height);
    crop.right = crop.left + width;
    crop.bottom = crop.top + height;
}

fn crop_from_drag(
    start: [f32; 2],
    mut end: [f32; 2],
    image_aspect: f32,
    target_aspect: Option<f32>,
) -> CropSettings {
    if let Some(target) = target_aspect {
        let ratio = target / image_aspect.max(0.001);
        let dx = end[0] - start[0];
        let dy = end[1] - start[1];
        if dx.abs() / dy.abs().max(0.001) > ratio {
            let direction = if dy < 0.0 { -1.0 } else { 1.0 };
            end[1] = start[1] + dx.abs() / ratio * direction;
        } else {
            let direction = if dx < 0.0 { -1.0 } else { 1.0 };
            end[0] = start[0] + dy.abs() * ratio * direction;
        }
        if !(0.0..=1.0).contains(&end[0]) || !(0.0..=1.0).contains(&end[1]) {
            let scale_x = if end[0] < 0.0 {
                start[0] / (start[0] - end[0])
            } else if end[0] > 1.0 {
                (1.0 - start[0]) / (end[0] - start[0])
            } else {
                1.0
            };
            let scale_y = if end[1] < 0.0 {
                start[1] / (start[1] - end[1])
            } else if end[1] > 1.0 {
                (1.0 - start[1]) / (end[1] - start[1])
            } else {
                1.0
            };
            let scale = scale_x.min(scale_y).clamp(0.0, 1.0);
            end = [
                start[0] + (end[0] - start[0]) * scale,
                start[1] + (end[1] - start[1]) * scale,
            ];
        }
    }
    CropSettings {
        left: start[0].min(end[0]),
        top: start[1].min(end[1]),
        right: start[0].max(end[0]).max(start[0].min(end[0]) + 0.001),
        bottom: start[1].max(end[1]).max(start[1].min(end[1]) + 0.001),
        rotation_degrees: 0.0,
    }
}

fn preview_texture_options(
    preview: &Image,
    full_source: Option<&DecodedImage>,
) -> egui::TextureOptions {
    if full_source.is_some_and(|source| {
        source.width as usize * source.height as usize <= PREVIEW_MAX_PIXELS
            || (source.width == preview.width() && source.height == preview.height())
    }) {
        egui::TextureOptions::NEAREST
    } else {
        egui::TextureOptions::LINEAR
    }
}

#[allow(clippy::cast_possible_truncation)]
fn bounded_preview_dimensions(width: u32, height: u32, maximum_pixels: usize) -> [u32; 2] {
    let source_pixels = width as usize * height as usize;
    if source_pixels <= maximum_pixels {
        return [width, height];
    }
    let scale = (maximum_pixels as f64 / source_pixels as f64).sqrt();
    [
        (f64::from(width) * scale).floor().max(1.0) as u32,
        (f64::from(height) * scale).floor().max(1.0) as u32,
    ]
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn preview_content_dimensions(
    source: [u32; 2],
    crop_mode: CropMode,
    crop: Option<CropSettings>,
) -> [u32; 2] {
    if crop_mode != CropMode::Applied {
        return source;
    }
    crop.map_or(source, |crop| {
        [
            ((crop.right - crop.left) * source[0] as f32)
                .round()
                .max(1.0) as u32,
            ((crop.bottom - crop.top) * source[1] as f32)
                .round()
                .max(1.0) as u32,
        ]
    })
}

fn default_export_path(directory: &std::path::Path, source_path: &std::path::Path) -> PathBuf {
    let stem = source_path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("untitled");
    directory.join(format!("{stem}-edited.png"))
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
    fn clipping_warnings_are_independent_and_preserve_unclipped_pixels() {
        let pixels = [[0.0, 0.2, 0.3], [0.2, 1.0, 0.3], [0.2, 0.3, 0.4]];
        let normal = rgba_bytes_with_clipping(&pixels, false, false);
        assert_eq!(&normal[0..4], &[0, 51, 77, 255]);
        assert_eq!(&normal[4..8], &[51, 255, 77, 255]);

        let highlights = rgba_bytes_with_clipping(&pixels, true, false);
        assert_eq!(&highlights[0..4], &[0, 51, 77, 255]);
        assert_eq!(&highlights[4..8], &[255, 48, 48, 255]);
        let lowlights = rgba_bytes_with_clipping(&pixels, false, true);
        assert_eq!(&lowlights[0..4], &[0, 51, 77, 255]);
        assert_eq!(&lowlights[4..8], &[51, 255, 77, 255]);
    }

    #[test]
    fn clipping_masks_mark_white_and_black_after_the_output_transform() {
        let source = Image::new(
            3,
            1,
            vec![[0.0, 0.0, 0.0], [1.0, 1.0, 1.0], [0.5, 0.5, 0.5]],
            focal_core::ImageContract::SRGB_DISPLAY,
        )
        .unwrap();
        let context = focal_core::RenderContext::new(focal_core::RenderQuality::Preview);
        let (rendered, report) = focal_core::Pipeline::default()
            .render_with_context(source, &context, &mut |_| {})
            .unwrap();
        let clipping = report.clipping.as_ref();
        let bytes = rgba_bytes_with_clipping_masks(rendered.pixels(), clipping, true, true);

        assert_eq!(&bytes[0..4], &[48, 128, 255, 255]);
        assert_eq!(&bytes[4..8], &[255, 48, 48, 255]);
        assert_eq!(&bytes[8..12], &[128, 128, 127, 255]);
    }

    #[test]
    fn clipping_fallback_does_not_mark_chromatic_highlights_as_lowlights() {
        let pixels = [[1.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, 0.0, 0.0]];
        let bytes = rgba_bytes_with_clipping(&pixels, false, true);

        assert_eq!(&bytes[0..4], &[255, 0, 0, 255]);
        assert_eq!(&bytes[4..8], &[0, 0, 255, 255]);
        assert_eq!(&bytes[8..12], &[48, 128, 255, 255]);
    }

    #[test]
    fn loupe_and_pixel_sampling_clamp_at_view_boundaries() {
        let bounds = Rect::from_min_size(Pos2::ZERO, Vec2::new(300.0, 200.0));
        assert_eq!(
            loupe_rect(bounds, Pos2::new(5.0, 8.0), 100.0),
            Rect::from_min_size(Pos2::ZERO, Vec2::new(100.0, 100.0))
        );
        let image = Rect::from_min_size(Pos2::ZERO, Vec2::new(200.0, 100.0));
        let uv = loupe_uv_rect(
            Pos2::new(0.0, 0.0),
            image,
            Rect::from_min_size(Pos2::ZERO, Vec2::splat(100.0)),
            LOUPE_ZOOM,
        );
        assert_eq!(uv.min, Pos2::ZERO);
        assert!(uv.max.x > 0.0 && uv.max.y > 0.0);

        let pixels = [
            [0.1, 0.2, 0.3],
            [0.4, 0.5, 0.6],
            [0.7, 0.8, 0.9],
            [1.0, 1.0, 1.0],
        ];
        assert_eq!(
            sample_pixel_at(Pos2::new(100.0, 50.0), image, &pixels, [2, 2]),
            Some([1.0, 1.0, 1.0])
        );
        let sampled = sampled_texture_rect(
            image,
            PreviewSampling {
                left: 0.25,
                top: 0.0,
                right: 0.75,
                bottom: 1.0,
                width: 2,
                height: 2,
            },
        );
        let uv = loupe_uv_rect(
            sampled.center(),
            sampled,
            Rect::from_min_size(Pos2::ZERO, Vec2::splat(100.0)),
            LOUPE_ZOOM,
        );
        assert!((uv.center().x - 0.5).abs() < f32::EPSILON);
        assert!((uv.center().y - 0.5).abs() < f32::EPSILON);
        assert!(sample_pixel_at(Pos2::ZERO, image, &pixels[..3], [2, 2]).is_none());
        assert!(sample_pixel_at(Pos2::ZERO, image, &pixels, [0, 2]).is_none());
        assert!(sample_pixel_at(Pos2::new(220.0, 50.0), sampled, &pixels, [2, 2]).is_none());
    }

    #[test]
    fn white_balance_picker_handles_neutral_dark_and_extreme_samples() {
        let (warmth, tint) = white_balance_from_sample([0.5, 0.5, 0.5]).unwrap();
        assert!(warmth.abs() < 0.01);
        assert!(tint.abs() < 0.01);
        let (warmth, tint) = white_balance_from_sample([1.0, 0.5, 0.25]).unwrap();
        assert!(warmth < 0.0);
        assert!(tint.is_finite());
        assert_eq!(white_balance_from_sample([0.0, 0.0, 0.0]), None);
        assert_eq!(white_balance_from_sample([f32::NAN, 0.5, 0.5]), None);
        let (warmth, tint) = white_balance_from_sample([100.0, 0.0, 0.0]).unwrap();
        assert!((warmth + 100.0).abs() < f32::EPSILON);
        assert!((tint + 100.0).abs() < f32::EPSILON);
    }

    #[test]
    fn processing_bar_state_prioritises_loading_and_clamps_progress() {
        assert_eq!(
            processing_bar_state(true, true, true, 0.8),
            (0.25, "Loading…")
        );
        assert_eq!(
            processing_bar_state(false, true, false, -1.0),
            (0.0, "Processing…")
        );
        assert_eq!(
            processing_bar_state(false, true, false, 2.0),
            (1.0, "Processing…")
        );
        assert_eq!(
            processing_bar_state(false, false, true, 0.3),
            (0.25, "Processing…")
        );
        assert_eq!(
            processing_bar_state(false, false, false, 0.3),
            (1.0, "Ready")
        );
        assert_eq!(
            processing_bar_colour(true, false),
            PROCESSING_LOADING_COLOUR
        );
        assert_eq!(
            processing_bar_colour(false, true),
            PROCESSING_RENDERING_COLOUR
        );
        assert_eq!(processing_bar_colour(false, false), PROCESSING_READY_COLOUR);
    }

    #[test]
    fn centre_panel_stays_inside_the_remaining_width() {
        let available = 1_000.0;
        let right = 330.0;
        let splitter = 10.0;
        let centre = centre_panel_width(available, right, splitter);
        assert!((centre + right + splitter - available).abs() < f32::EPSILON);
        assert!((centre_panel_width(300.0, 240.0, splitter) - 50.0).abs() < f32::EPSILON);
    }

    #[test]
    fn copied_adjustments_round_trip_all_edit_values() {
        let context = egui::Context::default();
        let mut app = FocalEditorApp::new(&context);
        let adjustments = Adjustments {
            warmth: 12.0,
            tint: -8.0,
            exposure_stops: 1.5,
            contrast: -22.0,
            local_contrast_amount: 30.0,
            local_contrast_radius: 24.0,
            saturation: 18.0,
            noise_luminance: 6.0,
            noise_colour: 11.0,
            crop: Some(CropSettings {
                left: 0.1,
                top: 0.2,
                right: 0.8,
                bottom: 0.9,
                rotation_degrees: 3.0,
            }),
        };
        app.set_adjustments(adjustments);
        assert_eq!(app.adjustments(), adjustments);
    }

    #[test]
    fn pasting_to_another_filmstrip_item_waits_for_that_image_to_load() {
        let context = egui::Context::default();
        let mut app = FocalEditorApp::new(&context);
        app.source_path = Some(PathBuf::from("source.jpg"));
        app.copied_edits = Some(Adjustments {
            exposure_stops: 1.0,
            ..Adjustments::default()
        });
        let previous_load_generation = app.latest_load_generation;

        app.paste_to_path(PathBuf::from("target.jpg"), &context);

        assert!(app.paste_after_load);
        assert!(app.latest_load_generation > previous_load_generation);
        assert_eq!(app.source_path, Some(PathBuf::from("source.jpg")));
    }

    #[test]
    fn empty_histograms_and_histogram_scales_have_defined_boundaries() {
        let empty = Histogram::from_pixels(&[]);
        assert_eq!(empty.maximum, 0);
        assert_eq!(empty.channels, [[0; 256]; 3]);
        assert!(histogram_height_fraction(0.0, 10.0, DensityScale::Linear).abs() < f32::EPSILON);
        assert!(
            histogram_height_fraction(0.0, 10.0, DensityScale::Logarithmic).abs() < f32::EPSILON
        );
        assert!(
            (histogram_height_fraction(10.0, 10.0, DensityScale::Linear) - 1.0).abs()
                < f32::EPSILON
        );
        assert!(
            (histogram_height_fraction(10.0, 10.0, DensityScale::Logarithmic) - 1.0).abs()
                < f32::EPSILON
        );
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
            warmth: 8.0,
            tint: -3.0,
            local_contrast_amount: 20.0,
            local_contrast_radius: 64.0,
            saturation: 18.0,
            noise_luminance: 12.0,
            noise_colour: 24.0,
            crop: None,
        };
        let json = serde_json::to_string(&state).unwrap();
        assert_eq!(serde_json::from_str::<EditSidecar>(&json).unwrap(), state);
    }

    #[test]
    fn resetting_local_radius_restores_its_documented_default() {
        let mut radius = 12.0;
        reset_parameter(&mut radius, 80.0);
        assert!((radius - 80.0).abs() < f32::EPSILON);
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
    fn panel_split_clamps_desired_height_and_handles_zero_space() {
        assert_eq!(split_panel_heights(0.0, 50.0, 10.0, 20.0, 5.0), (0.0, 0.0));
        assert_eq!(
            split_panel_heights(100.0, -10.0, 10.0, 20.0, 5.0),
            (10.0, 85.0)
        );
        assert_eq!(
            split_panel_heights(100.0, 200.0, 10.0, 20.0, 5.0),
            (75.0, 20.0)
        );
    }

    #[test]
    fn pending_scope_analysis_keeps_ui_polling_after_render_finishes() {
        assert!(background_work_needs_repaint(false, false, true, false, 0));
        assert!(!background_work_needs_repaint(
            false, false, false, false, 0
        ));
    }

    #[test]
    fn pending_thumbnails_keep_ui_polling_after_other_work_finishes() {
        assert!(background_work_needs_repaint(false, false, false, false, 1));
        assert!(!background_work_needs_repaint(
            false, false, false, false, 0
        ));
    }

    #[test]
    fn every_background_work_flag_keeps_the_editor_repainting() {
        for state in [
            (true, false, false, false, 0),
            (false, true, false, false, 0),
            (false, false, true, false, 0),
            (false, false, false, true, 0),
            (false, false, false, false, 1),
        ] {
            assert!(background_work_needs_repaint(
                state.0, state.1, state.2, state.3, state.4
            ));
        }
        assert!(!background_work_needs_repaint(
            false, false, false, false, 0
        ));
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
        assert!(!film_strip_paths_match(
            &current,
            &[PathBuf::from("b.jpg"), PathBuf::from("a.jpg")]
        ));
        assert!(!film_strip_paths_match(&current[..1], &discovered));
    }

    #[test]
    fn filmstrip_prefetches_twice_the_visible_thumbnail_count() {
        assert_eq!(prefetch_range(100, 0, 9), 0..20);
        assert_eq!(prefetch_range(100, 20, 29), 15..35);
        assert_eq!(prefetch_range(100, 90, 99), 80..100);
        assert_eq!(prefetch_range(12, 0, 9), 0..12);
        assert_eq!(prefetch_range(0, 0, 0), 0..0);
        assert_eq!(prefetch_range(10, 10, 10), 0..0);
        assert_eq!(prefetch_range(10, 5, 4), 0..0);
        assert_eq!(prefetch_range(10, 8, 20), 6..10);
    }

    #[test]
    fn thumbnail_requests_cover_cached_requested_and_failed_states() {
        let path = PathBuf::from("thumbnail.png");
        let mut items = vec![
            FilmStripItem {
                path: path.clone(),
                thumbnail: None,
                dimensions: None,
                thumbnail_requested: false,
            },
            FilmStripItem {
                path: PathBuf::from("dimensions.png"),
                thumbnail: None,
                dimensions: Some([10, 10]),
                thumbnail_requested: false,
            },
            FilmStripItem {
                path: PathBuf::from("requested.png"),
                thumbnail: None,
                dimensions: None,
                thumbnail_requested: true,
            },
        ];
        assert!(thumbnail_needs_request(&items[0]));
        assert!(!thumbnail_needs_request(&items[1]));
        assert!(!thumbnail_needs_request(&items[2]));
        mark_thumbnail_failed(&mut items, &path);
        assert!(thumbnail_needs_request(&items[0]));
        mark_thumbnail_failed(&mut items, std::path::Path::new("missing.png"));
    }

    #[test]
    fn sibling_discovery_filters_supported_files_and_ignores_missing_parents() {
        let directory =
            std::env::temp_dir().join(format!("focal-editor-siblings-{}", std::process::id()));
        std::fs::create_dir_all(directory.join("nested.jpg")).unwrap();
        for name in ["a.PNG", "b.jpg", "c.JPEG", "d.tiff", "e.RAF", "ignore.txt"] {
            std::fs::write(directory.join(name), []).unwrap();
        }
        let mut found = discover_sibling_images(&directory.join("a.PNG"))
            .into_iter()
            .filter_map(|path| path.file_name().map(std::borrow::ToOwned::to_owned))
            .collect::<Vec<_>>();
        found.sort();
        assert_eq!(
            found,
            ["a.PNG", "b.jpg", "c.JPEG", "d.tiff", "e.RAF"]
                .into_iter()
                .map(std::ffi::OsString::from)
                .collect::<Vec<_>>()
        );
        assert!(discover_sibling_images(std::path::Path::new("missing.jpg")).is_empty());
        std::fs::remove_dir_all(directory).unwrap();
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

        app.rendering = false;
        app.loading = true;
        assert!(!app.can_export());
    }

    #[test]
    fn opening_another_image_invalidates_in_flight_preview_and_export_state() {
        let context = egui::Context::default();
        let mut app = FocalEditorApp::new(&context);
        app.latest_generation = 7;
        app.output_generation = Some(7);
        app.output = Some(
            Image::new(
                1,
                1,
                vec![[0.5; 3]],
                focal_core::ImageContract::SRGB_DISPLAY,
            )
            .unwrap(),
        );
        app.open_path(PathBuf::from("new-selection.jpg"));

        assert_ne!(app.latest_generation, 7);
        assert!(app.output.is_none());
        assert!(!app.can_export());
    }

    #[test]
    fn a_new_preview_cancels_and_invalidates_an_export_request() {
        let context = egui::Context::default();
        let mut app = FocalEditorApp::new(&context);
        let cancellation = CancellationToken::new();
        app.exporting = true;
        app.export_generation = Some(4);
        app.export_cancellation = Some(cancellation.clone());
        assert!(app.export_result_is_current(4));
        assert!(!app.export_result_is_current(5));

        app.request_preview(&context);

        assert!(cancellation.is_cancelled());
        assert!(!app.exporting);
        assert_eq!(app.export_generation, None);
        assert!(app.export_cancellation.is_none());
    }

    #[test]
    fn failed_thumbnail_decode_becomes_retryable() {
        let mut items = vec![FilmStripItem {
            path: PathBuf::from("broken.jpg"),
            thumbnail: None,
            dimensions: None,
            thumbnail_requested: true,
        }];
        mark_thumbnail_failed(&mut items, std::path::Path::new("broken.jpg"));
        assert!(thumbnail_needs_request(&items[0]));
    }

    #[test]
    fn main_photo_transform_uses_fit_as_one_x_zoom_and_applies_pan() {
        let bounds = Rect::from_min_size(Pos2::ZERO, Vec2::new(200.0, 100.0));
        let transformed = transformed_image_rect(
            bounds,
            1.0,
            PreviewView {
                zoom: 2.0,
                pan: Vec2::new(10.0, -5.0),
            },
        );

        assert_eq!(transformed.size(), Vec2::splat(200.0));
        assert_eq!(
            transformed.center(),
            bounds.center() + Vec2::new(10.0, -5.0)
        );
        assert_eq!(
            transformed_image_rect(
                bounds,
                1.0,
                PreviewView {
                    zoom: 0.0,
                    pan: Vec2::ZERO,
                }
            )
            .size(),
            Vec2::new(100.0, 100.0)
        );
    }

    #[test]
    fn image_geometry_clamps_aspect_zoom_and_normalised_positions() {
        let bounds = Rect::from_min_size(Pos2::ZERO, Vec2::new(200.0, 100.0));
        let narrow = fit_rect(bounds, 0.0).size();
        assert!((narrow.x - 0.1).abs() < 1.0e-4);
        assert!((narrow.y - 100.0).abs() < 1.0e-4);
        assert_eq!(
            fit_rect(Rect::from_min_size(Pos2::ZERO, Vec2::ZERO), 1.0).size(),
            Vec2::ZERO
        );

        let [x, y] = normalised_image_position(Pos2::new(-1.0, 101.0), bounds);
        assert!(x.abs() < f32::EPSILON);
        assert!((y - 1.0).abs() < f32::EPSILON);
        assert_eq!(
            current_position([0.25, 0.75], bounds),
            Pos2::new(50.0, 75.0)
        );
        assert_eq!(
            sampled_texture_rect(bounds, PreviewSampling::full(1, 1)),
            bounds
        );
    }

    #[test]
    fn zoom_sampling_uses_the_visible_region_and_never_exceeds_one_megapixel() {
        let bounds = Rect::from_min_size(Pos2::ZERO, Vec2::new(1_200.0, 800.0));
        let sampling = preview_sampling_for_view(
            bounds,
            [8_000, 5_000],
            PreviewView {
                zoom: 4.0,
                pan: Vec2::new(100.0, -50.0),
            },
            1.0,
            PREVIEW_MAX_PIXELS,
        );
        assert!(sampling.left > 0.0);
        assert!(sampling.top > 0.0);
        assert!(sampling.right < 1.0);
        assert!(sampling.bottom < 1.0);
        assert!(sampling.width as usize * sampling.height as usize <= PREVIEW_MAX_PIXELS);
    }

    #[test]
    fn toolbar_regions_share_one_reserved_row_without_overlap() {
        let bounds = Rect::from_min_size(Pos2::new(0.0, 20.0), Vec2::new(1_400.0, 48.0));
        let [left, tabs, right] = toolbar_regions(bounds);
        let same = |a: f32, b: f32| (a - b).abs() < f32::EPSILON;

        assert!(same(left.top(), bounds.top()));
        assert!(same(tabs.top(), bounds.top()));
        assert!(same(right.top(), bounds.top()));
        assert!(same(left.bottom(), bounds.bottom()));
        assert!(same(tabs.bottom(), bounds.bottom()));
        assert!(same(right.bottom(), bounds.bottom()));
        assert!(same(left.right(), tabs.left()));
        assert!(same(tabs.right(), right.left()));
    }

    #[test]
    fn crop_aspect_lock_preserves_the_requested_pixel_ratio() {
        let mut crop = CropSettings {
            left: 0.1,
            top: 0.1,
            right: 0.9,
            bottom: 0.9,
            rotation_degrees: 0.0,
        };
        constrain_crop_aspect(&mut crop, 3.0 / 2.0, 1.0);
        let pixel_ratio = (crop.right - crop.left) * 1.5 / (crop.bottom - crop.top);
        assert!((pixel_ratio - 1.0).abs() < 1.0e-6);
        let unchanged = crop;
        let mut invalid = crop;
        constrain_crop_aspect(&mut invalid, 0.0, 1.0);
        assert_eq!(invalid, unchanged);
    }

    #[test]
    fn crop_hit_testing_distinguishes_side_and_rotation_handles() {
        let image = Rect::from_min_size(Pos2::ZERO, Vec2::new(300.0, 200.0));
        let crop = CropSettings {
            left: 0.25,
            top: 0.25,
            right: 0.75,
            bottom: 0.75,
            rotation_degrees: 0.0,
        };
        let selected = crop_screen_rect(image, &crop);

        assert_eq!(
            crop_drag_kind(
                Pos2::new(selected.left(), selected.center().y),
                image,
                &crop
            ),
            CropDragKind::Left
        );
        assert_eq!(
            crop_drag_kind(
                Pos2::new(selected.center().x, selected.top() - 28.0),
                image,
                &crop
            ),
            CropDragKind::Rotate
        );
    }

    #[test]
    fn rotated_crop_geometry_rotates_overlay_and_handle_positions() {
        let image = Rect::from_min_size(Pos2::ZERO, Vec2::new(300.0, 200.0));
        let crop = CropSettings {
            left: 0.25,
            top: 0.25,
            right: 0.75,
            bottom: 0.75,
            rotation_degrees: 20.0,
        };
        let geometry = crop_screen_geometry(image, &crop);

        assert!(geometry.corners[0].y < geometry.corners[1].y);
        assert!((geometry.side_handles[0].x - image.center().x).abs() > f32::EPSILON);
        assert!(geometry.rotation_handle.distance(geometry.side_handles[0]) > 27.0);
        let top_handle = normalised_image_position(geometry.side_handles[0], image);
        let local = unrotate_crop_position(top_handle, &crop, image.aspect_ratio());
        assert!((local[0] - 0.5).abs() < 1.0e-6);
        assert!((local[1] - crop.top).abs() < 1.0e-6);
    }

    #[test]
    fn linked_crop_drag_keeps_the_press_point_as_a_corner() {
        let start = [0.8, 0.7];
        let crop = crop_from_drag(start, [0.2, 0.1], 3.0 / 2.0, Some(1.0));

        assert!((crop.right - start[0]).abs() < f32::EPSILON);
        assert!((crop.bottom - start[1]).abs() < f32::EPSILON);
        let ratio = (crop.right - crop.left) * 1.5 / (crop.bottom - crop.top);
        assert!((ratio - 1.0).abs() < 1.0e-6);
    }

    #[test]
    fn crop_drag_covers_free_and_vertical_aspect_adjustment_paths() {
        let free = crop_from_drag([0.8, 0.7], [0.2, 0.1], 1.5, None);
        assert!((free.left - 0.2).abs() < f32::EPSILON);
        assert!((free.top - 0.1).abs() < f32::EPSILON);
        assert!((free.right - 0.8).abs() < f32::EPSILON);
        assert!((free.bottom - 0.7).abs() < f32::EPSILON);

        let vertical = crop_from_drag([0.2, 0.2], [0.25, 0.8], 1.5, Some(2.0));
        let ratio = (vertical.right - vertical.left) * 1.5 / (vertical.bottom - vertical.top);
        assert!((ratio - 2.0).abs() < 1.0e-5);
        assert!(vertical.right <= 1.0 && vertical.bottom <= 1.0);
    }

    #[test]
    fn crop_is_excluded_from_render_snapshot_until_finalised() {
        let context = egui::Context::default();
        let mut app = FocalEditorApp::new(&context);
        app.crop = Some(CropSettings {
            left: 0.2,
            top: 0.2,
            right: 0.8,
            bottom: 0.8,
            rotation_degrees: 10.0,
        });
        app.crop_mode = CropMode::Editing;
        assert!(app.adjustments().crop.is_none());
        app.crop_mode = CropMode::Applied;
        assert_eq!(app.adjustments().crop, app.crop);
    }

    #[test]
    fn logarithmic_histogram_scale_lifts_sparse_bins() {
        let linear = histogram_height_fraction(1.0, 1_000.0, DensityScale::Linear);
        let logarithmic = histogram_height_fraction(1.0, 1_000.0, DensityScale::Logarithmic);
        assert!(logarithmic > linear);
        assert!(
            (histogram_height_fraction(1_000.0, 1_000.0, DensityScale::Logarithmic) - 1.0).abs()
                < f32::EPSILON
        );
    }

    #[test]
    fn cropping_a_small_image_keeps_nearest_neighbour_presentation() {
        let source = DecodedImage {
            width: 2,
            height: 2,
            rgba: vec![255; 16],
            pixels: vec![[1.0; 3]; 4],
            alpha: vec![1.0; 4],
            input_contract: focal_core::ImageContract::SRGB_DISPLAY,
            has_transparency: false,
        };
        let cropped = Image::new(
            1,
            1,
            vec![[1.0; 3]],
            focal_core::ImageContract::SRGB_DISPLAY,
        )
        .unwrap();
        assert_eq!(
            preview_texture_options(&cropped, Some(&source)),
            egui::TextureOptions::NEAREST
        );
        assert_eq!(
            preview_texture_options(&cropped, None),
            egui::TextureOptions::LINEAR
        );
        let large_source = DecodedImage {
            width: 2_000,
            height: 2_000,
            rgba: vec![255; 16],
            pixels: vec![[1.0; 3]; 4],
            alpha: vec![1.0; 4],
            input_contract: focal_core::ImageContract::SRGB_DISPLAY,
            has_transparency: false,
        };
        assert_eq!(
            preview_texture_options(&cropped, Some(&large_source)),
            egui::TextureOptions::LINEAR
        );
    }

    #[test]
    fn repeat_export_uses_the_last_directory_and_current_source_name() {
        assert_eq!(
            default_export_path(
                std::path::Path::new("/exports"),
                std::path::Path::new("/photos/frame.jpg"),
            ),
            PathBuf::from("/exports/frame-edited.png")
        );
    }

    #[test]
    fn preview_source_is_bounded_without_upscaling_small_images() {
        let preview = bounded_preview_dimensions(4, 3, 6);
        assert!(preview[0] as usize * preview[1] as usize <= 6);
        assert!(preview[0] < 4 || preview[1] < 3);
        assert_eq!(bounded_preview_dimensions(2, 1, PREVIEW_MAX_PIXELS), [2, 1]);
        assert_eq!(PREVIEW_MAX_PIXELS, 1_000_000);
        assert_eq!(bounded_preview_dimensions(0, 0, 10), [0, 0]);
        assert_eq!(
            preview_content_dimensions([100, 50], CropMode::Editing, None),
            [100, 50]
        );
        assert_eq!(
            preview_content_dimensions(
                [100, 50],
                CropMode::Applied,
                Some(CropSettings {
                    left: 0.0,
                    top: 0.0,
                    right: 0.001,
                    bottom: 0.001,
                    rotation_degrees: 0.0,
                })
            ),
            [1, 1]
        );
    }

    #[test]
    fn rgba_bytes_clamps_channels_and_always_adds_opaque_alpha() {
        assert!(rgba_bytes(&[]).is_empty());
        assert_eq!(
            rgba_bytes(&[[-1.0, 0.5, 1.1], [0.0, 0.25, 0.75]]),
            vec![0, 128, 255, 255, 0, 64, 191, 255]
        );
        let image = rgba_image(&[[0.0, 0.5, 1.0]], 1, 1);
        assert_eq!(image.size, [1, 1]);
        assert_eq!(
            image.pixels,
            vec![Color32::from_rgba_unmultiplied(0, 128, 255, 255)]
        );
    }

    #[test]
    fn plot_and_thumbnail_adapters_preserve_dimensions_and_space() {
        let source = focal_core::scope::VectorscopeAnalysis {
            space: focal_core::scope::ScopeSpace::Cie1931,
            resolution: 2,
            density: vec![0.0; 4],
            colours: vec![[0.0; 3]; 4],
            sampled_pixels: 3,
        };
        let adapted = plot_analysis(&source);
        assert_eq!(adapted.space, ScopeSpace::Cie1931);
        assert_eq!(adapted.resolution, 2);
        assert_eq!(adapted.sampled_pixels, 3);
        let thumbnail = Thumbnail {
            width: 1,
            height: 1,
            rgba: vec![1, 2, 3, 4],
        };
        let image = thumbnail_color_image(&thumbnail);
        assert_eq!(image.size, [1, 1]);
        assert_eq!(
            image.pixels,
            vec![Color32::from_rgba_unmultiplied(1, 2, 3, 4)]
        );
    }

    #[test]
    fn interactive_preview_samples_the_full_source_while_export_keeps_it_untouched() {
        let context = egui::Context::default();
        let mut app = FocalEditorApp::new(&context);
        app.source_core = Some(Arc::new(
            Image::new(
                4,
                3,
                vec![[0.5; 3]; 12],
                focal_core::ImageContract::SRGB_DISPLAY,
            )
            .unwrap(),
        ));
        app.preview_sampling = PreviewSampling::full(2, 1);
        assert_eq!(app.preview_render_source().unwrap().width(), 4);
        assert_eq!(app.full_resolution_export_source().unwrap().width(), 4);
        assert!(Arc::ptr_eq(
            app.preview_render_source().unwrap(),
            app.full_resolution_export_source().unwrap()
        ));
        assert_eq!(app.preview_sampling.width, 2);
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
                alpha: vec![1.0],
                input_contract: focal_core::ImageContract::SRGB_DISPLAY,
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
