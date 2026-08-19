#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::excessive_precision,
    clippy::unreadable_literal
)]

use std::{path::PathBuf, sync::Arc};

use eframe::egui::{
    self, Color32, CornerRadius, Pos2, Rect, Sense, Stroke, StrokeKind, TextureHandle, Vec2,
};

use crate::{
    loader::{AnalysisKind, ImageLoader, LoadEvent, LoadedImage},
    vectorscope::{
        AnalysisRegion, DensityScale, ScopeSpace, VectorscopeAnalysis, render_trace, ring_colour,
    },
};

const BACKGROUND: Color32 = Color32::from_rgb(3, 4, 5);
const SCOPE_BACKGROUND: Color32 = Color32::from_rgb(8, 9, 10);
const GRID: Color32 = Color32::from_rgba_premultiplied(110, 118, 124, 48);
const SELECTION: Color32 = Color32::from_rgb(255, 205, 80);
const HOVER: Color32 = Color32::from_rgb(240, 245, 250);
const CIE_X_MAX: f32 = 0.8;
const CIE_Y_MAX: f32 = 0.9;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ScopeTab {
    Cie1931,
    Ryb,
}

impl ScopeTab {
    fn space(self) -> ScopeSpace {
        match self {
            Self::Cie1931 => ScopeSpace::Cie1931,
            Self::Ryb => ScopeSpace::Ryb,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Cie1931 => "CIE 1931",
            Self::Ryb => "RYB",
        }
    }
}

#[derive(Default)]
struct ScopeAnalyses {
    full: Option<VectorscopeAnalysis>,
    rectangle: Option<VectorscopeAnalysis>,
    hover: Option<VectorscopeAnalysis>,
}

#[derive(Default)]
struct ScopeTextures {
    full: Option<TextureHandle>,
    rectangle: Option<TextureHandle>,
    hover: Option<TextureHandle>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PendingAnalysis {
    id: u64,
    space: ScopeSpace,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct ImageRect {
    min: [f32; 2],
    max: [f32; 2],
}

impl ImageRect {
    fn new(a: [f32; 2], b: [f32; 2]) -> Self {
        Self {
            min: [a[0].min(b[0]), a[1].min(b[1])],
            max: [a[0].max(b[0]), a[1].max(b[1])],
        }
    }

    fn clamp(self, width: f32, height: f32) -> Self {
        let min_x = self.min[0].clamp(0.0, width);
        let min_y = self.min[1].clamp(0.0, height);
        let max_x = self.max[0].clamp(min_x, width);
        let max_y = self.max[1].clamp(min_y, height);
        Self {
            min: [min_x, min_y],
            max: [max_x, max_y],
        }
    }

    fn contains(self, point: [f32; 2]) -> bool {
        point[0] >= self.min[0]
            && point[0] <= self.max[0]
            && point[1] >= self.min[1]
            && point[1] <= self.max[1]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RectHandle {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

#[derive(Clone, Copy, Debug)]
enum RectInteraction {
    Draw {
        start: [f32; 2],
    },
    Move {
        start: [f32; 2],
        original: ImageRect,
    },
    Resize {
        handle: RectHandle,
        original: ImageRect,
    },
}

#[derive(Clone, Copy)]
enum ScopeHitArea {
    Ryb { centre: Pos2, radius: f32 },
    Cie { chart: Rect },
}

impl ScopeHitArea {
    fn coordinate(&self, pointer: Pos2) -> Option<[f32; 2]> {
        match self {
            Self::Ryb { centre, radius } => {
                let delta = pointer - *centre;
                if delta.length() > *radius {
                    return None;
                }
                Some([
                    (delta.x / (radius * 2.0) + 0.5).clamp(0.0, 1.0),
                    (delta.y / (radius * 2.0) + 0.5).clamp(0.0, 1.0),
                ])
            }
            Self::Cie { chart } if chart.contains(pointer) => Some([
                ((pointer.x - chart.left()) / chart.width()).clamp(0.0, 1.0),
                ((pointer.y - chart.top()) / chart.height()).clamp(0.0, 1.0),
            ]),
            Self::Cie { .. } => None,
        }
    }
}

pub struct BetterPlotsApp {
    loader: ImageLoader,
    latest_request: u64,
    loading: bool,
    error: Option<String>,
    image_path: Option<PathBuf>,
    image_texture: Option<TextureHandle>,
    ryb_analyses: ScopeAnalyses,
    cie_analyses: ScopeAnalyses,
    ryb_textures: ScopeTextures,
    cie_textures: ScopeTextures,
    active_scope: ScopeTab,
    dimensions: Option<[u32; 2]>,
    sampled_pixels: usize,
    trace_intensity: f32,
    dot_sharpness: f32,
    density_scale: DensityScale,
    loaded: Option<Arc<LoadedImage>>,
    selection_tool: bool,
    selection: Option<ImageRect>,
    rect_interaction: Option<RectInteraction>,
    hover_position: Option<[f32; 2]>,
    hover_radius: f32,
    last_hover_region: Option<(ScopeSpace, AnalysisRegion)>,
    hover_request: Option<PendingAnalysis>,
    rectangle_request: Option<PendingAnalysis>,
    reverse_texture: Option<TextureHandle>,
    reverse_request: Option<u64>,
    reverse_query: Option<(ScopeSpace, [f32; 2], f32)>,
    reverse_radius: f32,
}

impl BetterPlotsApp {
    pub fn new(context: &egui::Context) -> Self {
        configure_visuals(context);
        let mut loader = ImageLoader::new();
        let initial_path = std::env::args_os().nth(1).map(PathBuf::from);
        let latest_request = initial_path
            .as_ref()
            .map_or(0, |path| loader.open(path.clone()));
        Self {
            loader,
            latest_request,
            loading: initial_path.is_some(),
            error: None,
            image_path: None,
            image_texture: None,
            ryb_analyses: ScopeAnalyses::default(),
            cie_analyses: ScopeAnalyses::default(),
            ryb_textures: ScopeTextures::default(),
            cie_textures: ScopeTextures::default(),
            active_scope: ScopeTab::Cie1931,
            dimensions: None,
            sampled_pixels: 0,
            trace_intensity: 1.0,
            dot_sharpness: 0.55,
            density_scale: DensityScale::Logarithmic,
            loaded: None,
            selection_tool: false,
            selection: None,
            rect_interaction: None,
            hover_position: None,
            hover_radius: 0.5,
            last_hover_region: None,
            hover_request: None,
            rectangle_request: None,
            reverse_texture: None,
            reverse_request: None,
            reverse_query: None,
            reverse_radius: 0.012,
        }
    }

    fn open_image(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Images", &["png", "jpg", "jpeg"])
            .pick_file()
        else {
            return;
        };
        self.latest_request = self.loader.open(path);
        self.loading = true;
        self.error = None;
        self.clear_reverse_highlight();
    }

    fn poll_loader(&mut self, context: &egui::Context) {
        for event in self.loader.poll() {
            match event {
                LoadEvent::Loaded { id, image } if id == self.latest_request => {
                    self.install_image(context, image);
                    self.loading = false;
                    self.error = None;
                }
                LoadEvent::Analysed {
                    id,
                    analysis,
                    kind,
                    space,
                } => {
                    let pending = match kind {
                        AnalysisKind::Hover
                            if self.hover_request.is_some_and(|request| {
                                request.id == id && request.space == space
                            }) =>
                        {
                            self.hover_request.take()
                        }
                        AnalysisKind::Rectangle
                            if self.rectangle_request.is_some_and(|request| {
                                request.id == id && request.space == space
                            }) =>
                        {
                            self.rectangle_request.take()
                        }
                        _ => None,
                    };
                    if let Some(pending) = pending {
                        let analyses = self.analyses_mut(pending.space);
                        match kind {
                            AnalysisKind::Hover => analyses.hover = Some(analysis),
                            AnalysisKind::Rectangle => analyses.rectangle = Some(analysis),
                        }
                        self.refresh_scope_textures(context, pending.space);
                    }
                }
                LoadEvent::Failed { id, message } if id == self.latest_request => {
                    self.loading = false;
                    self.error = Some(message);
                }
                LoadEvent::Highlighted {
                    id,
                    rgba,
                    width,
                    height,
                } if self.reverse_request == Some(id) => {
                    self.reverse_request = None;
                    self.reverse_texture = Some(context.load_texture(
                        "scope-reverse-highlight",
                        egui::ColorImage::from_rgba_unmultiplied(
                            [width as usize, height as usize],
                            &rgba,
                        ),
                        egui::TextureOptions::LINEAR,
                    ));
                }
                _ => {}
            }
        }
    }

    fn install_image(&mut self, context: &egui::Context, image: LoadedImage) {
        let image = Arc::new(image);
        let colour_image = egui::ColorImage::from_rgba_unmultiplied(
            [image.width as usize, image.height as usize],
            &image.rgba,
        );
        self.image_texture =
            Some(context.load_texture("loaded-image", colour_image, egui::TextureOptions::LINEAR));
        self.image_path = Some(image.path.clone());
        self.dimensions = Some([image.width, image.height]);
        self.sampled_pixels = image.scope.sampled_pixels;
        self.ryb_analyses = ScopeAnalyses {
            full: Some(image.scope.clone()),
            ..ScopeAnalyses::default()
        };
        self.cie_analyses = ScopeAnalyses {
            full: Some(image.cie_scope.clone()),
            ..ScopeAnalyses::default()
        };
        self.ryb_textures = ScopeTextures::default();
        self.cie_textures = ScopeTextures::default();
        self.selection = None;
        self.rect_interaction = None;
        self.hover_position = None;
        self.last_hover_region = None;
        self.hover_request = None;
        self.rectangle_request = None;
        self.reverse_texture = None;
        self.reverse_request = None;
        self.reverse_query = None;
        self.loaded = Some(image);
        self.refresh_scope_textures(context, ScopeSpace::Ryb);
        self.refresh_scope_textures(context, ScopeSpace::Cie1931);
    }

    fn analyses(&self, space: ScopeSpace) -> &ScopeAnalyses {
        match space {
            ScopeSpace::Ryb => &self.ryb_analyses,
            ScopeSpace::Cie1931 => &self.cie_analyses,
        }
    }

    fn analyses_mut(&mut self, space: ScopeSpace) -> &mut ScopeAnalyses {
        match space {
            ScopeSpace::Ryb => &mut self.ryb_analyses,
            ScopeSpace::Cie1931 => &mut self.cie_analyses,
        }
    }

    fn textures_mut(&mut self, space: ScopeSpace) -> &mut ScopeTextures {
        match space {
            ScopeSpace::Ryb => &mut self.ryb_textures,
            ScopeSpace::Cie1931 => &mut self.cie_textures,
        }
    }

    fn scope_texture_name(space: ScopeSpace, layer: &str) -> String {
        let prefix = match space {
            ScopeSpace::Ryb => "ryb",
            ScopeSpace::Cie1931 => "cie1931",
        };
        format!("{prefix}-scope-{layer}")
    }

    fn refresh_scope_textures(&mut self, context: &egui::Context, space: ScopeSpace) {
        let settings = (
            self.trace_intensity,
            self.dot_sharpness,
            self.scale_for_space(space),
        );
        let analyses = self.analyses(space);
        let full = analyses.full.clone();
        let rectangle = analyses.rectangle.clone();
        let hover = analyses.hover.clone();
        let render = |analysis: &VectorscopeAnalysis, inverse| {
            render_trace(analysis, settings.0, settings.1, settings.2, inverse)
        };
        let full_texture = full.as_ref().map(|analysis| {
            context.load_texture(
                Self::scope_texture_name(space, "full"),
                render(analysis, false),
                egui::TextureOptions::LINEAR,
            )
        });
        let rectangle_texture = rectangle.as_ref().map(|analysis| {
            context.load_texture(
                Self::scope_texture_name(space, "rectangle"),
                render(analysis, false),
                egui::TextureOptions::LINEAR,
            )
        });
        let hover_texture = hover.as_ref().map(|analysis| {
            context.load_texture(
                Self::scope_texture_name(space, "hover"),
                render(analysis, true),
                egui::TextureOptions::LINEAR,
            )
        });
        let textures = self.textures_mut(space);
        textures.full = full_texture;
        textures.rectangle = rectangle_texture;
        textures.hover = hover_texture;
    }

    fn scale_for_space(&self, space: ScopeSpace) -> DensityScale {
        match space {
            ScopeSpace::Ryb => self.density_scale,
            ScopeSpace::Cie1931 => DensityScale::Linear,
        }
    }

    fn request_analysis(
        &mut self,
        region: AnalysisRegion,
        kind: AnalysisKind,
        space: ScopeSpace,
    ) -> u64 {
        let Some(image) = self.loaded.clone() else {
            return 0;
        };
        self.loader.analyse(image, region, kind, space)
    }

    fn request_hover_analysis(&mut self, position: [f32; 2]) {
        let region = AnalysisRegion::Circle {
            centre: position,
            radius: self.hover_radius,
        };
        let space = self.active_scope.space();
        if self.last_hover_region == Some((space, region)) {
            return;
        }
        self.last_hover_region = Some((space, region));
        self.hover_request = Some(PendingAnalysis {
            id: self.request_analysis(region, AnalysisKind::Hover, space),
            space,
        });
    }

    fn request_rectangle_analysis(&mut self) {
        let Some(rectangle) = self.selection else {
            self.analyses_mut(self.active_scope.space()).rectangle = None;
            return;
        };
        let space = self.active_scope.space();
        self.rectangle_request = Some(PendingAnalysis {
            id: self.request_analysis(
                AnalysisRegion::Rectangle {
                    min: rectangle.min,
                    max: rectangle.max,
                },
                AnalysisKind::Rectangle,
                space,
            ),
            space,
        });
    }

    fn request_reverse_highlight(&mut self, centre: [f32; 2], radius: f32) {
        let Some(image) = self.loaded.clone() else {
            return;
        };
        let space = self.active_scope.space();
        let radius = radius.max(0.000_1);
        let query = (space, centre, radius);
        if self.reverse_query == Some(query) {
            return;
        }
        self.reverse_query = Some(query);
        self.reverse_request =
            Some(
                self.loader
                    .highlight(image, centre, radius, space, self.scale_for_space(space)),
            );
    }

    fn show_toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            if ui.button("Open image…").clicked() {
                self.open_image();
            }
            ui.separator();
            let selection_label = if self.selection_tool {
                "Rectangle tool on"
            } else {
                "Draw rectangle"
            };
            if ui
                .selectable_label(self.selection_tool, selection_label)
                .on_hover_text("Drag to draw a rectangle. Drag inside it to move it, or drag a corner to resize it.")
                .clicked()
            {
                self.selection_tool = !self.selection_tool;
            }
            if self.selection.is_some() && ui.button("Clear rectangle").clicked() {
                self.selection = None;
                self.rectangle_request = None;
                self.ryb_analyses.rectangle = None;
                self.cie_analyses.rectangle = None;
                self.ryb_textures.rectangle = None;
                self.cie_textures.rectangle = None;
            }
            if self.loading {
                ui.spinner();
                ui.label("Loading and analysing…");
            }
            if let Some(path) = &self.image_path {
                ui.separator();
                ui.label(
                    path.file_name()
                        .map_or_else(|| path.display().to_string(), |name| name.to_string_lossy().into_owned()),
                );
            }
        });
        if let Some(message) = &self.error {
            ui.colored_label(Color32::from_rgb(235, 120, 110), message);
        }
    }

    fn select_scope(&mut self, tab: ScopeTab) {
        if self.active_scope == tab {
            return;
        }
        self.active_scope = tab;
        self.last_hover_region = None;
        self.clear_reverse_highlight();
        if self.selection.is_some() {
            self.analyses_mut(tab.space()).rectangle = None;
            self.textures_mut(tab.space()).rectangle = None;
            self.request_rectangle_analysis();
        }
        if let Some(position) = self.hover_position {
            self.request_hover_analysis(position);
        }
    }

    fn show_scope_panel(&mut self, ui: &mut egui::Ui) {
        let rect = ui.available_rect_before_wrap();
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, CornerRadius::ZERO, BACKGROUND);

        ui.vertical(|ui| {
            ui.horizontal_wrapped(|ui| {
                for tab in [ScopeTab::Cie1931, ScopeTab::Ryb] {
                    if ui
                        .selectable_label(self.active_scope == tab, tab.label())
                        .clicked()
                    {
                        self.select_scope(tab);
                        self.refresh_scope_textures(ui.ctx(), tab.space());
                    }
                }
                ui.separator();
                if self.active_scope == ScopeTab::Ryb {
                    let mut logarithmic = self.density_scale == DensityScale::Logarithmic;
                    if ui
                        .checkbox(&mut logarithmic, "Logarithmic")
                        .on_hover_text(
                            "Logarithmic radius expands low-chroma colours near the neutral centre, as in darktable.",
                        )
                        .changed()
                    {
                        self.density_scale = if logarithmic {
                            DensityScale::Logarithmic
                        } else {
                            DensityScale::Linear
                        };
                        self.refresh_scope_textures(ui.ctx(), ScopeSpace::Ryb);
                        self.reverse_query = None;
                    }
                } else {
                    ui.weak("linear chromaticity");
                }
                let intensity = ui.add(
                    egui::Slider::new(&mut self.trace_intensity, 0.25..=3.0)
                        .text("Intensity")
                        .logarithmic(true),
                );
                let sharpness = ui
                    .add(
                        egui::Slider::new(&mut self.dot_sharpness, 0.2..=2.5)
                            .text("Dot sharpness"),
                    )
                    .on_hover_text(
                        "Higher values make colour dots crisper; lower values make their edges softer.",
                    );
                if intensity.changed() || sharpness.changed() {
                    let space = self.active_scope.space();
                    self.refresh_scope_textures(ui.ctx(), space);
                }
            });
            ui.separator();
            self.show_scope_plot(ui);
        });
    }

    fn show_scope_plot(&mut self, ui: &mut egui::Ui) {
        match self.active_scope {
            ScopeTab::Cie1931 => self.show_cie1931_scope(ui),
            ScopeTab::Ryb => self.show_ryb_scope(ui),
        }
    }

    fn active_textures(&self) -> &ScopeTextures {
        match self.active_scope.space() {
            ScopeSpace::Ryb => &self.ryb_textures,
            ScopeSpace::Cie1931 => &self.cie_textures,
        }
    }

    fn show_ryb_scope(&mut self, ui: &mut egui::Ui) {
        let plot_rect = ui.available_rect_before_wrap();
        let available = plot_rect.size();
        let side = available.x.min(available.y).max(1.0);
        let rect = Rect::from_center_size(plot_rect.center(), Vec2::splat(side));
        let response = ui.allocate_rect(plot_rect, Sense::hover());
        let painter = ui.painter_at(response.rect);
        painter.rect_filled(response.rect, CornerRadius::ZERO, BACKGROUND);
        painter.circle_filled(rect.center(), side * 0.495, SCOPE_BACKGROUND);

        let ring_radius = side * 0.465;
        for index in 0..3 {
            let radius = display_radius(
                side * 0.5 * (index + 1) as f32 / 3.0,
                side * 0.5,
                self.density_scale,
            );
            painter.circle_stroke(rect.center(), radius, Stroke::new(1.0, GRID));
        }
        draw_hue_ring(&painter, rect.center(), ring_radius, self.density_scale);

        let textures = self.active_textures();
        let base_texture = if self.selection.is_some() {
            textures.rectangle.as_ref()
        } else {
            textures.full.as_ref()
        };
        if let Some(texture) = base_texture {
            let trace_rect = Rect::from_center_size(rect.center(), Vec2::splat(ring_radius * 2.0));
            painter.image(
                texture.id(),
                trace_rect,
                Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                Color32::WHITE,
            );
        } else {
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                if self.selection.is_some() {
                    "Analysing rectangle…"
                } else {
                    "Open a PNG or JPEG"
                },
                egui::FontId::proportional(18.0),
                Color32::from_gray(145),
            );
        }
        if self.selection.is_none()
            && let Some(texture) = &textures.hover
        {
            let trace_rect = Rect::from_center_size(rect.center(), Vec2::splat(ring_radius * 2.0));
            painter.image(
                texture.id(),
                trace_rect,
                Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                Color32::WHITE,
            );
        }
        if let Some((ScopeSpace::Ryb, [x, y], reverse_radius)) = self.reverse_query {
            let position = Pos2::new(
                rect.center().x - ring_radius + x * ring_radius * 2.0,
                rect.center().y - ring_radius + y * ring_radius * 2.0,
            );
            painter.circle_stroke(
                position,
                (reverse_radius * ring_radius * 2.0).max(2.0),
                Stroke::new(1.0, HOVER.linear_multiply(0.8)),
            );
        }
        painter.circle_filled(rect.center(), 2.5, Color32::from_gray(130));
        self.handle_scope_interaction(
            ui,
            &response,
            ScopeHitArea::Ryb {
                centre: rect.center(),
                radius: ring_radius,
            },
        );
    }

    fn show_cie1931_scope(&mut self, ui: &mut egui::Ui) {
        let plot_rect = ui.available_rect_before_wrap();
        let available = plot_rect.size() - Vec2::splat(16.0);
        let chart_size = fit_size(Vec2::new(CIE_X_MAX, CIE_Y_MAX), available).max(Vec2::splat(1.0));
        let chart = Rect::from_center_size(plot_rect.center(), chart_size);
        let response = ui.allocate_rect(plot_rect, Sense::hover());
        let painter = ui.painter_at(response.rect);
        painter.rect_filled(response.rect, CornerRadius::ZERO, BACKGROUND);
        draw_cie1931_background(&painter, chart, DensityScale::Linear);

        let textures = self.active_textures();
        let base_texture = if self.selection.is_some() {
            textures.rectangle.as_ref()
        } else {
            textures.full.as_ref()
        };
        if let Some(texture) = base_texture {
            painter.image(
                texture.id(),
                chart,
                Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                Color32::WHITE,
            );
        } else {
            painter.text(
                chart.center(),
                egui::Align2::CENTER_CENTER,
                if self.selection.is_some() {
                    "Analysing rectangle…"
                } else {
                    "Open a PNG or JPEG"
                },
                egui::FontId::proportional(18.0),
                Color32::from_gray(145),
            );
        }
        if self.selection.is_none()
            && let Some(texture) = &textures.hover
        {
            painter.image(
                texture.id(),
                chart,
                Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                Color32::WHITE,
            );
        }
        if let Some((ScopeSpace::Cie1931, [x, y], reverse_radius)) = self.reverse_query {
            let position = Pos2::new(
                chart.left() + x * chart.width(),
                chart.top() + y * chart.height(),
            );
            painter.circle_stroke(
                position,
                (reverse_radius * chart.width().min(chart.height())).max(2.0),
                Stroke::new(1.0, HOVER.linear_multiply(0.8)),
            );
        }
        let white = cie_point_to_screen(chart, [0.312_7, 0.329_0]);
        painter.circle_filled(white, 2.5, Color32::from_gray(175));
        self.handle_scope_interaction(ui, &response, ScopeHitArea::Cie { chart });
    }

    fn clear_reverse_highlight(&mut self) {
        self.reverse_texture = None;
        self.reverse_request = None;
        self.reverse_query = None;
    }

    fn handle_scope_interaction(
        &mut self,
        ui: &mut egui::Ui,
        response: &egui::Response,
        area: ScopeHitArea,
    ) {
        let Some(pointer) = response.hover_pos() else {
            self.clear_reverse_highlight();
            return;
        };
        let Some(centre) = area.coordinate(pointer) else {
            self.clear_reverse_highlight();
            return;
        };
        let scroll = ui.input(|input| input.smooth_scroll_delta.y);
        if scroll.abs() > f32::EPSILON {
            self.reverse_radius = (self.reverse_radius * (scroll / 120.0).exp()).clamp(0.002, 0.25);
            self.reverse_query = None;
        }
        self.request_reverse_highlight(centre, self.reverse_radius);
        if self.reverse_request.is_some() {
            ui.ctx().request_repaint();
        }
    }

    fn show_image(&mut self, ui: &mut egui::Ui) {
        let rect = ui.available_rect_before_wrap();
        let panel_response = ui.allocate_rect(rect, Sense::hover());
        let painter = ui.painter_at(panel_response.rect);
        painter.rect_filled(
            panel_response.rect,
            CornerRadius::ZERO,
            Color32::from_rgb(13, 14, 16),
        );

        if let (Some(texture), Some([width, height])) = (&self.image_texture, self.dimensions) {
            let fitted = fit_size(
                Vec2::new(width as f32, height as f32),
                panel_response.rect.size() - Vec2::splat(24.0),
            );
            let image_rect = Rect::from_center_size(panel_response.rect.center(), fitted);
            let image_response = ui.interact(
                image_rect,
                ui.id().with("image-interaction"),
                Sense::click_and_drag(),
            );
            painter.image(
                texture.id(),
                image_rect,
                Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                Color32::WHITE,
            );
            if let Some(reverse_texture) = &self.reverse_texture {
                painter.image(
                    reverse_texture.id(),
                    image_rect,
                    Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                    Color32::WHITE,
                );
            }
            painter.rect_stroke(
                image_rect,
                CornerRadius::ZERO,
                Stroke::new(1.0, Color32::from_gray(42)),
                StrokeKind::Outside,
            );
            self.handle_image_interaction(ui, &image_response, image_rect, width, height);
            self.draw_image_overlays(ui, image_rect, width, height);
        } else {
            painter.text(
                panel_response.rect.center(),
                egui::Align2::CENTER_CENTER,
                "Image preview",
                egui::FontId::proportional(18.0),
                Color32::from_gray(105),
            );
        }
    }

    fn handle_image_interaction(
        &mut self,
        ui: &mut egui::Ui,
        response: &egui::Response,
        image_rect: Rect,
        width: u32,
        height: u32,
    ) {
        let to_image = |position: Pos2| {
            [
                ((position.x - image_rect.left()) / image_rect.width() * width as f32)
                    .clamp(0.0, width.saturating_sub(1) as f32),
                ((position.y - image_rect.top()) / image_rect.height() * height as f32)
                    .clamp(0.0, height.saturating_sub(1) as f32),
            ]
        };
        let Some(pointer) = response.hover_pos() else {
            self.hover_position = None;
            let space = self.active_scope.space();
            self.analyses_mut(space).hover = None;
            self.textures_mut(space).hover = None;
            self.last_hover_region = None;
            self.hover_request = None;
            return;
        };
        let image_position = to_image(pointer);
        let hover_pixel = [
            image_position[0].floor() + 0.5,
            image_position[1].floor() + 0.5,
        ];
        if self.hover_position != Some(hover_pixel) {
            self.hover_position = Some(hover_pixel);
            self.request_hover_analysis(hover_pixel);
        }
        let scroll = ui.input(|input| input.smooth_scroll_delta.y);
        if scroll.abs() > f32::EPSILON {
            let factor = (scroll / 120.0).exp();
            let max_radius = (width.min(height) as f32 / 2.0).max(0.5);
            self.hover_radius = (self.hover_radius * factor).clamp(0.5, max_radius);
            self.last_hover_region = None;
            self.request_hover_analysis(hover_pixel);
        }

        if self.selection_tool {
            if response.drag_started()
                && let Some(position) = response.interact_pointer_pos()
            {
                let space = self.active_scope.space();
                self.analyses_mut(space).rectangle = None;
                self.textures_mut(space).rectangle = None;
                let image_position = to_image(position);
                self.rect_interaction = Some(match self.selection {
                    Some(selection) if selection.contains(image_position) => {
                        if let Some(handle) =
                            Self::near_handle(selection, image_position, image_rect, width, height)
                        {
                            RectInteraction::Resize {
                                handle,
                                original: selection,
                            }
                        } else {
                            RectInteraction::Move {
                                start: image_position,
                                original: selection,
                            }
                        }
                    }
                    _ => RectInteraction::Draw {
                        start: image_position,
                    },
                });
            }
            if let (Some(interaction), Some(position)) =
                (self.rect_interaction, response.interact_pointer_pos())
            {
                let current = to_image(position);
                let next = match interaction {
                    RectInteraction::Draw { start } => ImageRect::new(start, current),
                    RectInteraction::Move { start, original } => {
                        let dx = current[0] - start[0];
                        let dy = current[1] - start[1];
                        let rectangle_width = original.max[0] - original.min[0];
                        let rectangle_height = original.max[1] - original.min[1];
                        let next_x =
                            (original.min[0] + dx).clamp(0.0, width as f32 - rectangle_width);
                        let next_y =
                            (original.min[1] + dy).clamp(0.0, height as f32 - rectangle_height);
                        ImageRect {
                            min: [next_x, next_y],
                            max: [next_x + rectangle_width, next_y + rectangle_height],
                        }
                    }
                    RectInteraction::Resize { handle, original } => {
                        resize_rect(original, handle, current)
                    }
                };
                self.selection = Some(next.clamp(width as f32, height as f32));
            }
            if response.drag_stopped() && self.rect_interaction.take().is_some() {
                self.request_rectangle_analysis();
            }
        }
    }

    fn near_handle(
        selection: ImageRect,
        point: [f32; 2],
        image_rect: Rect,
        width: u32,
        height: u32,
    ) -> Option<RectHandle> {
        let threshold_x = width as f32 * 12.0 / image_rect.width();
        let threshold_y = height as f32 * 12.0 / image_rect.height();
        let corners = [
            (RectHandle::TopLeft, selection.min),
            (RectHandle::TopRight, [selection.max[0], selection.min[1]]),
            (RectHandle::BottomLeft, [selection.min[0], selection.max[1]]),
            (RectHandle::BottomRight, selection.max),
        ];
        corners.into_iter().find_map(|(handle, corner)| {
            if (point[0] - corner[0]).abs() <= threshold_x
                && (point[1] - corner[1]).abs() <= threshold_y
            {
                Some(handle)
            } else {
                None
            }
        })
    }

    fn draw_image_overlays(&self, ui: &mut egui::Ui, image_rect: Rect, width: u32, height: u32) {
        let painter = ui.painter_at(image_rect);
        let from_image = |point: [f32; 2]| {
            Pos2::new(
                image_rect.left() + point[0] / width as f32 * image_rect.width(),
                image_rect.top() + point[1] / height as f32 * image_rect.height(),
            )
        };
        if let Some(selection) = self.selection {
            let selection_rect =
                Rect::from_two_pos(from_image(selection.min), from_image(selection.max));
            painter.rect_filled(
                selection_rect,
                CornerRadius::ZERO,
                SELECTION.linear_multiply(0.08),
            );
            painter.rect_stroke(
                selection_rect,
                CornerRadius::ZERO,
                Stroke::new(1.5, SELECTION),
                StrokeKind::Inside,
            );
            for corner in [
                selection.min,
                [selection.max[0], selection.min[1]],
                [selection.min[0], selection.max[1]],
                selection.max,
            ] {
                painter.circle_filled(from_image(corner), 5.0, SELECTION);
                painter.circle_stroke(from_image(corner), 7.0, Stroke::new(1.0, BACKGROUND));
            }
        }
        if let Some(position) = self.hover_position {
            let highlight = self.inverse_colour_at(position);
            let screen_position = from_image(position);
            let source_per_screen =
                (width as f32 / image_rect.width()).max(height as f32 / image_rect.height());
            let screen_radius = self.hover_radius / source_per_screen;
            if (self.hover_radius - 0.5).abs() <= f32::EPSILON {
                painter.rect_stroke(
                    Rect::from_center_size(screen_position, Vec2::splat(4.0)),
                    CornerRadius::ZERO,
                    Stroke::new(1.0, highlight),
                    StrokeKind::Inside,
                );
            } else {
                painter.circle_stroke(
                    screen_position,
                    screen_radius.max(2.0),
                    Stroke::new(1.0, highlight),
                );
            }
        }
    }

    fn inverse_colour_at(&self, position: [f32; 2]) -> Color32 {
        let Some(image) = &self.loaded else {
            return HOVER;
        };
        let x = position[0]
            .floor()
            .clamp(0.0, image.width.saturating_sub(1) as f32) as usize;
        let y = position[1]
            .floor()
            .clamp(0.0, image.height.saturating_sub(1) as f32) as usize;
        let index = (y * image.width as usize + x) * 4;
        Color32::from_rgb(
            255 - image.rgba[index],
            255 - image.rgba[index + 1],
            255 - image.rgba[index + 2],
        )
    }
}

impl eframe::App for BetterPlotsApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let context = ui.ctx().clone();
        self.poll_loader(&context);
        if self.loading {
            context.request_repaint();
        }

        egui::Panel::top("toolbar").show(ui, |ui| self.show_toolbar(ui));
        egui::Panel::bottom("status").show(ui, |ui| {
            ui.horizontal(|ui| {
                if let Some([width, height]) = self.dimensions {
                    ui.weak(format!("{width} × {height}"));
                    ui.separator();
                    ui.weak(format!("{} scope samples", self.sampled_pixels));
                } else {
                    ui.weak("Scopes analyse decoded pixels currently assumed to be sRGB.");
                }
            });
        });
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(BACKGROUND))
            .show(ui, |ui| {
                ui.columns(2, |columns| {
                    self.show_scope_panel(&mut columns[0]);
                    self.show_image(&mut columns[1]);
                });
            });
    }
}

// CIE 1931 2° spectral locus, sampled every 5 nm from 380–780 nm. The
// higher sampling density keeps the horseshoe smooth at normal window sizes;
// the final purple edge is closed by joining the two ends of the spectrum.
const CIE1931_LOCUS: [[f32; 2]; 81] = [
    [0.1741123, 0.0049637],
    [0.1740078, 0.0049805],
    [0.1738008, 0.0049154],
    [0.1735599, 0.0049232],
    [0.1733369, 0.0047967],
    [0.1730210, 0.0047751],
    [0.1725766, 0.0047993],
    [0.1720866, 0.0048325],
    [0.1714074, 0.0051022],
    [0.1703010, 0.0057885],
    [0.1688775, 0.0069002],
    [0.1668953, 0.0085556],
    [0.1644118, 0.0108576],
    [0.1611046, 0.0137934],
    [0.1566409, 0.0177048],
    [0.1509854, 0.0227402],
    [0.1439604, 0.0297030],
    [0.1355027, 0.0398791],
    [0.1241185, 0.0578025],
    [0.1095943, 0.0868425],
    [0.0912935, 0.1327021],
    [0.0687059, 0.2007232],
    [0.0453907, 0.2949760],
    [0.0234599, 0.4127035],
    [0.0081680, 0.5384231],
    [0.0038585, 0.6548232],
    [0.0138702, 0.7501864],
    [0.0388518, 0.8120160],
    [0.0743024, 0.8338031],
    [0.1141607, 0.8262070],
    [0.1547221, 0.8058635],
    [0.1928762, 0.7816291],
    [0.2296197, 0.7543291],
    [0.2657751, 0.7243239],
    [0.3016039, 0.6923077],
    [0.3373633, 0.6588483],
    [0.3731015, 0.6244509],
    [0.4087363, 0.5896069],
    [0.4440625, 0.5547139],
    [0.4787748, 0.5202023],
    [0.5124864, 0.4865908],
    [0.5447865, 0.4544341],
    [0.5751513, 0.4242322],
    [0.6029328, 0.3964966],
    [0.6270366, 0.3724911],
    [0.6482331, 0.3513949],
    [0.6657636, 0.3340107],
    [0.6800788, 0.3197472],
    [0.6915040, 0.3083422],
    [0.7006061, 0.2993007],
    [0.7079178, 0.2920271],
    [0.7140316, 0.2859289],
    [0.7190329, 0.2809350],
    [0.7230316, 0.2769484],
    [0.7259923, 0.2740077],
    [0.7282717, 0.2717283],
    [0.7299690, 0.2700310],
    [0.7310894, 0.2689106],
    [0.7319933, 0.2680067],
    [0.7327189, 0.2672811],
    [0.7334170, 0.2665830],
    [0.7340473, 0.2659527],
    [0.7343902, 0.2656098],
    [0.7345917, 0.2654083],
    [0.7346873, 0.2653127],
    [0.7346920, 0.2653080],
    [0.7346783, 0.2653217],
    [0.7346683, 0.2653317],
    [0.7346680, 0.2653320],
    [0.7346719, 0.2653281],
    [0.7346939, 0.2653061],
    [0.7347539, 0.2652461],
    [0.7348243, 0.2651757],
    [0.7345679, 0.2654321],
    [0.7345133, 0.2654867],
    [0.7343750, 0.2656250],
    [0.7345133, 0.2654867],
    [0.7358491, 0.2641509],
    [0.7345133, 0.2654867],
    [0.7375000, 0.2625000],
    [0.7368421, 0.2631579],
];

fn cie_point_to_screen(chart: Rect, point: [f32; 2]) -> Pos2 {
    Pos2::new(
        chart.left() + point[0] / CIE_X_MAX * chart.width(),
        chart.bottom() - point[1] / CIE_Y_MAX * chart.height(),
    )
}

fn cie_wavelength_colour(wavelength: f32) -> Color32 {
    let (red, green, blue) = match wavelength {
        380.0..420.0 => ((-(wavelength - 420.0) / 40.0), 0.0, 1.0),
        420.0..490.0 => (0.0, (wavelength - 420.0) / 70.0, 1.0),
        490.0..510.0 => (0.0, 1.0, -(wavelength - 510.0) / 20.0),
        510.0..580.0 => ((wavelength - 510.0) / 70.0, 1.0, 0.0),
        580.0..645.0 => (1.0, -(wavelength - 645.0) / 65.0, 0.0),
        _ => (1.0, 0.0, 0.0),
    };
    let pastel = |channel: f32| (channel * 0.72 + 0.28) * 255.0;
    Color32::from_rgb(
        pastel(red.clamp(0.0, 1.0)) as u8,
        pastel(green.clamp(0.0, 1.0)) as u8,
        pastel(blue.clamp(0.0, 1.0)) as u8,
    )
}

fn display_radius(linear_radius: f32, maximum: f32, scale: DensityScale) -> f32 {
    match scale {
        DensityScale::Linear => linear_radius,
        DensityScale::Logarithmic => {
            ((linear_radius / maximum * 30.0_f32.ln()).exp() - 1.0) / 29.0 * maximum
        }
    }
}

fn cie_display_point(chart: Rect, point: [f32; 2], scale: DensityScale) -> Pos2 {
    if scale == DensityScale::Linear {
        return cie_point_to_screen(chart, point);
    }
    let centre = [0.312_7 / CIE_X_MAX, 1.0 - 0.329_0 / CIE_Y_MAX];
    let output = [point[0] / CIE_X_MAX, 1.0 - point[1] / CIE_Y_MAX];
    let delta = [output[0] - centre[0], output[1] - centre[1]];
    let radius = delta[0].hypot(delta[1]);
    if radius <= f32::EPSILON {
        return cie_point_to_screen(chart, point);
    }
    let maximum = 0.65;
    let scaled = display_radius(radius, maximum, scale);
    let mapped = [
        centre[0] + delta[0] / radius * scaled,
        centre[1] + delta[1] / radius * scaled,
    ];
    Pos2::new(
        chart.left() + mapped[0] * chart.width(),
        chart.top() + mapped[1] * chart.height(),
    )
}

fn draw_cie1931_background(painter: &egui::Painter, chart: Rect, scale: DensityScale) {
    painter.rect_filled(chart, CornerRadius::ZERO, SCOPE_BACKGROUND);
    for step in 1..8 {
        let x = step as f32 / 10.0;
        painter.line_segment(
            [
                cie_point_to_screen(chart, [x, 0.0]),
                cie_point_to_screen(chart, [x, CIE_Y_MAX]),
            ],
            Stroke::new(1.0, GRID),
        );
    }
    for step in 1..9 {
        let y = step as f32 / 10.0;
        painter.line_segment(
            [
                cie_point_to_screen(chart, [0.0, y]),
                cie_point_to_screen(chart, [CIE_X_MAX, y]),
            ],
            Stroke::new(1.0, GRID),
        );
    }

    for (index, segment) in CIE1931_LOCUS.windows(2).enumerate() {
        let a = cie_display_point(chart, segment[0], scale);
        let b = cie_display_point(chart, segment[1], scale);
        let colour = cie_wavelength_colour(380.0 + index as f32 * 5.0);
        painter.line_segment([a, b], Stroke::new(1.2, colour.linear_multiply(0.78)));
    }
    let first = cie_display_point(chart, CIE1931_LOCUS[0], scale);
    let last = cie_display_point(
        chart,
        *CIE1931_LOCUS.last().unwrap_or(&CIE1931_LOCUS[0]),
        scale,
    );
    painter.line_segment(
        [last, first],
        Stroke::new(1.2, Color32::from_rgb(220, 145, 215).linear_multiply(0.78)),
    );
}

fn draw_hue_ring(painter: &egui::Painter, centre: Pos2, radius: f32, scale: DensityScale) {
    const SEGMENTS: usize = 288;
    for index in 0..SEGMENTS {
        let turn_a = index as f32 / SEGMENTS as f32;
        let turn_b = (index + 1) as f32 / SEGMENTS as f32;
        let angle_a = -std::f32::consts::FRAC_PI_2 - std::f32::consts::TAU * turn_a;
        let angle_b = -std::f32::consts::FRAC_PI_2 - std::f32::consts::TAU * turn_b;
        let mapped_radius = display_radius(radius, radius / 0.465 * 0.5, scale);
        let point_a = centre + Vec2::angled(angle_a) * mapped_radius;
        let point_b = centre + Vec2::angled(angle_b) * mapped_radius;
        painter.line_segment(
            [point_a, point_b],
            Stroke::new(
                1.4,
                ring_colour((turn_a + turn_b) * 0.5).gamma_multiply(0.72),
            ),
        );
    }

    for index in 0..6 {
        let turn = index as f32 / 6.0;
        let angle = -std::f32::consts::FRAC_PI_2 - std::f32::consts::TAU * turn;
        let mapped_radius = display_radius(radius, radius / 0.465 * 0.5, scale);
        let point = centre + Vec2::angled(angle) * mapped_radius;
        painter.circle_filled(point, 2.5, ring_colour(turn));
        painter.circle_stroke(point, 3.2, Stroke::new(1.0, Color32::from_gray(90)));
    }
}

fn fit_size(source: Vec2, available: Vec2) -> Vec2 {
    if source.x <= 0.0 || source.y <= 0.0 || available.x <= 0.0 || available.y <= 0.0 {
        return Vec2::ZERO;
    }
    let scale = (available.x / source.x).min(available.y / source.y);
    source * scale
}

fn resize_rect(original: ImageRect, handle: RectHandle, point: [f32; 2]) -> ImageRect {
    match handle {
        RectHandle::TopLeft => ImageRect::new(point, original.max),
        RectHandle::TopRight => {
            ImageRect::new([original.min[0], point[1]], [point[0], original.max[1]])
        }
        RectHandle::BottomLeft => {
            ImageRect::new([point[0], original.min[1]], [original.max[0], point[1]])
        }
        RectHandle::BottomRight => ImageRect::new(original.min, point),
    }
}

fn configure_visuals(context: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = BACKGROUND;
    visuals.window_fill = Color32::from_rgb(14, 16, 18);
    visuals.extreme_bg_color = SCOPE_BACKGROUND;
    visuals.faint_bg_color = Color32::from_rgb(18, 20, 22);
    context.set_visuals(visuals);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_point_eq(actual: [f32; 2], expected: [f32; 2]) {
        for (actual, expected) in actual.into_iter().zip(expected) {
            assert!((actual - expected).abs() <= f32::EPSILON);
        }
    }

    #[test]
    fn fitting_preserves_aspect_ratio() {
        let fitted = fit_size(Vec2::new(400.0, 200.0), Vec2::new(300.0, 300.0));
        assert_eq!(fitted, Vec2::new(300.0, 150.0));
    }

    #[test]
    fn rectangle_clamp_keeps_edges_inside_image() {
        let rectangle = ImageRect::new([-20.0, 10.0], [120.0, 140.0]).clamp(100.0, 100.0);
        assert_point_eq(rectangle.min, [0.0, 10.0]);
        assert_point_eq(rectangle.max, [100.0, 100.0]);
    }

    #[test]
    fn resizing_rectangle_updates_only_the_selected_corner() {
        let original = ImageRect::new([20.0, 20.0], [60.0, 70.0]);
        let resized = resize_rect(original, RectHandle::BottomRight, [90.0, 95.0]);
        assert_point_eq(resized.min, original.min);
        assert_point_eq(resized.max, [90.0, 95.0]);
    }
}
