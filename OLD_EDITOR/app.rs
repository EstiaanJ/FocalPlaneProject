use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::SystemTime;

use eframe::egui::{
    self, Align, Color32, Layout, Rect, RichText, Sense, Stroke, StrokeKind, TextureHandle, Vec2,
};
use focal_engine::{
    CancellationToken, CropRect, DigitalParameters, DisplayImage, EditRecipe, FilmSimulation,
    FilmStockPreset, ImageLoadError, OutputSettings, ParameterDefinition, PlotDiagnostics,
    PlotDomain, PreviewImage, RESPONSE_CURVE_INPUT_STOPS, RESPONSE_CURVE_OUTPUT_MAXIMUM,
    RESPONSE_CURVE_OUTPUT_MINIMUM, ResponseCurve, SilvergrainAdapter, SpektrafilmAdapter,
    apply_geometry, bound_scene, load_scene_fixture, load_scene_fixture_cancellable,
    load_thumbnail, parameters, render_digital, render_file, render_scene,
    vendored_spektrafilm_data_dir, write_rendered_scene,
};
use focalplot::{PlotSettings, draw_plot as draw_focal_plot, plot_header};

use crate::{
    CropInteraction, PhotoRecipes, PhotoSaveTargets, PreviewCancellation, PreviewGeneration,
    PreviewTracker, StockLibrary, default_open_directory, default_stock_directory, discover_images,
    discover_stock_presets, thumbnail_window,
};

const LEFT_PANEL_WIDTH: f32 = 145.0;
const RIGHT_PANEL_WIDTH: f32 = 368.0;
const MENU_BAR_HEIGHT: f32 = 32.0;
const FILM_STRIP_HEIGHT: f32 = 138.0;
const PHOTO_PREVIEW_MAXIMUM: u32 = 4_096;
const PLOT_PREVIEW_MAXIMUM: u32 = 1_024;
const THUMBNAIL_MAXIMUM: u32 = 112;
const FILM_STRIP_ITEM_WIDTH: f32 = 120.0;

struct PhotoView {
    path: PathBuf,
    texture: TextureHandle,
    dimensions: [usize; 2],
    source_plots: PlotDiagnostics,
    output_plots: PlotDiagnostics,
}

struct FilmStripItem {
    path: PathBuf,
    thumbnail: Option<TextureHandle>,
    dimensions: Option<[usize; 2]>,
}

struct PhotoLoadMessage {
    generation: PreviewGeneration,
    path: PathBuf,
    result: Result<RenderedPreview, String>,
}

struct RenderedPreview {
    image: DisplayImage,
    source_plots: PlotDiagnostics,
    output_plots: PlotDiagnostics,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SourceCacheKey {
    path: PathBuf,
    length: u64,
    modified: Option<SystemTime>,
}

#[derive(Default)]
struct SourcePlotCache {
    entry: Option<(SourceCacheKey, PlotDiagnostics)>,
}

impl SourcePlotCache {
    fn key(path: &Path) -> Option<SourceCacheKey> {
        let metadata = path.metadata().ok()?;
        Some(SourceCacheKey {
            path: path.to_path_buf(),
            length: metadata.len(),
            modified: metadata.modified().ok(),
        })
    }

    fn get(&self, path: &Path) -> Option<PlotDiagnostics> {
        let key = Self::key(path)?;
        self.entry
            .as_ref()
            .filter(|(cached, _)| cached == &key)
            .map(|(_, diagnostics)| diagnostics.clone())
    }

    fn insert(&mut self, path: &Path, diagnostics: PlotDiagnostics) {
        self.entry = Self::key(path).map(|key| (key, diagnostics));
    }
}

struct PhotoRenderRequest {
    generation: PreviewGeneration,
    path: PathBuf,
    recipe: EditRecipe,
    film_stock: Option<FilmStockPreset>,
    cancellation: CancellationToken,
    context: egui::Context,
}

struct ThumbnailLoadMessage {
    library_generation: u64,
    thumbnail_generation: u64,
    path: PathBuf,
    result: Result<PreviewImage, ImageLoadError>,
}

struct LibraryLoadMessage {
    generation: u64,
    directory: PathBuf,
    selected_path: Option<PathBuf>,
    result: std::io::Result<Vec<PathBuf>>,
}

struct ExportMessage {
    generation: u64,
    source: PathBuf,
    destination: PathBuf,
    result: Result<(), String>,
}

pub struct FocalPlaneApp {
    recipe: EditRecipe,
    photo_recipes: PhotoRecipes,
    photo_save_targets: PhotoSaveTargets,
    preview_cancellation: PreviewCancellation,
    photo_load_tracker: PreviewTracker,
    photo_request_sender: Sender<PhotoRenderRequest>,
    photo_receiver: Receiver<PhotoLoadMessage>,
    thumbnail_sender: Sender<ThumbnailLoadMessage>,
    thumbnail_receiver: Receiver<ThumbnailLoadMessage>,
    library_sender: Sender<LibraryLoadMessage>,
    library_receiver: Receiver<LibraryLoadMessage>,
    export_sender: Sender<ExportMessage>,
    export_receiver: Receiver<ExportMessage>,
    export_generation: u64,
    exporting: bool,
    library_generation: u64,
    thumbnail_generation: u64,
    thumbnail_cancellation: CancellationToken,
    thumbnail_range: std::ops::Range<usize>,
    thumbnails_pending: usize,
    library_loading: bool,
    film_strip: Vec<FilmStripItem>,
    selected_path: Option<PathBuf>,
    photo: Option<PhotoView>,
    crop_interaction: CropInteraction,
    stock_library: StockLibrary,
    active_preset: Option<FilmStockPreset>,
    photo_presets: HashMap<PathBuf, Option<FilmStockPreset>>,
    source_plot: PlotSettings,
    output_plot: PlotSettings,
    status: String,
}

impl Default for FocalPlaneApp {
    fn default() -> Self {
        let (photo_request_sender, photo_request_receiver) = mpsc::channel();
        let (photo_sender, photo_receiver) = mpsc::channel();
        let (thumbnail_sender, thumbnail_receiver) = mpsc::channel();
        let (library_sender, library_receiver) = mpsc::channel();
        let (export_sender, export_receiver) = mpsc::channel();
        spawn_photo_render_worker(photo_request_receiver, photo_sender);
        let stock_library = discover_stock_presets(&default_stock_directory()).unwrap_or_default();

        Self {
            recipe: EditRecipe::default(),
            photo_recipes: PhotoRecipes::default(),
            photo_save_targets: PhotoSaveTargets::default(),
            preview_cancellation: PreviewCancellation::default(),
            photo_load_tracker: PreviewTracker::default(),
            photo_request_sender,
            photo_receiver,
            thumbnail_sender,
            thumbnail_receiver,
            library_sender,
            library_receiver,
            export_sender,
            export_receiver,
            export_generation: 0,
            exporting: false,
            library_generation: 0,
            thumbnail_generation: 0,
            thumbnail_cancellation: CancellationToken::new(),
            thumbnail_range: 0..0,
            thumbnails_pending: 0,
            library_loading: false,
            film_strip: Vec::new(),
            selected_path: None,
            photo: None,
            crop_interaction: CropInteraction::default(),
            stock_library,
            active_preset: None,
            photo_presets: HashMap::new(),
            source_plot: PlotSettings::default(),
            output_plot: PlotSettings::default(),
            status: "Ready — open a JPEG or PNG to begin".to_owned(),
        }
    }
}

impl FocalPlaneApp {
    /// Constructs the native application from an eframe creation context.
    ///
    /// Keeping native and headless GUI tests on this entry point ensures both
    /// exercise the same initial application state.
    #[must_use]
    pub fn new(_creation_context: &eframe::CreationContext<'_>) -> Self {
        Self::default()
    }
}

impl eframe::App for FocalPlaneApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.receive_loaded_images(ui.ctx());
        if self.crop_interaction.is_active()
            && ui.input(|input| input.key_pressed(egui::Key::Enter))
        {
            self.commit_crop();
            let context = ui.ctx().clone();
            self.request_selected_preview(&context);
        }
        self.show_menu(ui);
        self.show_film_strip(ui);
        self.show_left_panel(ui);
        self.show_right_panel(ui);
        self.show_photo_viewer(ui);
    }
}

impl FocalPlaneApp {
    fn receive_loaded_images(&mut self, context: &egui::Context) {
        while let Ok(message) = self.library_receiver.try_recv() {
            if message.generation != self.library_generation {
                continue;
            }
            self.library_loading = false;
            match message.result {
                Ok(images) if images.is_empty() => {
                    self.set_film_strip(Vec::new(), context);
                    if message.selected_path.is_none() {
                        self.preview_cancellation.cancel();
                        self.photo_load_tracker.invalidate();
                        self.photo = None;
                        self.selected_path = None;
                    }
                    self.status =
                        format!("No JPEG or PNG photos in {}", message.directory.display());
                }
                Ok(images) => {
                    let selected = message
                        .selected_path
                        .filter(|path| images.contains(path))
                        .unwrap_or_else(|| images[0].clone());
                    self.set_film_strip(images, context);
                    if self.selected_path.as_ref() != Some(&selected) {
                        self.begin_photo_load(selected, context);
                    }
                }
                Err(error) => {
                    self.status =
                        format!("Could not open {}: {error}", message.directory.display());
                }
            }
        }

        while let Ok(message) = self.photo_receiver.try_recv() {
            if !self.photo_load_tracker.complete(message.generation) {
                continue;
            }

            match message.result {
                Ok(preview) => {
                    let dimensions = preview.image.dimensions();
                    let texture = load_texture(
                        context,
                        &format!("photo:{}", message.path.display()),
                        preview.image,
                    );
                    self.status = format!(
                        "Loaded {} ({}×{})",
                        display_name(&message.path),
                        dimensions[0],
                        dimensions[1]
                    );
                    self.photo = Some(PhotoView {
                        path: message.path,
                        texture,
                        dimensions,
                        source_plots: preview.source_plots,
                        output_plots: preview.output_plots,
                    });
                }
                Err(error) => {
                    self.status = error.to_string();
                    self.photo = None;
                }
            }
        }

        while let Ok(message) = self.thumbnail_receiver.try_recv() {
            if message.library_generation != self.library_generation
                || message.thumbnail_generation != self.thumbnail_generation
            {
                continue;
            }
            self.thumbnails_pending = self.thumbnails_pending.saturating_sub(1);
            let Ok(image) = message.result else {
                continue;
            };
            let dimensions = image.dimensions();
            let texture = load_texture(
                context,
                &format!("thumbnail:{}", message.path.display()),
                image,
            );
            if let Some(item) = self
                .film_strip
                .iter_mut()
                .find(|item| item.path == message.path)
            {
                item.thumbnail = Some(texture);
                item.dimensions = Some(dimensions);
            }
        }

        while let Ok(message) = self.export_receiver.try_recv() {
            if message.generation != self.export_generation {
                continue;
            }
            self.exporting = false;
            self.status = match message.result {
                Ok(()) => {
                    self.photo_save_targets
                        .set(message.source, message.destination.clone());
                    format!("Saved {}", message.destination.display())
                }
                Err(error) => format!("Could not save {}: {error}", message.destination.display()),
            };
        }
    }

    fn show_menu(&mut self, root: &mut egui::Ui) {
        egui::Panel::top("menu_bar")
            .exact_size(MENU_BAR_HEIGHT)
            .show(root, |ui| {
                egui::MenuBar::new().ui(ui, |ui| {
                    let context = ui.ctx().clone();
                    if ui.button("Open Photo…").clicked()
                        && let Some(path) = rfd::FileDialog::new()
                            .set_directory(default_open_directory())
                            .add_filter("JPEG and PNG photos", &["jpg", "jpeg", "png"])
                            .pick_file()
                    {
                        self.open_photo(path, &context);
                    }
                    if ui.button("Open Directory…").clicked()
                        && let Some(path) = rfd::FileDialog::new()
                            .set_directory(default_open_directory())
                            .pick_folder()
                    {
                        self.open_directory(path, &context);
                    }
                    let can_save = self
                        .selected_path
                        .as_deref()
                        .and_then(|source| self.photo_save_targets.for_photo(source))
                        .is_some()
                        && !self.exporting;
                    if ui
                        .add_enabled(can_save, egui::Button::new("Save"))
                        .on_hover_text("Save to this photo's last explicit Save As destination.")
                        .clicked()
                    {
                        self.save_current(&context);
                    }
                    if ui
                        .add_enabled(
                            self.selected_path.is_some() && !self.exporting,
                            egui::Button::new("Save As…"),
                        )
                        .clicked()
                    {
                        self.save_current_as(&context);
                    }
                    ui.separator();
                    if ui.button("Help").clicked() {
                        self.status =
                            "Open a photo or directory; select photos from the film strip"
                                .to_owned();
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(RichText::new("FOCALPLANE").strong());
                        render_status(
                            ui,
                            self.library_loading,
                            self.thumbnails_pending,
                            self.photo_load_tracker.is_pending(),
                            self.exporting,
                        );
                    });
                });
            });
    }

    fn open_photo(&mut self, path: PathBuf, context: &egui::Context) {
        if let Some(directory) = path.parent() {
            self.set_film_strip(vec![path.clone()], context);
            self.begin_library_load(directory.to_path_buf(), Some(path.clone()), context);
        } else {
            self.set_film_strip(vec![path.clone()], context);
        }
        self.begin_photo_load(path, context);
    }

    fn save_current(&mut self, context: &egui::Context) {
        let Some(source) = self.selected_path.clone() else {
            self.status = "Open a photo before saving".to_owned();
            return;
        };
        let Some(destination) = self
            .photo_save_targets
            .for_photo(&source)
            .map(Path::to_path_buf)
        else {
            self.status = "Choose Save As before using Save".to_owned();
            return;
        };
        self.begin_export(source, destination, context);
    }

    fn save_current_as(&mut self, context: &egui::Context) {
        let Some(source) = self.selected_path.clone() else {
            self.status = "Open a photo before saving".to_owned();
            return;
        };
        let default_name = source
            .file_stem()
            .map(|stem| format!("{}-edited.png", stem.to_string_lossy()))
            .unwrap_or_else(|| "focalplane-edit.png".to_owned());
        let directory = source
            .parent()
            .map_or_else(default_open_directory, PathBuf::from);
        let destination = rfd::FileDialog::new()
            .set_directory(directory)
            .set_file_name(default_name)
            .add_filter("PNG image", &["png"])
            .add_filter("JPEG image", &["jpg", "jpeg"])
            .save_file();
        if let Some(destination) = destination {
            self.begin_export(source, destination, context);
        }
    }

    fn begin_export(&mut self, source: PathBuf, destination: PathBuf, context: &egui::Context) {
        let settings = match OutputSettings::from_path(&destination) {
            Ok(settings) => settings,
            Err(error) => {
                self.status = error.to_string();
                return;
            }
        };
        self.export_generation = self.export_generation.saturating_add(1);
        let generation = self.export_generation;
        self.exporting = true;
        self.status = format!("Saving {}…", destination.display());
        let recipe = self.recipe.clone();
        let film_stock = self.active_preset.clone();
        let sender = self.export_sender.clone();
        let context = context.clone();
        thread::spawn(move || {
            let result = if let Some(film_stock) = film_stock.as_ref() {
                load_scene_fixture(&source)
                    .map_err(|error| error.to_string())
                    .and_then(|scene| render_with_optional_film(&scene, &recipe, Some(film_stock)))
                    .and_then(|rendered| {
                        write_rendered_scene(&source, &destination, &rendered, settings)
                            .map_err(|error| error.to_string())
                    })
            } else {
                render_file(&source, &destination, &recipe, settings)
                    .map_err(|error| error.to_string())
            };
            let _ = sender.send(ExportMessage {
                generation,
                source,
                destination,
                result,
            });
            context.request_repaint();
        });
    }

    fn open_directory(&mut self, directory: PathBuf, context: &egui::Context) {
        self.begin_library_load(directory, None, context);
    }

    fn begin_library_load(
        &mut self,
        directory: PathBuf,
        selected_path: Option<PathBuf>,
        context: &egui::Context,
    ) {
        self.library_generation = self.library_generation.saturating_add(1);
        self.thumbnail_cancellation.cancel();
        self.thumbnails_pending = 0;
        let generation = self.library_generation;
        self.library_loading = true;
        self.status = format!("Reading {}…", directory.display());
        let sender = self.library_sender.clone();
        let context = context.clone();
        thread::spawn(move || {
            let result = discover_images(&directory);
            let _ = sender.send(LibraryLoadMessage {
                generation,
                directory,
                selected_path,
                result,
            });
            context.request_repaint();
        });
    }

    fn set_film_strip(&mut self, paths: Vec<PathBuf>, context: &egui::Context) {
        self.library_generation = self.library_generation.saturating_add(1);
        self.film_strip = paths
            .iter()
            .cloned()
            .map(|path| FilmStripItem {
                path,
                thumbnail: None,
                dimensions: None,
            })
            .collect();
        self.thumbnail_range = 0..0;
        self.request_thumbnail_window(0, 1, context);
    }

    fn request_thumbnail_window(
        &mut self,
        first_visible: usize,
        visible_count: usize,
        context: &egui::Context,
    ) {
        let range = thumbnail_window(self.film_strip.len(), first_visible, visible_count);
        if range == self.thumbnail_range {
            return;
        }
        self.thumbnail_cancellation.cancel();
        self.thumbnail_cancellation = CancellationToken::new();
        self.thumbnail_generation = self.thumbnail_generation.saturating_add(1);
        self.thumbnail_range = range.clone();
        for (index, item) in self.film_strip.iter_mut().enumerate() {
            if !range.contains(&index) {
                item.thumbnail = None;
                item.dimensions = None;
            }
        }
        let paths: Vec<_> = range
            .filter_map(|index| {
                let item = &self.film_strip[index];
                item.thumbnail.is_none().then(|| item.path.clone())
            })
            .collect();
        self.thumbnails_pending = paths.len();
        if paths.is_empty() {
            return;
        }
        let library_generation = self.library_generation;
        let thumbnail_generation = self.thumbnail_generation;
        let sender = self.thumbnail_sender.clone();
        let context = context.clone();
        let cancellation = self.thumbnail_cancellation.clone();
        thread::spawn(move || {
            for path in paths {
                if cancellation.is_cancelled() {
                    break;
                }
                let result = load_thumbnail(&path, THUMBNAIL_MAXIMUM);
                if cancellation.is_cancelled() {
                    break;
                }
                if sender
                    .send(ThumbnailLoadMessage {
                        library_generation,
                        thumbnail_generation,
                        path,
                        result,
                    })
                    .is_err()
                {
                    break;
                }
                context.request_repaint();
            }
        });
    }

    fn begin_photo_load(&mut self, path: PathBuf, context: &egui::Context) {
        if self.selected_path.as_ref() != Some(&path) {
            let previous_path = self.selected_path.clone();
            if let Some(previous_path) = &previous_path {
                self.photo_presets
                    .insert(previous_path.clone(), self.active_preset.clone());
            }
            self.recipe = self.photo_recipes.switch(
                previous_path
                    .as_deref()
                    .map(|previous| (previous, &self.recipe)),
                &path,
            );
            self.active_preset = self.photo_presets.get(&path).cloned().flatten();
            self.crop_interaction.cancel();
        }
        let cancellation = self.preview_cancellation.begin();
        let generation = self.photo_load_tracker.begin_request();
        self.selected_path = Some(path.clone());
        self.status = format!("Loading {}…", display_name(&path));

        let preview_recipe = recipe_for_preview(&self.recipe, self.crop_interaction.is_active());
        let request = PhotoRenderRequest {
            generation,
            path,
            recipe: preview_recipe,
            film_stock: self.active_preset.clone(),
            cancellation,
            context: context.clone(),
        };
        if self.photo_request_sender.send(request).is_err() {
            self.status = "Preview worker stopped unexpectedly".to_owned();
        }
    }

    fn show_film_strip(&mut self, root: &mut egui::Ui) {
        egui::Panel::bottom("film_strip")
            .exact_size(FILM_STRIP_HEIGHT)
            .resizable(false)
            .show(root, |ui| {
                ui.horizontal(|ui| {
                    ui.heading("Film Strip");
                    if !self.film_strip.is_empty() {
                        ui.label(RichText::new(format!("{} photos", self.film_strip.len())).weak());
                    }
                });

                let mut selected = None;
                let output = egui::ScrollArea::horizontal()
                    .id_salt("film_strip_scroll")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            for item in &self.film_strip {
                                let is_selected = self.selected_path.as_ref() == Some(&item.path);
                                if film_strip_item(ui, item, is_selected).clicked() {
                                    selected = Some(item.path.clone());
                                }
                            }
                        });
                    });
                let first_visible =
                    (output.state.offset.x / FILM_STRIP_ITEM_WIDTH).floor() as usize;
                let visible_count =
                    (output.inner_rect.width() / FILM_STRIP_ITEM_WIDTH).ceil() as usize + 1;
                self.request_thumbnail_window(first_visible, visible_count, ui.ctx());

                if let Some(path) = selected {
                    let context = ui.ctx().clone();
                    self.begin_photo_load(path, &context);
                }
                ui.label(RichText::new(&self.status).weak());
            });
    }

    fn show_left_panel(&mut self, root: &mut egui::Ui) {
        egui::Panel::left("navigation_and_presets")
            .default_size(LEFT_PANEL_WIDTH)
            .min_size(120.0)
            .max_size(600.0)
            .resizable(true)
            .show(root, |ui| {
                egui::Panel::top("navigator_panel")
                    .default_size(185.0)
                    .min_size(90.0)
                    .resizable(true)
                    .show(ui, |ui| {
                        ui.heading("Navigator");
                        ui.add_space(8.0);
                        let (response, painter) =
                            ui.allocate_painter(ui.available_size(), Sense::hover());
                        draw_photo_or_placeholder(
                            &painter,
                            response.rect,
                            self.photo.as_ref(),
                            "No photo",
                        );
                    });

                egui::CentralPanel::default().show(ui, |ui| {
                    ui.heading("Presets");
                    ui.add_space(4.0);
                    let mut selection = None;
                    if ui
                        .selectable_label(self.active_preset.is_none(), "Digital Neutral")
                        .clicked()
                    {
                        selection = Some(None);
                    }
                    for entry in &self.stock_library.presets {
                        let selected = self
                            .active_preset
                            .as_ref()
                            .is_some_and(|preset| preset == &entry.preset);
                        let folder = entry
                            .relative_path
                            .parent()
                            .filter(|path| !path.as_os_str().is_empty())
                            .map(|path| format!("{} / ", path.display()))
                            .unwrap_or_default();
                        if ui
                            .selectable_label(selected, format!("{folder}{}", entry.preset.name))
                            .clicked()
                        {
                            selection = Some(Some(entry.preset.clone()));
                        }
                    }
                    if let Some(preset) = selection {
                        self.apply_preset(preset, ui.ctx());
                    }
                });
            });
    }

    fn show_right_panel(&mut self, root: &mut egui::Ui) {
        egui::Panel::right("plots_and_controls")
            .default_size(RIGHT_PANEL_WIDTH)
            .min_size(220.0)
            .max_size(600.0)
            .resizable(true)
            .show(root, |ui| {
                egui::Panel::top("source_plot_panel")
                    .default_size(150.0)
                    .min_size(80.0)
                    .resizable(true)
                    .show(ui, |ui| {
                        plot_header(ui, "Source Plot", &mut self.source_plot);
                        ui.label(RichText::new("Source preview · display encoded").weak());
                        draw_plot(
                            ui,
                            ui.available_height(),
                            self.source_plot,
                            self.photo.as_ref().map(|photo| &photo.source_plots),
                        );
                    });

                egui::Panel::top("output_plot_panel")
                    .default_size(150.0)
                    .min_size(80.0)
                    .resizable(true)
                    .show(ui, |ui| {
                        plot_header(ui, "Output Plot", &mut self.output_plot);
                        ui.label(RichText::new("Rendered preview · display encoded").weak())
                            .on_hover_text(
                                "RGB histogram positions are display-encoded channel values, \
                                 not scene-luminance EV. They do not align directly with the \
                                 Response Curve's input axis.",
                            );
                        draw_plot(
                            ui,
                            ui.available_height(),
                            self.output_plot,
                            self.photo.as_ref().map(|photo| &photo.output_plots),
                        );
                    });

                egui::CentralPanel::default().show(ui, |ui| {
                    ui.heading("Controls");
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            let geometry_changed =
                                geometry_controls(ui, &mut self.recipe, &mut self.crop_interaction);
                            ui.separator();
                            let digital_changed =
                                digital_controls(ui, self.recipe.post_process_mut());
                            if geometry_changed || digital_changed {
                                self.request_selected_preview(ui.ctx());
                            }
                        });
                });
            });
    }

    fn request_selected_preview(&mut self, context: &egui::Context) {
        if let Some(path) = self.selected_path.clone() {
            self.begin_photo_load(path, context);
        }
    }

    fn apply_preset(&mut self, preset: Option<FilmStockPreset>, context: &egui::Context) {
        if let Some(preset) = &preset {
            *self.recipe.pre_process_mut() = preset.pre_process.clone();
            *self.recipe.post_process_mut() = preset.post_process.clone();
        } else {
            *self.recipe.pre_process_mut() = DigitalParameters::default();
            *self.recipe.post_process_mut() = DigitalParameters::default();
        }
        self.recipe.set_film_enabled(false);
        self.active_preset = preset;
        if let Some(path) = &self.selected_path {
            self.photo_presets
                .insert(path.clone(), self.active_preset.clone());
        }
        self.request_selected_preview(context);
    }

    fn commit_crop(&mut self) {
        commit_crop(&mut self.recipe, &mut self.crop_interaction);
    }

    fn show_photo_viewer(&mut self, root: &mut egui::Ui) {
        egui::CentralPanel::default().show(root, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Photo Viewer");
                if let Some(photo) = &self.photo {
                    ui.label(RichText::new(display_name(&photo.path)).weak());
                }
            });
            let available = ui.available_size();
            let sense = if self.crop_interaction.is_active() {
                Sense::click_and_drag()
            } else {
                Sense::hover()
            };
            let (response, painter) = ui.allocate_painter(available, sense);
            let viewer_rect = response.rect.shrink(8.0);
            draw_photo_or_placeholder(
                &painter,
                viewer_rect,
                self.photo.as_ref(),
                "Open a JPEG or PNG to start editing",
            );
            if self.crop_interaction.is_active()
                && let Some(photo) = &self.photo
            {
                let image_rect = fitted_rect(viewer_rect.shrink(3.0), photo.dimensions);
                update_crop_drag(&response, image_rect, &mut self.crop_interaction);
                if let Some(crop) = self.crop_interaction.draft() {
                    draw_crop_overlay(&painter, image_rect, crop);
                }
            }
        });
    }
}

fn spawn_photo_render_worker(
    receiver: Receiver<PhotoRenderRequest>,
    sender: Sender<PhotoLoadMessage>,
) {
    thread::spawn(move || {
        let mut source_plot_cache = SourcePlotCache::default();
        while let Ok(mut request) = receiver.recv() {
            while let Ok(newer) = receiver.try_recv() {
                request = newer;
            }
            let result = render_preview(
                &request.path,
                &request.recipe,
                request.film_stock.as_ref(),
                &request.cancellation,
                &mut source_plot_cache,
            );
            let context = request.context;
            if sender
                .send(PhotoLoadMessage {
                    generation: request.generation,
                    path: request.path,
                    result,
                })
                .is_err()
            {
                break;
            }
            context.request_repaint();
        }
    });
}

fn render_preview(
    path: &Path,
    recipe: &EditRecipe,
    film_stock: Option<&FilmStockPreset>,
    cancellation: &CancellationToken,
    source_plot_cache: &mut SourcePlotCache,
) -> Result<RenderedPreview, String> {
    let scene =
        load_scene_fixture_cancellable(path, cancellation).map_err(|error| error.to_string())?;
    if cancellation.is_cancelled() {
        return Err("preview rendering was cancelled".to_owned());
    }
    let source_plots = if let Some(cached) = source_plot_cache.get(path) {
        cached
    } else {
        let source_image = DisplayImage::from_linear_srgb_scene(&scene)
            .and_then(|display| display.bounded(PHOTO_PREVIEW_MAXIMUM))
            .map_err(|error| error.to_string())?;
        let diagnostics = PlotDiagnostics::from_display_image_cancellable(
            &source_image,
            PlotDomain::Source,
            PLOT_PREVIEW_MAXIMUM,
            cancellation,
        )
        .map_err(|error| error.to_string())?;
        source_plot_cache.insert(path, diagnostics.clone());
        diagnostics
    };
    let render_bound =
        if film_stock.is_some_and(|stock| stock.simulation == FilmSimulation::Silvergrain) {
            640
        } else {
            PHOTO_PREVIEW_MAXIMUM
        };
    let preview_scene = bound_scene(&scene, render_bound).map_err(|error| error.to_string())?;
    let rendered = render_with_optional_film(&preview_scene, recipe, film_stock)?;
    if cancellation.is_cancelled() {
        return Err("preview rendering was cancelled".to_owned());
    }
    let image = DisplayImage::from_linear_srgb_scene(&rendered)
        .and_then(|display| display.bounded(PHOTO_PREVIEW_MAXIMUM))
        .map_err(|error| error.to_string())?;
    let output_plots = PlotDiagnostics::from_display_image_cancellable(
        &image,
        PlotDomain::Output,
        PLOT_PREVIEW_MAXIMUM,
        cancellation,
    )
    .map_err(|error| error.to_string())?;
    Ok(RenderedPreview {
        source_plots,
        output_plots,
        image,
    })
}

fn render_with_optional_film(
    scene: &focal_engine::SceneImage,
    recipe: &EditRecipe,
    film_stock: Option<&FilmStockPreset>,
) -> Result<focal_engine::SceneImage, String> {
    let Some(film_stock) = film_stock else {
        return render_scene(scene, recipe).map_err(|error| error.to_string());
    };
    let geometry = apply_geometry(
        scene,
        recipe.geometry().crop(),
        recipe.geometry().rotation_degrees(),
    )
    .map_err(|error| error.to_string())?;
    let pre_film =
        render_digital(&geometry, recipe.pre_process()).map_err(|error| error.to_string())?;
    let filmed = match film_stock.simulation {
        FilmSimulation::None => pre_film,
        FilmSimulation::Spektrafilm => {
            let adapter =
                SpektrafilmAdapter::new(&film_stock.film, &vendored_spektrafilm_data_dir())
                    .map_err(|error| error.to_string())?;
            adapter
                .render(&pre_film)
                .map_err(|error| error.to_string())?
        }
        FilmSimulation::Silvergrain => {
            let adapter = SilvergrainAdapter::new(&film_stock.silvergrain)
                .map_err(|error| error.to_string())?;
            adapter
                .render(&pre_film)
                .map_err(|error| error.to_string())?
        }
    };
    render_digital(&filmed, recipe.post_process()).map_err(|error| error.to_string())
}

fn load_texture(context: &egui::Context, name: &str, image: PreviewImage) -> TextureHandle {
    let colour_image = egui::ColorImage::from_rgba_unmultiplied(image.dimensions(), image.rgba8());
    context.load_texture(name, colour_image, egui::TextureOptions::LINEAR)
}

fn film_strip_item(ui: &mut egui::Ui, item: &FilmStripItem, selected: bool) -> egui::Response {
    let desired_size = Vec2::new(112.0, 78.0);
    let (rect, response) = ui.allocate_exact_size(desired_size, Sense::click());
    let stroke = if selected {
        Stroke::new(2.0, ui.visuals().selection.stroke.color)
    } else {
        Stroke::new(1.0, Color32::from_gray(62))
    };
    ui.painter()
        .rect_filled(rect, 3.0, Color32::from_rgb(24, 25, 27));

    if let (Some(texture), Some(dimensions)) = (&item.thumbnail, item.dimensions) {
        let image_rect = fitted_rect(rect.shrink(3.0), dimensions);
        ui.painter().image(
            texture.id(),
            image_rect,
            Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
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
        .rect_stroke(rect, 3.0, stroke, StrokeKind::Inside);
    response.on_hover_text(display_name(&item.path))
}

fn render_status(
    ui: &mut egui::Ui,
    library_loading: bool,
    thumbnails_pending: usize,
    preview_loading: bool,
    exporting: bool,
) {
    let (progress, text) = if exporting {
        (0.75, "Saving…")
    } else if preview_loading {
        (0.55, "Rendering…")
    } else if library_loading {
        (0.25, "Reading folder…")
    } else if thumbnails_pending > 0 {
        (0.35, "Loading film strip…")
    } else {
        (1.0, "Ready")
    };
    ui.add(
        egui::ProgressBar::new(progress)
            .desired_width(132.0)
            .text(text)
            .animate(progress < 1.0),
    );
}

fn draw_photo_or_placeholder(
    painter: &egui::Painter,
    rect: Rect,
    photo: Option<&PhotoView>,
    placeholder: &str,
) {
    painter.rect_filled(rect, 4.0, Color32::from_rgb(24, 25, 27));
    painter.rect_stroke(
        rect,
        4.0,
        Stroke::new(1.0, Color32::from_gray(62)),
        StrokeKind::Inside,
    );

    if let Some(photo) = photo {
        painter.image(
            photo.texture.id(),
            fitted_rect(rect.shrink(3.0), photo.dimensions),
            Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
            Color32::WHITE,
        );
    } else {
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            placeholder,
            egui::FontId::proportional(16.0),
            Color32::from_gray(145),
        );
    }
}

fn fitted_rect(bounds: Rect, dimensions: [usize; 2]) -> Rect {
    let image_size = Vec2::new(dimensions[0] as f32, dimensions[1] as f32);
    let scale = (bounds.width() / image_size.x).min(bounds.height() / image_size.y);
    Rect::from_center_size(bounds.center(), image_size * scale)
}

fn display_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

fn digital_controls(ui: &mut egui::Ui, parameters: &mut DigitalParameters) -> bool {
    let mut changed = false;
    changed |= parameter_control(ui, parameters::TEMPERATURE, &mut parameters.temperature);
    changed |= parameter_control(ui, parameters::TINT, &mut parameters.tint);
    changed |= parameter_control(ui, parameters::EXPOSURE, &mut parameters.exposure_ev);
    changed |= parameter_control(ui, parameters::CONTRAST, &mut parameters.contrast);
    changed |= response_curve_control(ui, &mut parameters.response_curve);
    ui.add_enabled_ui(false, |ui| {
        parameter_control(
            ui,
            parameters::LOCAL_CONTRAST,
            &mut parameters.local_contrast,
        );
    })
    .response
    .on_disabled_hover_text("Local Contrast is not implemented yet.");
    changed |= parameter_control(ui, parameters::VIBRANCE, &mut parameters.vibrance);
    changed |= parameter_control(ui, parameters::SATURATION, &mut parameters.saturation);
    changed
}

fn response_curve_control(ui: &mut egui::Ui, curve: &mut ResponseCurve) -> bool {
    const DISPLAY_ADJUSTMENT_STOPS: f32 = 2.0;

    ui.label("Response curve");
    let mut changed = false;
    ui.horizontal(|ui| {
        if ui
            .add_sized([20.0, 20.0], egui::Button::new("↺"))
            .on_hover_text("Reset Response curve to identity.")
            .clicked()
        {
            *curve = ResponseCurve::default();
            changed = true;
        }

        let desired = Vec2::new((ui.available_width() - 4.0).max(120.0), 112.0);
        let (rect, response) = ui.allocate_exact_size(desired, Sense::click_and_drag());
        let painter = ui.painter_at(rect);
        let plot_rect = Rect::from_min_max(
            rect.min + egui::vec2(28.0, 8.0),
            rect.max - egui::vec2(8.0, 19.0),
        );
        painter.rect_filled(rect, 3.0, Color32::from_rgb(24, 25, 27));
        painter.rect_stroke(
            rect,
            3.0,
            Stroke::new(1.0, Color32::from_gray(62)),
            StrokeKind::Inside,
        );

        let to_screen = |input: f32, output: f32| {
            let adjustment = output - input;
            egui::pos2(
                egui::remap(
                    input,
                    RESPONSE_CURVE_INPUT_STOPS[0]..=RESPONSE_CURVE_INPUT_STOPS[4],
                    plot_rect.left()..=plot_rect.right(),
                ),
                egui::remap(
                    adjustment,
                    -DISPLAY_ADJUSTMENT_STOPS..=DISPLAY_ADJUSTMENT_STOPS,
                    plot_rect.bottom()..=plot_rect.top(),
                ),
            )
        };
        for input in RESPONSE_CURVE_INPUT_STOPS {
            let x = to_screen(input, input).x;
            painter.line_segment(
                [
                    egui::pos2(x, plot_rect.top()),
                    egui::pos2(x, plot_rect.bottom()),
                ],
                Stroke::new(1.0, Color32::from_gray(45)),
            );
        }
        for adjustment in [-2.0, -1.0, 0.0, 1.0, 2.0] {
            let y = egui::remap(
                adjustment,
                -DISPLAY_ADJUSTMENT_STOPS..=DISPLAY_ADJUSTMENT_STOPS,
                plot_rect.bottom()..=plot_rect.top(),
            );
            painter.line_segment(
                [
                    egui::pos2(plot_rect.left(), y),
                    egui::pos2(plot_rect.right(), y),
                ],
                Stroke::new(
                    if adjustment == 0.0 { 1.25 } else { 1.0 },
                    Color32::from_gray(if adjustment == 0.0 { 75 } else { 45 }),
                ),
            );
        }
        for (input, label) in [(-8.0, "−8"), (0.0, "0"), (8.0, "+8")] {
            painter.text(
                egui::pos2(to_screen(input, input).x, plot_rect.bottom() + 3.0),
                egui::Align2::CENTER_TOP,
                label,
                egui::FontId::proportional(9.0),
                Color32::from_gray(130),
            );
        }
        for (adjustment, label) in [(2.0, "+2"), (0.0, "0"), (-2.0, "−2")] {
            let y = egui::remap(
                adjustment,
                -DISPLAY_ADJUSTMENT_STOPS..=DISPLAY_ADJUSTMENT_STOPS,
                plot_rect.bottom()..=plot_rect.top(),
            );
            painter.text(
                egui::pos2(plot_rect.left() - 4.0, y),
                egui::Align2::RIGHT_CENTER,
                label,
                egui::FontId::proportional(9.0),
                Color32::from_gray(130),
            );
        }
        painter.text(
            egui::pos2(plot_rect.left(), rect.top() + 1.0),
            egui::Align2::LEFT_TOP,
            "Δ EV",
            egui::FontId::proportional(9.0),
            Color32::from_gray(145),
        );
        painter.text(
            egui::pos2(plot_rect.right(), rect.bottom() - 1.0),
            egui::Align2::RIGHT_BOTTOM,
            "Input EV",
            egui::FontId::proportional(9.0),
            Color32::from_gray(145),
        );

        let points: Vec<_> = RESPONSE_CURVE_INPUT_STOPS
            .iter()
            .copied()
            .zip(curve.output_stops().iter().copied())
            .map(|(input, output)| to_screen(input, output))
            .collect();
        for pair in points.windows(2) {
            painter.line_segment(
                [pair[0], pair[1]],
                Stroke::new(1.5, Color32::from_rgb(220, 180, 92)),
            );
        }
        for point in points {
            painter.circle_filled(point, 3.5, Color32::from_rgb(238, 211, 145));
        }

        if (response.dragged() || response.clicked())
            && let Some(position) = response.interact_pointer_pos()
        {
            let input =
                egui::remap_clamp(position.x, plot_rect.left()..=plot_rect.right(), -8.0..=8.0);
            let index = RESPONSE_CURVE_INPUT_STOPS
                .iter()
                .enumerate()
                .min_by(|(_, left), (_, right)| {
                    (*left - input).abs().total_cmp(&(*right - input).abs())
                })
                .map(|(index, _)| index)
                .expect("response curve has fixed points");
            let adjustment = egui::remap_clamp(
                position.y,
                plot_rect.bottom()..=plot_rect.top(),
                -DISPLAY_ADJUSTMENT_STOPS..=DISPLAY_ADJUSTMENT_STOPS,
            );
            let mut output = (RESPONSE_CURVE_INPUT_STOPS[index] + adjustment).clamp(
                RESPONSE_CURVE_OUTPUT_MINIMUM,
                RESPONSE_CURVE_OUTPUT_MAXIMUM,
            );
            if index > 0 {
                output = output.max(curve.output_stops()[index - 1]);
            }
            if index + 1 < RESPONSE_CURVE_INPUT_STOPS.len() {
                output = output.min(curve.output_stops()[index + 1]);
            }
            if curve.set_output_stop(index, output).is_ok() {
                changed = true;
            }
        }
        response.on_hover_text(
            "Shapes scene luminance before display rendering.\n\
             X axis: input exposure in stops relative to 18% grey (left is darker; right is brighter).\n\
             Y axis: output adjustment in stops. Zero is no change; up brightens and down darkens.\n\
             Drag a point vertically. It influences both neighbouring intervals.\n\
             The curve remains monotonic to prevent tone reversals.\n\
             Output histograms show display-encoded RGB channels, so their horizontal positions \
             do not correspond directly to this scene-luminance EV axis.",
        );
    });
    ui.add_space(3.0);
    changed
}

fn parameter_control(ui: &mut egui::Ui, definition: ParameterDefinition, value: &mut f32) -> bool {
    ui.label(definition.label);
    let mut changed = false;
    ui.horizontal(|ui| {
        if ui
            .add_sized([20.0, 20.0], egui::Button::new("↺"))
            .on_hover_text(format!("Reset {} to its default.", definition.label))
            .clicked()
        {
            *value = definition.default;
            changed = true;
        }
        let slider_width = (ui.available_width() - 72.0).max(80.0);
        let slider = ui
            .add_sized(
                [slider_width, 20.0],
                egui::Slider::new(value, definition.minimum..=definition.maximum).show_value(false),
            )
            .on_hover_text(format!(
                "{}\nUnit: {}.",
                definition.tooltip, definition.unit
            ));
        changed |= slider.changed();

        let number = ui
            .add(
                egui::DragValue::new(value)
                    .speed(definition.fine_step)
                    .max_decimals(2),
            )
            .on_hover_text("Drag for fine adjustment.");
        changed |= number.changed();
    });
    ui.add_space(3.0);
    changed
}

fn geometry_controls(
    ui: &mut egui::Ui,
    recipe: &mut EditRecipe,
    crop_interaction: &mut CropInteraction,
) -> bool {
    ui.label(RichText::new("Geometry").strong());
    let mut changed = false;
    let crop_button = ui
        .selectable_label(crop_interaction.is_active(), "Crop")
        .on_hover_text(if crop_interaction.is_active() {
            "Apply the crop drawn on the photo."
        } else {
            "Show the full source and draw a crop on the photo."
        });
    if crop_button.clicked() {
        if crop_interaction.is_active() {
            commit_crop(recipe, crop_interaction);
        } else {
            crop_interaction.enter(recipe.geometry().crop());
        }
        changed = true;
    }

    let mut rotation = recipe.geometry().rotation_degrees();
    if parameter_control(ui, parameters::ROTATION, &mut rotation) {
        recipe
            .geometry_mut()
            .set_rotation_degrees(rotation)
            .expect("rotation control uses the engine parameter range");
        changed = true;
    }
    changed
}

fn recipe_for_preview(recipe: &EditRecipe, editing_crop: bool) -> EditRecipe {
    let mut preview_recipe = recipe.clone();
    if editing_crop {
        preview_recipe.geometry_mut().set_crop(CropRect::FULL);
        preview_recipe
            .geometry_mut()
            .set_rotation_degrees(0.0)
            .expect("zero rotation is always valid");
    }
    preview_recipe
}

fn commit_crop(recipe: &mut EditRecipe, crop_interaction: &mut CropInteraction) {
    if let Some(crop) = crop_interaction.commit() {
        recipe.geometry_mut().set_crop(crop);
    }
}

fn update_crop_drag(
    response: &egui::Response,
    image_rect: Rect,
    crop_interaction: &mut CropInteraction,
) {
    if response.drag_started()
        && let Some(position) = response.interact_pointer_pos()
    {
        crop_interaction.begin_drag(normalised_position(image_rect, position));
    }
    if response.dragged()
        && let Some(position) = response.interact_pointer_pos()
    {
        crop_interaction.update_drag(normalised_position(image_rect, position));
    }
    if response.drag_stopped() {
        crop_interaction.end_drag();
    }
}

fn normalised_position(rect: Rect, position: egui::Pos2) -> [f32; 2] {
    [
        ((position.x - rect.left()) / rect.width()).clamp(0.0, 1.0),
        ((position.y - rect.top()) / rect.height()).clamp(0.0, 1.0),
    ]
}

fn draw_crop_overlay(painter: &egui::Painter, image_rect: Rect, crop: CropRect) {
    let crop_rect = Rect::from_min_max(
        egui::pos2(
            image_rect.left() + image_rect.width() * crop.left(),
            image_rect.top() + image_rect.height() * crop.top(),
        ),
        egui::pos2(
            image_rect.left() + image_rect.width() * crop.right(),
            image_rect.top() + image_rect.height() * crop.bottom(),
        ),
    );
    let shade = Color32::from_black_alpha(145);
    for rect in [
        Rect::from_min_max(
            image_rect.min,
            egui::pos2(image_rect.right(), crop_rect.top()),
        ),
        Rect::from_min_max(
            egui::pos2(image_rect.left(), crop_rect.bottom()),
            image_rect.max,
        ),
        Rect::from_min_max(
            egui::pos2(image_rect.left(), crop_rect.top()),
            egui::pos2(crop_rect.left(), crop_rect.bottom()),
        ),
        Rect::from_min_max(
            egui::pos2(crop_rect.right(), crop_rect.top()),
            egui::pos2(image_rect.right(), crop_rect.bottom()),
        ),
    ] {
        painter.rect_filled(rect, 0.0, shade);
    }
    painter.rect_stroke(
        crop_rect,
        0.0,
        Stroke::new(2.0, Color32::WHITE),
        StrokeKind::Inside,
    );
}

fn draw_plot(
    ui: &mut egui::Ui,
    height: f32,
    settings: PlotSettings,
    plots: Option<&PlotDiagnostics>,
) {
    draw_focal_plot(
        ui,
        height,
        settings,
        plots.map(PlotDiagnostics::histogram),
        plots.map(PlotDiagnostics::waveform),
        plots.map(PlotDiagnostics::parade),
        plots.map(PlotDiagnostics::vectorscope),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use focalplot::PlotMode;

    fn fixture(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join("test-image")
            .join(name)
    }

    #[test]
    fn preview_render_keeps_source_and_output_domains_separate() {
        let mut recipe = EditRecipe::default();
        recipe.post_process_mut().exposure_ev = 1.0;
        recipe.post_process_mut().temperature = 25.0;
        recipe.post_process_mut().contrast = 30.0;
        recipe.post_process_mut().saturation = 20.0;
        recipe.post_process_mut().vibrance = 15.0;
        let preview = render_preview(
            &fixture("test.png"),
            &recipe,
            None,
            &CancellationToken::new(),
            &mut SourcePlotCache::default(),
        )
        .expect("preview");

        assert_eq!(preview.source_plots.domain(), PlotDomain::Source);
        assert_eq!(preview.output_plots.domain(), PlotDomain::Output);
        assert_ne!(
            preview.source_plots.histogram(),
            preview.output_plots.histogram()
        );
    }

    #[test]
    fn preview_render_honours_pre_cancelled_requests() {
        let cancellation = CancellationToken::new();
        cancellation.cancel();
        let error = render_preview(
            &fixture("test.png"),
            &EditRecipe::default(),
            None,
            &cancellation,
            &mut SourcePlotCache::default(),
        )
        .err()
        .expect("cancelled preview");
        assert!(error.contains("cancel"));
    }

    #[test]
    fn a_focalplane_stock_runs_spektrafilm_in_the_editor_preview() {
        let preset = FilmStockPreset::default();
        let preview = render_preview(
            &fixture("test.png"),
            &EditRecipe::default(),
            Some(&preset),
            &CancellationToken::new(),
            &mut SourcePlotCache::default(),
        )
        .expect("film-stock preview");

        assert_eq!(preview.output_plots.domain(), PlotDomain::Output);
        assert_ne!(
            preview.source_plots.histogram(),
            preview.output_plots.histogram()
        );
    }

    #[test]
    fn a_silvergrain_stock_runs_the_monochrome_provider_in_the_editor_preview() {
        let preset = FilmStockPreset {
            simulation: FilmSimulation::Silvergrain,
            ..FilmStockPreset::default()
        };
        let preview = render_preview(
            &fixture("test.png"),
            &EditRecipe::default(),
            Some(&preset),
            &CancellationToken::new(),
            &mut SourcePlotCache::default(),
        )
        .expect("Silvergrain preview");

        assert_ne!(
            preview.source_plots.histogram(),
            preview.output_plots.histogram()
        );
    }

    #[test]
    fn film_stock_selection_is_isolated_per_photo() {
        let mut app = FocalPlaneApp::default();
        let first = PathBuf::from("first.png");
        let second = PathBuf::from("second.png");
        let first_stock = FilmStockPreset {
            name: "First stock".into(),
            ..FilmStockPreset::default()
        };
        let second_stock = FilmStockPreset {
            name: "Second stock".into(),
            ..FilmStockPreset::default()
        };
        app.selected_path = Some(first.clone());
        app.active_preset = Some(first_stock.clone());
        app.photo_presets
            .insert(second.clone(), Some(second_stock.clone()));

        app.begin_photo_load(second, &egui::Context::default());

        assert_eq!(
            app.photo_presets
                .get(&first)
                .and_then(Option::as_ref)
                .map(|preset| preset.name.as_str()),
            Some("First stock")
        );
        assert_eq!(
            app.active_preset
                .as_ref()
                .map(|preset| preset.name.as_str()),
            Some("Second stock")
        );
    }

    #[test]
    fn applying_a_saved_stock_installs_its_complete_editor_pipeline() {
        let mut app = FocalPlaneApp::default();
        let preset = FilmStockPreset {
            name: "Complete stock".into(),
            pre_process: DigitalParameters {
                exposure_ev: 0.5,
                ..DigitalParameters::default()
            },
            post_process: DigitalParameters {
                saturation: 12.0,
                ..DigitalParameters::default()
            },
            ..FilmStockPreset::default()
        };

        app.apply_preset(Some(preset.clone()), &egui::Context::default());

        assert_eq!(app.recipe.pre_process(), &preset.pre_process);
        assert_eq!(app.recipe.post_process(), &preset.post_process);
        assert_eq!(app.active_preset, Some(preset));
    }

    #[test]
    fn source_plot_cache_reuses_only_the_same_unchanged_file() {
        let first_path = fixture("test.png");
        let second_path = fixture("pure_chrome.png");
        let image = DisplayImage::from_linear_srgb_scene(
            &load_scene_fixture_cancellable(&first_path, &CancellationToken::new())
                .expect("fixture"),
        )
        .expect("display");
        let plots =
            PlotDiagnostics::from_display_image(&image, PlotDomain::Source, 1_024).expect("plots");
        let mut cache = SourcePlotCache::default();

        assert!(cache.get(&first_path).is_none());
        cache.insert(&first_path, plots.clone());
        assert_eq!(cache.get(&first_path), Some(plots));
        assert!(cache.get(&second_path).is_none());
    }

    #[test]
    fn fitted_rectangle_preserves_aspect_ratio_and_centres_image() {
        let bounds = Rect::from_min_size(egui::pos2(10.0, 20.0), Vec2::new(200.0, 100.0));
        let wide = fitted_rect(bounds, [400, 100]);
        assert_eq!(wide.width(), 200.0);
        assert_eq!(wide.height(), 50.0);
        assert_eq!(wide.center(), bounds.center());

        let tall = fitted_rect(bounds, [100, 400]);
        assert_eq!(tall.width(), 25.0);
        assert_eq!(tall.height(), 100.0);
        assert_eq!(tall.center(), bounds.center());
    }

    #[test]
    fn crop_editing_uses_the_unrotated_full_source_without_changing_the_recipe() {
        let crop = CropRect::new(0.1, 0.2, 0.8, 0.9).expect("crop");
        let mut recipe = EditRecipe::default();
        recipe.geometry_mut().set_crop(crop);
        recipe
            .geometry_mut()
            .set_rotation_degrees(27.0)
            .expect("rotation");

        let preview = recipe_for_preview(&recipe, true);

        assert_eq!(preview.geometry().crop(), CropRect::FULL);
        assert_eq!(preview.geometry().rotation_degrees(), 0.0);
        assert_eq!(recipe.geometry().crop(), crop);
        assert_eq!(recipe.geometry().rotation_degrees(), 27.0);
    }

    #[test]
    fn plot_settings_default_to_independent_full_rgb_log_histograms() {
        let source = PlotSettings::default();
        let output = PlotSettings {
            mode: PlotMode::Vectorscope,
            channels: [false, true, true],
            ..PlotSettings::default()
        };

        assert_eq!(source.mode.label(), "Histogram");
        assert_eq!(PlotMode::Waveform.label(), "Waveform");
        assert_eq!(PlotMode::Parade.label(), "RGB parade");
        assert_eq!(PlotMode::Vectorscope.label(), "Vectorscope");
        assert_eq!(source.channels, [true; 3]);
        assert!(source.logarithmic);
        assert_ne!(source, output);
    }

    #[test]
    fn display_name_handles_normal_and_root_paths() {
        assert_eq!(display_name(Path::new("/tmp/photo.png")), "photo.png");
        assert_eq!(display_name(Path::new("/")), "/");
    }
}
