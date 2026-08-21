#![allow(clippy::cast_precision_loss)]

use std::{path::PathBuf, sync::Arc, time::Duration};

use eframe::egui::{
    self, Color32, CornerRadius, PointerButton, Pos2, Rect, Sense, Stroke, StrokeKind,
    TextureHandle, Ui, Vec2,
};

use crate::{
    curve::{
        BezierHandleKind, CURVE_DOMAIN_LABEL, Curve, CurveChannel, CurveInterpolation, CurveMode,
        CurveSet, DerivativeCurve, LuminanceDefinition,
    },
    loader::{ImageEvent, ImageLoader},
    pipeline::{
        Histogram, HistogramCalculation, InputColourSpace, PipelineSnapshot, PreparedImage,
        RenderedPreview, SourceImage, write_srgb_png,
    },
    preview::{PreviewWorker, RenderEvent},
};

const PANEL_BACKGROUND: Color32 = Color32::from_rgb(25, 29, 34);
const GRAPH_BACKGROUND: Color32 = Color32::from_rgb(20, 23, 27);
const GRID: Color32 = Color32::from_rgb(57, 63, 70);
const CURVE_WHITE: Color32 = Color32::from_rgb(230, 236, 240);
const ACCENT: Color32 = Color32::from_rgb(113, 184, 196);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GraphMode {
    ToneCurve,
    Derivative,
}

impl GraphMode {
    const fn label(self) -> &'static str {
        match self {
            Self::ToneCurve => "Tone curve",
            Self::Derivative => "Derivative",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DragTarget {
    Point,
    Handle(BezierHandleKind),
}

#[derive(Clone, Debug)]
struct CurveDrag {
    index: usize,
    target: DragTarget,
    mode: CurveMode,
    channel: CurveChannel,
    initial: Curve,
    initial_derivative: Option<DerivativeCurve>,
    tension_delta: f32,
}

#[derive(Clone, Copy, Debug)]
struct SyncedView {
    zoom: f32,
    pan: Vec2,
}

impl Default for SyncedView {
    fn default() -> Self {
        Self {
            zoom: 1.0,
            pan: Vec2::ZERO,
        }
    }
}

#[allow(clippy::struct_excessive_bools)]
pub struct CurveApp {
    source: Arc<SourceImage>,
    prepared: Arc<PreparedImage>,
    worker: PreviewWorker,
    image_loader: ImageLoader,
    image_request: u64,
    loading_image: bool,
    image_error: Option<String>,
    pending_transparency_path: Option<PathBuf>,
    current_path: Option<PathBuf>,
    curves: CurveSet,
    mode: CurveMode,
    interpolation: CurveInterpolation,
    luminance_definition: LuminanceDefinition,
    histogram_calculation: HistogramCalculation,
    input_colour_space: InputColourSpace,
    channel: CurveChannel,
    graph_mode: GraphMode,
    derivative_editor: Option<DerivativeCurve>,
    view: SyncedView,
    before_texture: Option<TextureHandle>,
    after_texture: Option<TextureHandle>,
    rendered: Option<RenderedPreview>,
    latest_request: u64,
    progress: f32,
    rendering: bool,
    superseded_render: bool,
    histogram_stale: bool,
    drag: Option<CurveDrag>,
    last_export: Option<PathBuf>,
}

impl CurveApp {
    pub fn new(egui_context: &egui::Context, source: SourceImage, prepared: PreparedImage) -> Self {
        let source = Arc::new(source);
        let prepared = Arc::new(prepared);
        let input_colour_space = prepared.input_colour_space;
        let mut worker = PreviewWorker::new(prepared.clone());
        let curves = CurveSet::default();
        let latest_request = worker.request(PipelineSnapshot {
            mode: CurveMode::LinkedRgb,
            curves: curves.clone(),
            luminance: LuminanceDefinition::AdobeRgb,
            interpolation: CurveInterpolation::Smooth,
            histogram_calculation: HistogramCalculation::FullResolution,
        });
        set_visuals(egui_context);
        Self {
            source,
            prepared,
            worker,
            image_loader: ImageLoader::new(),
            image_request: 0,
            loading_image: false,
            image_error: None,
            pending_transparency_path: None,
            current_path: None,
            curves,
            mode: CurveMode::LinkedRgb,
            interpolation: CurveInterpolation::Smooth,
            luminance_definition: LuminanceDefinition::AdobeRgb,
            histogram_calculation: HistogramCalculation::FullResolution,
            input_colour_space,
            channel: CurveChannel::Red,
            graph_mode: GraphMode::ToneCurve,
            derivative_editor: None,
            view: SyncedView::default(),
            before_texture: None,
            after_texture: None,
            rendered: None,
            latest_request,
            progress: 0.0,
            rendering: true,
            superseded_render: false,
            histogram_stale: true,
            drag: None,
            last_export: None,
        }
    }

    fn open_image(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Images", &["png", "jpg", "jpeg"])
            .pick_file()
        else {
            return;
        };
        self.image_request = self.image_loader.open(path);
        self.loading_image = true;
        self.image_error = None;
        self.invalidate_preview();
    }

    fn request_colour_space_change(&mut self) {
        self.image_request = self
            .image_loader
            .reprepare(self.prepared.clone(), self.input_colour_space);
        self.loading_image = true;
        self.image_error = None;
        self.invalidate_preview();
    }

    fn invalidate_preview(&mut self) {
        // The old worker may still finish while the image loader is reading a
        // replacement. Make every event from that worker stale immediately.
        self.latest_request = self.latest_request.wrapping_add(1);
        self.worker.cancel_active();
        self.rendering = false;
        self.superseded_render = false;
        self.progress = 0.0;
        self.histogram_stale = true;
        self.rendered = None;
        self.before_texture = None;
        self.after_texture = None;
        self.last_export = None;
        self.derivative_editor = None;
    }

    fn install_prepared(&mut self, prepared: PreparedImage) {
        self.prepared = Arc::new(prepared);
        self.worker = PreviewWorker::new(self.prepared.clone());
        self.before_texture = None;
        self.after_texture = None;
        self.rendered = None;
        self.derivative_editor = None;
        self.request_render();
    }

    fn receive_image_events(&mut self) {
        for event in self.image_loader.poll() {
            match event {
                ImageEvent::Opened {
                    id,
                    path,
                    source,
                    prepared,
                } if id == self.image_request => {
                    self.source = source;
                    self.current_path = Some(path);
                    self.input_colour_space = prepared.input_colour_space;
                    self.loading_image = false;
                    self.image_error = None;
                    self.last_export = None;
                    self.derivative_editor = None;
                    self.install_prepared(prepared);
                }
                ImageEvent::Reprepared { id, prepared } if id == self.image_request => {
                    self.loading_image = false;
                    self.image_error = None;
                    self.last_export = None;
                    self.derivative_editor = None;
                    self.install_prepared(prepared);
                }
                ImageEvent::Failed { id, message } if id == self.image_request => {
                    self.loading_image = false;
                    self.image_error = Some(message);
                }
                ImageEvent::TransparencyConfirmationRequired { id, path }
                    if id == self.image_request =>
                {
                    self.loading_image = false;
                    self.pending_transparency_path = Some(path);
                }
                _ => {}
            }
        }
    }

    fn request_render(&mut self) {
        let superseded = self.rendering;
        self.latest_request = self.worker.request(PipelineSnapshot {
            mode: self.mode,
            curves: self.curves.clone(),
            luminance: self.luminance_definition,
            interpolation: self.interpolation,
            histogram_calculation: self.histogram_calculation,
        });
        self.progress = 0.0;
        self.rendering = true;
        self.superseded_render = superseded;
        self.histogram_stale = true;
        self.last_export = None;
        // Do not show a histogram calculated for a previous mode or curve
        // beside the new graph while the latest immutable snapshot renders.
        self.rendered = None;
    }

    fn receive_render_events(&mut self, context: &egui::Context) {
        for event in self.worker.poll() {
            match event {
                RenderEvent::Progress { id, fraction } if id == self.latest_request => {
                    self.progress = fraction;
                    context.request_repaint();
                }
                RenderEvent::Finished { id, preview } if id == self.latest_request => {
                    self.progress = 1.0;
                    self.rendering = false;
                    self.superseded_render = false;
                    self.histogram_stale = false;
                    self.update_textures(context, &preview);
                    self.rendered = Some(preview);
                }
                _ => {}
            }
        }
    }

    fn update_textures(&mut self, context: &egui::Context, preview: &RenderedPreview) {
        let before = egui::ColorImage::from_rgba_unmultiplied(
            [preview.width as usize, preview.height as usize],
            &preview.before_rgba,
        );
        let after = egui::ColorImage::from_rgba_unmultiplied(
            [preview.width as usize, preview.height as usize],
            &preview.after_rgba,
        );
        self.before_texture = Some(context.load_texture(
            format!("before-preview-{}", self.latest_request),
            before,
            egui::TextureOptions::LINEAR,
        ));
        self.after_texture = Some(context.load_texture(
            format!("after-preview-{}", self.latest_request),
            after,
            egui::TextureOptions::LINEAR,
        ));
    }

    #[allow(clippy::too_many_lines)]
    fn show_toolbar(&mut self, ui: &mut Ui) {
        let previous_mode = self.mode;
        let previous_channel = self.channel;
        let previous_interpolation = self.interpolation;
        let previous_luminance = self.luminance_definition;
        let previous_colour_space = self.input_colour_space;
        ui.horizontal(|ui| {
            ui.heading("Exposure curve");
            if ui
                .button("Open image…")
                .on_hover_text("Open a PNG or JPEG and inspect its colour metadata")
                .clicked()
            {
                self.open_image();
            }
            ui.separator();
            ui.label(egui::RichText::new(self.mode.label()).strong());
            ui.label("•");
            ui.label("Adobe RGB curve domain").on_hover_text(format!(
                "The curve sees {CURVE_DOMAIN_LABEL} values from 0.0 to 1.0, not linear light or unbounded RAW data."
            ));
            ui.add_space(12.0);
            ui.label("Mode");
            egui::ComboBox::from_id_salt("curve-mode")
                .selected_text(self.mode.label())
                .show_ui(ui, |ui| {
                    for mode in CurveMode::ALL {
                        ui.selectable_value(&mut self.mode, mode, mode.label())
                            .on_hover_text(mode.description());
                    }
                });
            if self.mode == CurveMode::PerChannelRgb {
                for channel in CurveChannel::ALL {
                    ui.selectable_value(&mut self.channel, channel, channel.label());
                }
            }
            ui.label("Interpolation");
            egui::ComboBox::from_id_salt("curve-interpolation")
                .selected_text(self.interpolation.label())
                .show_ui(ui, |ui| {
                    for interpolation in CurveInterpolation::ALL {
                        ui.selectable_value(
                            &mut self.interpolation,
                            interpolation,
                            interpolation.label(),
                        )
                        .on_hover_text(interpolation.description());
                    }
                });
            if self.mode == CurveMode::Luminance {
                ui.label("Luminance definition");
                egui::ComboBox::from_id_salt("luminance-definition")
                    .selected_text(self.luminance_definition.label())
                    .show_ui(ui, |ui| {
                        for definition in LuminanceDefinition::ALL {
                            ui.selectable_value(
                                &mut self.luminance_definition,
                                definition,
                                definition.label(),
                            );
                        }
                    });
            }
            if ui
                .add_sized([25.0, 25.0], egui::Button::new("↺"))
                .on_hover_text("Reset the selected curve mode")
                .clicked()
            {
                self.curves.reset_mode(self.mode);
                self.derivative_editor = None;
                self.request_render();
            }
        });
        if self.mode != previous_mode
            || self.channel != previous_channel
            || self.interpolation != previous_interpolation
            || self.luminance_definition != previous_luminance
        {
            self.derivative_editor = None;
            self.request_render();
        }
        if self.input_colour_space != previous_colour_space {
            self.request_colour_space_change();
        }
        ui.add_space(2.0);
        ui.horizontal(|ui| {
            let profile = &self.prepared.profile;
            ui.label(
                egui::RichText::new(format!(
                    "Input  •  {}  •  {}  •  {}-bit {}  •  {}  •  ICC {} bytes  •  {}",
                    self.current_path
                        .as_deref()
                        .and_then(|path| path.file_name())
                        .and_then(|name| name.to_str())
                        .unwrap_or("controlled fixture"),
                    self.input_colour_space.label(),
                    self.prepared.bit_depth,
                    self.prepared.format.label(),
                    profile.label,
                    profile.byte_length,
                    profile.detection_source,
                ))
                .small()
                .color(Color32::from_rgb(161, 172, 180)),
            );
            ui.label("Input space");
            ui.add_enabled_ui(
                input_colour_space_controls_enabled(self.loading_image),
                |ui| {
                    egui::ComboBox::from_id_salt("input-colour-space")
                        .selected_text(self.input_colour_space.label())
                        .show_ui(ui, |ui| {
                            for colour_space in InputColourSpace::ALL {
                                ui.selectable_value(
                                    &mut self.input_colour_space,
                                    colour_space,
                                    colour_space.label(),
                                );
                            }
                        });
                },
            );
            ui.add_space(12.0);
            if self.loading_image {
                ui.spinner();
                ui.label("Loading image…");
            } else if self.rendering {
                ui.spinner();
                let status = if self.superseded_render {
                    format!(
                        "Rendering newest state  {:.0}%  •  older render cancelled",
                        self.progress * 100.0
                    )
                } else {
                    format!("Rendering newest state  {:.0}%", self.progress * 100.0)
                };
                ui.label(status);
            } else if let Some(preview) = &self.rendered {
                ui.label(
                    egui::RichText::new(format!("Preview ready  •  {} ms", preview.duration_ms))
                        .small()
                        .color(Color32::from_rgb(137, 190, 159)),
                );
            }
            if let Some(path) = &self.last_export {
                ui.label(
                    egui::RichText::new(format!("Exported {}", path.display()))
                        .small()
                        .color(Color32::from_rgb(180, 179, 145)),
                );
            }
            if let Some(error) = &self.image_error {
                ui.label(
                    egui::RichText::new(error)
                        .small()
                        .color(Color32::from_rgb(232, 126, 114)),
                );
            }
        });
    }

    fn show_previews(&mut self, ui: &mut Ui) {
        let row_height = (ui.available_height() * 0.51).clamp(240.0, 430.0);
        let before_texture = self.before_texture.clone();
        let after_texture = self.after_texture.clone();
        ui.horizontal(|ui| {
            let panel_width =
                ((ui.available_width() - ui.spacing().item_spacing.x) / 2.0).max(180.0);
            ui.allocate_ui(Vec2::new(panel_width, row_height), |ui| {
                self.preview_panel(ui, "Before", before_texture.as_ref(), false);
            });
            ui.allocate_ui(Vec2::new(panel_width, row_height), |ui| {
                self.preview_panel(ui, "After", after_texture.as_ref(), true);
            });
        });
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("Before and After share zoom and pan")
                    .small()
                    .color(Color32::from_rgb(160, 169, 179)),
            );
            ui.label(format!("Zoom {:.0}%", self.view.zoom * 100.0));
            if ui.small_button("Reset view").clicked() {
                self.view = SyncedView::default();
            }
            ui.add_space(10.0);
            ui.label(
                egui::RichText::new(
                    "Scroll over a preview to zoom • drag to pan • After updates on pointer release",
                )
                .small()
                .italics()
                .color(Color32::from_rgb(146, 155, 165)),
            );
        });
    }

    fn preview_panel(
        &mut self,
        ui: &mut Ui,
        title: &str,
        texture: Option<&TextureHandle>,
        emphasise: bool,
    ) {
        ui.label(egui::RichText::new(title).strong().color(if emphasise {
            ACCENT
        } else {
            Color32::from_rgb(205, 211, 216)
        }));
        let image_size = Vec2::new(
            ui.available_width(),
            (ui.available_height() - 25.0).max(120.0),
        );
        let (rect, response) = ui.allocate_exact_size(image_size, Sense::drag());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, CornerRadius::same(5), PANEL_BACKGROUND);

        if response.hovered() {
            let scroll = ui.input(|input| input.smooth_scroll_delta.y);
            if scroll.abs() > 0.0 {
                self.view.zoom = (self.view.zoom * (1.0 + scroll * 0.0015)).clamp(0.5, 6.0);
            }
        }
        if response.dragged() {
            self.view.pan += response.drag_delta();
        }

        if let Some(texture) = texture {
            let aspect = self.prepared.width as f32 / self.prepared.height as f32;
            let base = fit_rect(rect.shrink(4.0), aspect);
            let image_size = base.size() * self.view.zoom;
            let image_rect = Rect::from_center_size(base.center() + self.view.pan, image_size);
            painter.with_clip_rect(rect).image(
                texture.id(),
                image_rect,
                Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                Color32::WHITE,
            );
        } else {
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                if self.rendering {
                    "Preparing preview…"
                } else {
                    "No preview"
                },
                egui::FontId::proportional(14.0),
                Color32::from_rgb(140, 149, 159),
            );
        }
        painter.rect_stroke(
            rect,
            CornerRadius::same(5),
            Stroke::new(1.0, Color32::from_rgb(54, 62, 70)),
            StrokeKind::Inside,
        );
    }

    #[allow(clippy::too_many_lines)]
    fn show_curve_editor(&mut self, ui: &mut Ui, context: &egui::Context) {
        let previous_histogram_calculation = self.histogram_calculation;
        let previous_graph_mode = self.graph_mode;
        ui.separator();
        ui.horizontal(|ui| {
            ui.heading("Curve editor");
            ui.add_space(12.0);
            ui.label("Graph");
            for graph_mode in [GraphMode::ToneCurve, GraphMode::Derivative] {
                ui.selectable_value(&mut self.graph_mode, graph_mode, graph_mode.label());
            }
            ui.label("Histogram calculation");
            egui::ComboBox::from_id_salt("histogram-calculation")
                .selected_text(self.histogram_calculation.label())
                .show_ui(ui, |ui| {
                    for calculation in HistogramCalculation::ALL {
                        ui.selectable_value(
                            &mut self.histogram_calculation,
                            calculation,
                            calculation.label(),
                        )
                        .on_hover_text(calculation.description());
                    }
                });
            ui.label(
                egui::RichText::new(self.interpolation.label())
                    .small()
                    .color(Color32::from_rgb(151, 164, 176)),
            );
        });
        if self.histogram_calculation != previous_histogram_calculation {
            self.request_render();
        }
        if self.graph_mode != previous_graph_mode {
            self.derivative_editor = None;
            self.drag = None;
        }
        if self.graph_mode == GraphMode::Derivative && self.derivative_editor.is_none() {
            let curve = self.curves.curve(self.mode, self.channel);
            self.derivative_editor = Some(curve.derivative_curve(self.interpolation));
        }

        let graph_height = ui.available_height().max(220.0);
        let (rect, response) = ui.allocate_exact_size(
            Vec2::new(ui.available_width(), graph_height),
            Sense::click_and_drag(),
        );
        let plot = rect.shrink2(Vec2::new(58.0, 23.0));
        self.draw_curve_graph(ui, plot, rect);
        let derivative_range = self
            .derivative_editor
            .as_ref()
            .map(|curve| derivative_range(curve, self.interpolation));

        if response.secondary_clicked()
            && self.drag.is_none()
            && let Some(pointer) = response.interact_pointer_pos()
        {
            let target = graph_value_for_mode(plot, pointer, self.graph_mode, derivative_range);
            if self.add_graph_point(target) {
                self.request_render();
            }
        }
        if response.middle_clicked()
            && self.drag.is_none()
            && let Some(pointer) = response.interact_pointer_pos()
        {
            let changed = self.remove_or_reset_at(plot, pointer, derivative_range);
            if changed {
                self.request_render();
            }
        }
        if response.drag_started_by(PointerButton::Primary)
            && let Some(pointer) = response.interact_pointer_pos()
        {
            self.begin_graph_drag(plot, pointer, derivative_range);
        }
        if response.dragged_by(PointerButton::Primary)
            && let Some(pointer) = response.interact_pointer_pos()
        {
            if self.interpolation == CurveInterpolation::Smooth {
                let scroll = ui.input(|input| input.smooth_scroll_delta.y);
                if scroll.abs() > f32::EPSILON
                    && let Some(drag) = self.drag.as_mut()
                    && drag.target == DragTarget::Point
                {
                    drag.tension_delta = (drag.tension_delta + scroll * 0.003).clamp(-0.9, 3.0);
                }
            }
            if let Some(drag) = self.drag.clone() {
                self.update_graph_drag(plot, pointer, derivative_range, &drag);
                context.request_repaint();
            }
        }
        if response.drag_stopped_by(PointerButton::Primary) && self.drag.take().is_some() {
            self.request_render();
        }

        ui.horizontal(|ui| {
            let description = if self.graph_mode == GraphMode::Derivative {
                "Derivative mode: d(output) / d(input); identity is a horizontal line at slope 1"
            } else {
                "Input brightness → output brightness"
            };
            ui.label(
                egui::RichText::new(description)
                    .small()
                    .color(Color32::from_rgb(158, 169, 179)),
            );
            ui.label(
                egui::RichText::new("Right-click add on the existing function • left-drag move • middle-click delete/reset")
                    .small()
                    .color(Color32::from_rgb(176, 165, 139)),
            );
            if self.interpolation == CurveInterpolation::Smooth {
                ui.label(
                    egui::RichText::new("Scroll while dragging a point to adjust its smoothness")
                        .small()
                        .color(Color32::from_rgb(163, 175, 188)),
                );
            }
            if self.mode == CurveMode::Luminance {
                ui.label(
                    egui::RichText::new(format!(
                        "Luminance definition: {}",
                        self.luminance_definition.label()
                    ))
                    .small()
                    .color(Color32::from_rgb(188, 169, 130)),
                );
            }
        });
    }

    fn begin_graph_drag(
        &mut self,
        plot: Rect,
        pointer: Pos2,
        derivative_range: Option<(f32, f32)>,
    ) {
        let target = if self.graph_mode == GraphMode::ToneCurve {
            let curve = self.curves.curve(self.mode, self.channel);
            nearest_curve_target(curve, plot, pointer, self.interpolation)
        } else {
            let Some(curve) = self.derivative_editor.as_ref() else {
                return;
            };
            nearest_derivative_target(
                curve,
                plot,
                pointer,
                self.interpolation,
                derivative_range.unwrap_or((0.0, 2.0)),
            )
        };
        let Some((index, target)) = target else {
            return;
        };
        self.histogram_stale = true;
        let initial = self.curves.curve(self.mode, self.channel).clone();
        let initial_derivative = if self.graph_mode == GraphMode::Derivative {
            self.derivative_editor.clone()
        } else {
            None
        };
        self.drag = Some(CurveDrag {
            index,
            target,
            mode: self.mode,
            channel: self.channel,
            initial,
            initial_derivative,
            tension_delta: 0.0,
        });
    }

    fn update_graph_drag(
        &mut self,
        plot: Rect,
        pointer: Pos2,
        derivative_range: Option<(f32, f32)>,
        drag: &CurveDrag,
    ) {
        let target = graph_value_for_mode(plot, pointer, self.graph_mode, derivative_range);
        if self.graph_mode == GraphMode::ToneCurve {
            let mut updated = match drag.target {
                DragTarget::Point => {
                    Curve::dragged_from(&drag.initial, drag.index, target[0], target[1])
                }
                DragTarget::Handle(kind) => Curve::dragged_handle_from(
                    &drag.initial,
                    drag.index,
                    kind,
                    target[0],
                    target[1],
                ),
            };
            if self.interpolation == CurveInterpolation::Smooth && drag.target == DragTarget::Point
            {
                updated.set_tension(
                    drag.index,
                    drag.initial.tension(drag.index) + drag.tension_delta,
                );
            }
            *self.curves.curve_mut(drag.mode, drag.channel) = updated;
            self.derivative_editor = None;
        } else if let Some(initial_derivative) = drag.initial_derivative.as_ref() {
            let mut updated_derivative = match drag.target {
                DragTarget::Point => DerivativeCurve::dragged_from(
                    initial_derivative,
                    drag.index,
                    target[0],
                    target[1],
                ),
                DragTarget::Handle(kind) => DerivativeCurve::dragged_handle_from(
                    initial_derivative,
                    drag.index,
                    kind,
                    target[0],
                    target[1],
                ),
            };
            if self.interpolation == CurveInterpolation::Smooth && drag.target == DragTarget::Point
            {
                updated_derivative.set_tension(
                    drag.index,
                    initial_derivative.tension(drag.index) + drag.tension_delta,
                );
            }
            let updated_tone = Curve::apply_derivative_edit(
                &drag.initial,
                initial_derivative,
                &updated_derivative,
                self.interpolation,
            );
            *self.curves.curve_mut(drag.mode, drag.channel) = updated_tone;
            self.derivative_editor = Some(updated_derivative);
        }
    }

    fn add_graph_point(&mut self, target: [f32; 2]) -> bool {
        if self.graph_mode == GraphMode::ToneCurve {
            let curve = self.curves.curve_mut(self.mode, self.channel);
            return curve.insert_point_on_curve(target[0], self.interpolation);
        }
        let current = self.curves.curve(self.mode, self.channel).clone();
        let Some(editor) = self.derivative_editor.as_ref() else {
            return false;
        };
        let mut edited = editor.clone();
        if !edited.insert_point_on_curve(target[0], self.interpolation) {
            return false;
        }
        edited.set_point_y_near(target[0].clamp(0.0, 1.0), target[1], 1e-5);
        let tone_with_anchor = {
            let mut tone = current.clone();
            if !tone.insert_point_on_curve(target[0], self.interpolation) {
                return false;
            }
            tone
        };
        let baseline = tone_with_anchor.derivative_curve(self.interpolation);
        let updated =
            Curve::apply_derivative_edit(&tone_with_anchor, &baseline, &edited, self.interpolation);
        *self.curves.curve_mut(self.mode, self.channel) = updated;
        self.derivative_editor = Some(edited);
        true
    }

    fn remove_or_reset_at(
        &mut self,
        plot: Rect,
        pointer: Pos2,
        derivative_range: Option<(f32, f32)>,
    ) -> bool {
        if self.graph_mode == GraphMode::ToneCurve {
            let curve = self.curves.curve(self.mode, self.channel).clone();
            if self.interpolation == CurveInterpolation::Bezier
                && let Some((index, kind, _)) = nearest_curve_handle(&curve, plot, pointer)
            {
                return self
                    .curves
                    .curve_mut(self.mode, self.channel)
                    .reset_handle(index, kind);
            }
            let Some((index, _)) = nearest_point(&curve, plot, pointer) else {
                return false;
            };
            return self
                .curves
                .curve_mut(self.mode, self.channel)
                .delete_point(index);
        }
        let Some(editor) = self.derivative_editor.as_ref() else {
            return false;
        };
        let range = derivative_range.unwrap_or((0.0, 2.0));
        let mut edited = editor.clone();
        if self.interpolation == CurveInterpolation::Bezier
            && let Some((index, kind, _)) = nearest_derivative_handle(editor, plot, pointer, range)
        {
            if !edited.reset_handle(index, kind) {
                return false;
            }
            return self.apply_derivative_editor_change(edited);
        }
        let Some((index, _)) = nearest_derivative_point(editor, plot, pointer, range) else {
            return false;
        };
        if !edited.delete_point(index) {
            return false;
        }
        let mut tone = self.curves.curve(self.mode, self.channel).clone();
        if !tone.delete_point(index) {
            return false;
        }
        *self.curves.curve_mut(self.mode, self.channel) = tone.clone();
        self.derivative_editor = Some(tone.derivative_curve(self.interpolation));
        true
    }

    fn apply_derivative_editor_change(&mut self, edited: DerivativeCurve) -> bool {
        let Some(initial_derivative) = self.derivative_editor.as_ref() else {
            return false;
        };
        let current = self.curves.curve(self.mode, self.channel).clone();
        let updated =
            Curve::apply_derivative_edit(&current, initial_derivative, &edited, self.interpolation);
        *self.curves.curve_mut(self.mode, self.channel) = updated;
        self.derivative_editor = Some(edited);
        true
    }

    fn draw_curve_graph(&self, ui: &Ui, plot: Rect, outer: Rect) {
        let painter = ui.painter_at(outer);
        painter.rect_filled(outer, CornerRadius::same(5), GRAPH_BACKGROUND);
        painter.rect_filled(plot, CornerRadius::same(2), Color32::from_rgb(23, 26, 30));
        let range = self
            .derivative_editor
            .as_ref()
            .map(|curve| derivative_range(curve, self.interpolation));
        draw_graph_grid(&painter, plot, self.graph_mode, range);

        if let Some(rendered) = &self.rendered
            && histogram_is_current(true, self.histogram_stale)
        {
            draw_input_histogram(&painter, plot, &rendered.input_histogram);
            if self.graph_mode == GraphMode::ToneCurve {
                draw_output_histogram(&painter, plot, &rendered.output_histogram);
            }
            let label = if rendered.input_histogram.approximate {
                format!(
                    "Rec. 709 luminance histograms • {} • approximate bins",
                    rendered.input_histogram.calculation.label()
                )
            } else {
                "Rec. 709 luminance histograms".to_owned()
            };
            painter.text(
                Pos2::new(plot.left() + 8.0, plot.top() + 8.0),
                egui::Align2::LEFT_TOP,
                label,
                egui::FontId::proportional(10.0),
                Color32::from_rgba_unmultiplied(206, 214, 221, 160),
            );
        } else if self.histogram_stale {
            painter.text(
                Pos2::new(plot.left() + 8.0, plot.top() + 8.0),
                egui::Align2::LEFT_TOP,
                "Histograms • waiting for latest preview",
                egui::FontId::proportional(10.0),
                Color32::from_rgba_unmultiplied(206, 214, 221, 160),
            );
        }

        let colour = curve_colour(self.mode, self.channel);
        if self.graph_mode == GraphMode::ToneCurve {
            let curve = self.curves.curve(self.mode, self.channel);
            let points: Vec<Pos2> = curve
                .sample_with_interpolation(180, self.interpolation)
                .into_iter()
                .map(|sample| graph_screen(plot, sample))
                .collect();
            painter.add(egui::Shape::line(points, Stroke::new(2.5, colour)));
            draw_tone_controls(
                &painter,
                plot,
                curve,
                self.interpolation,
                colour,
                self.drag.as_ref(),
            );
        } else if let Some(curve) = &self.derivative_editor {
            let range = range.unwrap_or((0.0, 2.0));
            let points: Vec<Pos2> = curve
                .sample_with_interpolation(180, self.interpolation)
                .into_iter()
                .map(|sample| graph_screen_scaled(plot, sample, range))
                .collect();
            painter.add(egui::Shape::line(points, Stroke::new(2.5, colour)));
            draw_derivative_controls(
                &painter,
                plot,
                curve,
                self.interpolation,
                range,
                colour,
                self.drag.as_ref(),
            );
        }
        painter.rect_stroke(
            plot,
            CornerRadius::same(2),
            Stroke::new(1.0, Color32::from_rgb(70, 78, 87)),
            StrokeKind::Inside,
        );
    }

    fn export(&mut self) {
        let Some(preview) = &self.rendered else {
            return;
        };
        let Some(path) = rfd::FileDialog::new()
            .add_filter("PNG image", &["png"])
            .set_file_name("exposure-curve-preview.png")
            .save_file()
        else {
            return;
        };
        match write_srgb_png(&path, preview) {
            Ok(()) => self.last_export = Some(path),
            Err(error) => self.image_error = Some(error.to_string()),
        }
    }

    fn show_transparency_confirmation(&mut self, context: &egui::Context) {
        let Some(path) = self.pending_transparency_path.clone() else {
            return;
        };
        egui::Window::new("Flatten transparent image?")
            .collapsible(false)
            .resizable(false)
            .show(context, |ui| {
                ui.label("This image contains transparency.");
                ui.label("Continue by flattening it over black?");
                ui.horizontal(|ui| {
                    if ui.button("Flatten and open").clicked() {
                        self.pending_transparency_path = None;
                        self.image_request = self.image_loader.open_confirmed_flatten(path.clone());
                        self.loading_image = true;
                    }
                    if ui.button("Cancel").clicked() {
                        self.pending_transparency_path = None;
                    }
                });
            });
    }
}

impl eframe::App for CurveApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let context = ui.ctx().clone();
        self.receive_image_events();
        self.receive_render_events(&context);
        if self.rendering || self.loading_image {
            context.request_repaint_after(Duration::from_millis(16));
        }

        egui::Panel::top("toolbar")
            .frame(egui::Frame::NONE.inner_margin(egui::Margin::symmetric(18, 11)))
            .show(ui, |ui| {
                self.show_toolbar(ui);
            });

        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.inner_margin(egui::Margin::symmetric(18, 8)))
            .show(ui, |ui| {
                self.show_previews(ui);
                self.show_curve_editor(ui, &context);
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("Pipeline")
                            .strong()
                            .color(Color32::from_rgb(191, 204, 211)),
                    );
                    ui.label(
                        egui::RichText::new(
                            "read metadata → selected input space → Adobe RGB curve → 8-bit sRGB PNG",
                        )
                        .small()
                        .color(Color32::from_rgb(148, 160, 171)),
                    );
                    ui.add_space(10.0);
                    if ui
                        .add_enabled(
                            self.rendered.is_some()
                                && !self.rendering
                                && !self.loading_image
                                && !self.histogram_stale,
                            egui::Button::new("Export PNG"),
                        )
                        .on_hover_text("Write the latest After preview as an 8-bit sRGB PNG")
                        .clicked()
                    {
                        self.export();
                    }
                });
            });
        self.show_transparency_confirmation(&context);
    }
}

fn set_visuals(context: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    visuals.window_fill = Color32::from_rgb(28, 33, 38);
    visuals.panel_fill = Color32::from_rgb(31, 36, 42);
    visuals.extreme_bg_color = Color32::from_rgb(15, 18, 21);
    visuals.widgets.noninteractive.bg_fill = Color32::from_rgb(38, 44, 51);
    visuals.widgets.inactive.bg_fill = Color32::from_rgb(41, 48, 55);
    visuals.widgets.hovered.bg_fill = Color32::from_rgb(54, 66, 75);
    visuals.widgets.active.bg_fill = Color32::from_rgb(67, 92, 99);
    context.set_visuals(visuals);
}

fn fit_rect(rect: Rect, aspect: f32) -> Rect {
    let available_aspect = rect.width() / rect.height().max(1.0);
    if available_aspect > aspect {
        let width = rect.height() * aspect;
        Rect::from_center_size(rect.center(), Vec2::new(width, rect.height()))
    } else {
        let height = rect.width() / aspect.max(0.01);
        Rect::from_center_size(rect.center(), Vec2::new(rect.width(), height))
    }
}

fn graph_screen(plot: Rect, point: [f32; 2]) -> Pos2 {
    Pos2::new(
        egui::lerp(plot.left()..=plot.right(), point[0]),
        egui::lerp(plot.bottom()..=plot.top(), point[1]),
    )
}

fn graph_screen_scaled(plot: Rect, point: [f32; 2], range: (f32, f32)) -> Pos2 {
    let y = ((point[1] - range.0) / (range.1 - range.0).max(f32::EPSILON)).clamp(0.0, 1.0);
    Pos2::new(
        egui::lerp(plot.left()..=plot.right(), point[0].clamp(0.0, 1.0)),
        egui::lerp(plot.bottom()..=plot.top(), y),
    )
}

fn graph_value_for_mode(
    plot: Rect,
    position: Pos2,
    mode: GraphMode,
    derivative_range: Option<(f32, f32)>,
) -> [f32; 2] {
    let x = ((position.x - plot.left()) / plot.width()).clamp(0.0, 1.0);
    let unit_y = ((plot.bottom() - position.y) / plot.height()).clamp(0.0, 1.0);
    if mode == GraphMode::ToneCurve {
        [x, unit_y]
    } else {
        let range = derivative_range.unwrap_or((0.0, 2.0));
        [x, egui::lerp(range.0..=range.1, unit_y)]
    }
}

fn nearest_point(curve: &Curve, plot: Rect, pointer: Pos2) -> Option<(usize, f32)> {
    curve
        .points()
        .iter()
        .enumerate()
        .map(|(index, point)| {
            (
                index,
                graph_screen(plot, [point.x, point.y]).distance(pointer),
            )
        })
        .filter(|(_, distance)| *distance <= 18.0)
        .min_by(|left, right| left.1.total_cmp(&right.1))
}

fn nearest_curve_target(
    curve: &Curve,
    plot: Rect,
    pointer: Pos2,
    interpolation: CurveInterpolation,
) -> Option<(usize, DragTarget)> {
    if interpolation == CurveInterpolation::Bezier
        && let Some((index, kind, _)) = nearest_curve_handle(curve, plot, pointer)
    {
        return Some((index, DragTarget::Handle(kind)));
    }
    nearest_point(curve, plot, pointer).map(|(index, _)| (index, DragTarget::Point))
}

fn nearest_curve_handle(
    curve: &Curve,
    plot: Rect,
    pointer: Pos2,
) -> Option<(usize, BezierHandleKind, f32)> {
    curve
        .bezier_handles()
        .iter()
        .enumerate()
        .flat_map(|(index, handles)| {
            [
                (BezierHandleKind::Incoming, handles.incoming),
                (BezierHandleKind::Outgoing, handles.outgoing),
            ]
            .into_iter()
            .filter_map(move |(kind, handle)| {
                handle.map(|handle| {
                    (
                        index,
                        kind,
                        graph_screen(plot, [handle.x, handle.y]).distance(pointer),
                    )
                })
            })
        })
        .filter(|(_, _, distance)| *distance <= 14.0)
        .min_by(|left, right| left.2.total_cmp(&right.2))
}

fn nearest_derivative_point(
    curve: &DerivativeCurve,
    plot: Rect,
    pointer: Pos2,
    range: (f32, f32),
) -> Option<(usize, f32)> {
    curve
        .points()
        .iter()
        .enumerate()
        .map(|(index, point)| {
            (
                index,
                graph_screen_scaled(plot, [point.x, point.y], range).distance(pointer),
            )
        })
        .filter(|(_, distance)| *distance <= 18.0)
        .min_by(|left, right| left.1.total_cmp(&right.1))
}

fn nearest_derivative_target(
    curve: &DerivativeCurve,
    plot: Rect,
    pointer: Pos2,
    interpolation: CurveInterpolation,
    range: (f32, f32),
) -> Option<(usize, DragTarget)> {
    if interpolation == CurveInterpolation::Bezier
        && let Some((index, kind, _)) = nearest_derivative_handle(curve, plot, pointer, range)
    {
        return Some((index, DragTarget::Handle(kind)));
    }
    nearest_derivative_point(curve, plot, pointer, range)
        .map(|(index, _)| (index, DragTarget::Point))
}

fn nearest_derivative_handle(
    curve: &DerivativeCurve,
    plot: Rect,
    pointer: Pos2,
    range: (f32, f32),
) -> Option<(usize, BezierHandleKind, f32)> {
    curve
        .bezier_handles()
        .iter()
        .enumerate()
        .flat_map(|(index, handles)| {
            [
                (BezierHandleKind::Incoming, handles.incoming),
                (BezierHandleKind::Outgoing, handles.outgoing),
            ]
            .into_iter()
            .filter_map(move |(kind, handle)| {
                handle.map(|handle| {
                    (
                        index,
                        kind,
                        graph_screen_scaled(plot, [handle.x, handle.y], range).distance(pointer),
                    )
                })
            })
        })
        .filter(|(_, _, distance)| *distance <= 14.0)
        .min_by(|left, right| left.2.total_cmp(&right.2))
}

fn derivative_range(curve: &DerivativeCurve, interpolation: CurveInterpolation) -> (f32, f32) {
    let mut values: Vec<f32> = curve
        .sample_with_interpolation(180, interpolation)
        .into_iter()
        .map(|point| point[1])
        .chain(curve.points().iter().map(|point| point.y))
        .chain(
            curve
                .bezier_handles()
                .iter()
                .flat_map(|handles| [handles.incoming, handles.outgoing])
                .flatten()
                .map(|point| point.y),
        )
        .filter(|value| value.is_finite())
        .collect();
    if values.is_empty() {
        return (0.0, 2.0);
    }
    let mut minimum = values.drain(..).fold(f32::INFINITY, f32::min);
    let mut maximum = values.into_iter().fold(f32::NEG_INFINITY, f32::max);
    minimum = minimum.min(0.0);
    maximum = maximum.max(0.0);
    if minimum >= 0.0 && maximum <= 1.0 {
        // Keep a useful editing margin below zero even for an identity curve;
        // negative derivatives are valid and should be reachable immediately.
        return (-1.0, 2.0);
    }
    let span = (maximum - minimum).max(0.5);
    (minimum - span * 0.1, maximum + span * 0.1)
}

fn draw_graph_grid(
    painter: &egui::Painter,
    plot: Rect,
    mode: GraphMode,
    derivative_range: Option<(f32, f32)>,
) {
    let range = derivative_range.unwrap_or((0.0, 1.0));
    for step in 0..=4 {
        let fraction = step as f32 / 4.0;
        let x = egui::lerp(plot.left()..=plot.right(), fraction);
        let y = egui::lerp(plot.bottom()..=plot.top(), fraction);
        painter.line_segment(
            [Pos2::new(x, plot.top()), Pos2::new(x, plot.bottom())],
            Stroke::new(1.0, GRID),
        );
        painter.line_segment(
            [Pos2::new(plot.left(), y), Pos2::new(plot.right(), y)],
            Stroke::new(1.0, GRID),
        );
        painter.text(
            Pos2::new(x, plot.bottom() + 5.0),
            egui::Align2::CENTER_TOP,
            format!("{}", step * 25),
            egui::FontId::proportional(10.0),
            Color32::from_rgb(142, 152, 163),
        );
        let label = if mode == GraphMode::ToneCurve {
            format!("{}", step * 25)
        } else {
            format!("{:.2}", egui::lerp(range.0..=range.1, fraction))
        };
        painter.text(
            Pos2::new(plot.left() - 7.0, y),
            egui::Align2::RIGHT_CENTER,
            label,
            egui::FontId::proportional(10.0),
            Color32::from_rgb(142, 152, 163),
        );
    }
    painter.text(
        Pos2::new(plot.left() - 45.0, plot.top() - 12.0),
        egui::Align2::LEFT_TOP,
        if mode == GraphMode::ToneCurve {
            "Output"
        } else {
            "dY / dX"
        },
        egui::FontId::proportional(10.0),
        Color32::from_rgb(153, 164, 174),
    );
}

fn draw_tone_controls(
    painter: &egui::Painter,
    plot: Rect,
    curve: &Curve,
    interpolation: CurveInterpolation,
    colour: Color32,
    drag: Option<&CurveDrag>,
) {
    if interpolation == CurveInterpolation::Bezier {
        for (index, handles) in curve.bezier_handles().iter().enumerate() {
            let anchor = graph_screen(plot, [curve.points()[index].x, curve.points()[index].y]);
            for (kind, handle) in [
                (BezierHandleKind::Incoming, handles.incoming),
                (BezierHandleKind::Outgoing, handles.outgoing),
            ] {
                let Some(handle) = handle else { continue };
                let screen = graph_screen(plot, [handle.x, handle.y]);
                painter.line_segment(
                    [anchor, screen],
                    Stroke::new(1.0, Color32::from_rgba_unmultiplied(170, 185, 195, 150)),
                );
                painter.rect_filled(
                    Rect::from_center_size(screen, Vec2::splat(7.0)),
                    CornerRadius::same(1),
                    if drag.is_some_and(|drag| {
                        drag.index == index && drag.target == DragTarget::Handle(kind)
                    }) {
                        ACCENT
                    } else {
                        Color32::from_rgb(153, 170, 180)
                    },
                );
            }
        }
    }
    for (index, point) in curve.points().iter().enumerate() {
        let screen = graph_screen(plot, [point.x, point.y]);
        painter.circle_filled(screen, 6.0, GRAPH_BACKGROUND);
        painter.circle_stroke(screen, 2.0, Stroke::new(2.0, colour));
        if drag.is_some_and(|drag| drag.index == index && drag.target == DragTarget::Point) {
            painter.circle_stroke(screen, 10.0, Stroke::new(1.0, ACCENT));
        }
    }
}

fn draw_derivative_controls(
    painter: &egui::Painter,
    plot: Rect,
    curve: &DerivativeCurve,
    interpolation: CurveInterpolation,
    range: (f32, f32),
    colour: Color32,
    drag: Option<&CurveDrag>,
) {
    if interpolation == CurveInterpolation::Bezier {
        for (index, handles) in curve.bezier_handles().iter().enumerate() {
            let anchor = graph_screen_scaled(
                plot,
                [curve.points()[index].x, curve.points()[index].y],
                range,
            );
            for (kind, handle) in [
                (BezierHandleKind::Incoming, handles.incoming),
                (BezierHandleKind::Outgoing, handles.outgoing),
            ] {
                let Some(handle) = handle else { continue };
                let screen = graph_screen_scaled(plot, [handle.x, handle.y], range);
                painter.line_segment(
                    [anchor, screen],
                    Stroke::new(1.0, Color32::from_rgba_unmultiplied(170, 185, 195, 150)),
                );
                painter.rect_filled(
                    Rect::from_center_size(screen, Vec2::splat(7.0)),
                    CornerRadius::same(1),
                    if drag.is_some_and(|drag| {
                        drag.index == index && drag.target == DragTarget::Handle(kind)
                    }) {
                        ACCENT
                    } else {
                        Color32::from_rgb(153, 170, 180)
                    },
                );
            }
        }
    }
    for (index, point) in curve.points().iter().enumerate() {
        let screen = graph_screen_scaled(plot, [point.x, point.y], range);
        painter.circle_filled(screen, 6.0, GRAPH_BACKGROUND);
        painter.circle_stroke(screen, 2.0, Stroke::new(2.0, colour));
        if drag.is_some_and(|drag| drag.index == index && drag.target == DragTarget::Point) {
            painter.circle_stroke(screen, 10.0, Stroke::new(1.0, ACCENT));
        }
    }
}

fn draw_input_histogram(painter: &egui::Painter, plot: Rect, histogram: &Histogram) {
    let max = histogram.max();
    let height = plot.height() * 0.25;
    for (index, (left, right)) in histogram_bin_ranges(histogram.bins.len())
        .into_iter()
        .enumerate()
    {
        let x0 = egui::lerp(plot.left()..=plot.right(), left);
        let x1 = egui::lerp(plot.left()..=plot.right(), right);
        let bar_height = height * histogram.bins[index] / max;
        painter.rect_filled(
            Rect::from_min_max(
                Pos2::new(x0, plot.bottom() - bar_height),
                Pos2::new(x1 + 1.0, plot.bottom()),
            ),
            CornerRadius::ZERO,
            Color32::from_rgba_unmultiplied(228, 60, 192, 100),
        );
    }
    painter.text(
        Pos2::new(plot.right() - 5.0, plot.bottom() - height - 3.0),
        egui::Align2::RIGHT_BOTTOM,
        "Input • Rec. 709",
        egui::FontId::proportional(9.0),
        Color32::from_rgb(238, 111, 210),
    );
}

fn draw_output_histogram(painter: &egui::Painter, plot: Rect, histogram: &Histogram) {
    let max = histogram.max();
    let width = plot.width() * 0.25;
    for (index, (bottom, top)) in histogram_bin_ranges(histogram.bins.len())
        .into_iter()
        .enumerate()
    {
        let y0 = egui::lerp(plot.bottom()..=plot.top(), bottom);
        let y1 = egui::lerp(plot.bottom()..=plot.top(), top);
        let bar_width = width * histogram.bins[index] / max;
        painter.rect_filled(
            Rect::from_min_max(
                Pos2::new(plot.left(), y1),
                Pos2::new(plot.left() + bar_width, y0 + 1.0),
            ),
            CornerRadius::ZERO,
            Color32::from_rgba_unmultiplied(53, 220, 221, 100),
        );
    }
    painter.text(
        Pos2::new(plot.left() + width + 3.0, plot.top() + 6.0),
        egui::Align2::LEFT_TOP,
        "Output • Rec. 709",
        egui::FontId::proportional(9.0),
        Color32::from_rgb(89, 224, 224),
    );
}

fn curve_colour(mode: CurveMode, channel: CurveChannel) -> Color32 {
    match mode {
        CurveMode::LinkedRgb | CurveMode::Luminance => CURVE_WHITE,
        CurveMode::PerChannelRgb => match channel {
            CurveChannel::Red => Color32::from_rgb(241, 119, 111),
            CurveChannel::Green => Color32::from_rgb(135, 209, 139),
            CurveChannel::Blue => Color32::from_rgb(112, 165, 238),
        },
    }
}

fn histogram_bin_ranges(bin_count: usize) -> Vec<(f32, f32)> {
    let denominator = bin_count.max(1) as f32;
    (0..bin_count)
        .map(|index| (index as f32 / denominator, (index + 1) as f32 / denominator))
        .collect()
}

fn histogram_is_current(has_rendered_preview: bool, histogram_stale: bool) -> bool {
    has_rendered_preview && !histogram_stale
}

fn input_colour_space_controls_enabled(loading_image: bool) -> bool {
    !loading_image
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::curve::{
        BezierHandleKind, ControlPoint, Curve, CurveChannel, CurveInterpolation, DerivativeCurve,
    };
    use eframe::egui::{Pos2, Rect, Vec2};

    #[test]
    fn histogram_drawing_includes_the_brightest_bin() {
        let ranges = histogram_bin_ranges(128);
        assert_eq!(ranges.len(), 128);
        assert_eq!(ranges.first().copied(), Some((0.0, 1.0 / 128.0)));
        assert_eq!(ranges.last().copied(), Some((127.0 / 128.0, 1.0)));
        assert!(histogram_bin_ranges(0).is_empty());
    }

    #[test]
    fn graph_helpers_cover_modes_ranges_and_hit_test_boundaries() {
        let plot = Rect::from_min_size(Pos2::ZERO, Vec2::splat(100.0));
        assert_eq!(GraphMode::ToneCurve.label(), "Tone curve");
        assert_eq!(GraphMode::Derivative.label(), "Derivative");
        assert_eq!(graph_screen(plot, [0.0, 0.0]), Pos2::new(0.0, 100.0));
        assert_eq!(graph_screen(plot, [1.0, 1.0]), Pos2::new(100.0, 0.0));
        assert_eq!(
            graph_screen_scaled(plot, [-1.0, 4.0], (-1.0, 2.0)),
            Pos2::new(0.0, 0.0)
        );
        let [x, y] =
            graph_value_for_mode(plot, Pos2::new(-10.0, 110.0), GraphMode::ToneCurve, None);
        assert!(x.abs() < f32::EPSILON);
        assert!(y.abs() < f32::EPSILON);
        let [x, y] = graph_value_for_mode(
            plot,
            Pos2::new(50.0, 50.0),
            GraphMode::Derivative,
            Some((-1.0, 2.0)),
        );
        assert!((x - 0.5).abs() < f32::EPSILON);
        assert!((y - 0.5).abs() < f32::EPSILON);

        let curve = Curve::identity();
        assert_eq!(
            nearest_point(&curve, plot, Pos2::new(50.0, 50.0)),
            Some((2, 0.0))
        );
        assert!(nearest_point(&curve, plot, Pos2::new(150.0, 150.0)).is_none());
        let handle = curve.handle(2, BezierHandleKind::Outgoing).unwrap();
        assert!(
            nearest_curve_handle(&curve, plot, graph_screen(plot, [handle.x, handle.y])).is_some()
        );
        assert!(
            nearest_curve_target(
                &curve,
                plot,
                graph_screen(plot, [0.5, 0.5]),
                CurveInterpolation::Linear
            )
            .is_some()
        );
        assert!(
            nearest_curve_target(
                &curve,
                plot,
                graph_screen(plot, [handle.x, handle.y]),
                CurveInterpolation::Bezier
            )
            .is_some()
        );
    }

    #[test]
    fn stale_histograms_are_not_presented_as_current() {
        assert!(!histogram_is_current(true, true));
        assert!(!histogram_is_current(false, false));
        assert!(histogram_is_current(true, false));
    }

    #[test]
    fn input_colour_space_cannot_change_during_image_loading() {
        assert!(!input_colour_space_controls_enabled(true));
        assert!(input_colour_space_controls_enabled(false));
    }

    #[test]
    fn derivative_graph_leaves_room_for_negative_slopes() {
        let curve = DerivativeCurve::from_points(vec![
            ControlPoint { x: 0.0, y: 1.0 },
            ControlPoint { x: 1.0, y: 1.0 },
        ])
        .expect("valid derivative curve");
        assert_eq!(
            derivative_range(&curve, CurveInterpolation::Linear),
            (-1.0, 2.0)
        );
        assert!(
            nearest_derivative_point(
                &curve,
                Rect::from_min_size(Pos2::ZERO, Vec2::splat(100.0)),
                Pos2::new(0.0, 50.0),
                (-1.0, 2.0)
            )
            .is_some()
        );
        assert!(
            nearest_derivative_target(
                &curve,
                Rect::from_min_size(Pos2::ZERO, Vec2::splat(100.0)),
                Pos2::new(0.0, 50.0),
                CurveInterpolation::Linear,
                (-1.0, 2.0)
            )
            .is_some()
        );
        assert_eq!(
            curve_colour(CurveMode::LinkedRgb, CurveChannel::Red),
            CURVE_WHITE
        );
        assert_ne!(
            curve_colour(CurveMode::PerChannelRgb, CurveChannel::Red),
            CURVE_WHITE
        );
        assert_ne!(
            curve_colour(CurveMode::PerChannelRgb, CurveChannel::Green),
            CURVE_WHITE
        );
        assert_ne!(
            curve_colour(CurveMode::PerChannelRgb, CurveChannel::Blue),
            CURVE_WHITE
        );
    }
}
