use eframe::egui::{
    self, Color32, CornerRadius, Pos2, Rect, Sense, Stroke, TextureHandle, Ui, Vec2,
};

use crate::vectorscope::{CIE1931_LOCUS, ScopeSpace, ring_colour};

/// Reusable CIE 1931 or RYB scope presentation widget.
pub struct ScopeWidget<'a> {
    texture: Option<&'a TextureHandle>,
    space: ScopeSpace,
}

impl<'a> ScopeWidget<'a> {
    /// Construct a widget for one scope coordinate system and optional trace texture.
    #[must_use]
    pub fn new(space: ScopeSpace, texture: Option<&'a TextureHandle>) -> Self {
        Self { texture, space }
    }

    /// Paint the scope into the supplied egui UI.
    pub fn show(self, ui: &mut Ui) {
        draw_scope(ui, self.texture, self.space);
    }
}

/// Draw a scope texture with its coordinate grid and colour ring.
#[allow(clippy::cast_precision_loss)]
pub fn draw_scope(ui: &mut Ui, texture: Option<&TextureHandle>, space: ScopeSpace) {
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
