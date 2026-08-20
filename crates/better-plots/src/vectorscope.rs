// Plot coordinates and 8-bit texture components require bounded numeric casts.
// Every float-to-integer colour cast below follows explicit [0, 1] clamping or
// construction, and image/scope dimensions are practically far below isize::MAX.
#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::excessive_precision
)]
#![allow(clippy::unreadable_literal)] // Published CIE table values remain recognisable as written.

use std::{
    fmt,
    sync::{
        OnceLock,
        atomic::{AtomicBool, Ordering},
    },
};

use eframe::egui::{Color32, ColorImage};

pub const SCOPE_RESOLUTION: usize = 512;
const MAX_SAMPLES: usize = 1_000_000;
const CIE_X_MAX: f32 = 0.8;
const CIE_Y_MAX: f32 = 0.9;
/// CIE 1931 2° spectral locus sampled every 10 nm from 380–780 nm.
pub const CIE1931_LOCUS: [[f32; 2]; 41] = [
    [0.1741123, 0.0049637],
    [0.1738008, 0.0049154],
    [0.1733369, 0.0047967],
    [0.1725766, 0.0047993],
    [0.1714074, 0.0051022],
    [0.1688775, 0.0069002],
    [0.1644118, 0.0108576],
    [0.1566409, 0.0177048],
    [0.1439604, 0.0297030],
    [0.1241185, 0.0578025],
    [0.0912935, 0.1327021],
    [0.0453907, 0.2949760],
    [0.0081680, 0.5384231],
    [0.0138702, 0.7501864],
    [0.0743024, 0.8338031],
    [0.1547221, 0.8058635],
    [0.2296197, 0.7543291],
    [0.3016039, 0.6923077],
    [0.3731015, 0.6244509],
    [0.4440625, 0.5547139],
    [0.5124864, 0.4865908],
    [0.5751513, 0.4242322],
    [0.6270366, 0.3724911],
    [0.6657636, 0.3340107],
    [0.6915040, 0.3083422],
    [0.7079178, 0.2920271],
    [0.7190329, 0.2809350],
    [0.7259923, 0.2740077],
    [0.7299690, 0.2700310],
    [0.7319933, 0.2680067],
    [0.7334170, 0.2665830],
    [0.7343902, 0.2656098],
    [0.7346873, 0.2653127],
    [0.7346783, 0.2653217],
    [0.7346680, 0.2653320],
    [0.7346939, 0.2653061],
    [0.7348243, 0.2651757],
    [0.7345133, 0.2654867],
    [0.7345133, 0.2654867],
    [0.7345133, 0.2654867],
    [0.7368421, 0.2631579],
];
const RGB_HUE_KNOTS: [f32; 7] = [
    0.0,
    1.0 / 6.0,
    2.0 / 6.0,
    3.0 / 6.0,
    4.0 / 6.0,
    5.0 / 6.0,
    1.0,
];
const RYB_HUE_KNOTS: [f32; 7] = [
    0.0,
    1.0 / 3.0,
    0.472_217,
    0.611_105,
    0.715_271,
    5.0 / 6.0,
    1.0,
];
static RGB_TO_RYB_SECOND_DERIVATIVES: OnceLock<[f32; 7]> = OnceLock::new();
static RYB_TO_RGB_SECOND_DERIVATIVES: OnceLock<[f32; 7]> = OnceLock::new();

#[derive(Clone)]
pub struct VectorscopeAnalysis {
    pub space: ScopeSpace,
    pub resolution: usize,
    pub density: Vec<f32>,
    /// Average decoded display colour for each plotted bin, in sRGB [0, 1].
    pub colours: Vec<[f32; 3]>,
    pub sampled_pixels: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TraceRenderError {
    ResolutionTooSmall { actual: usize },
    DensityLength { expected: usize, actual: usize },
    ColourLength { expected: usize, actual: usize },
    NonFinitePresentationParameter,
}

impl fmt::Display for TraceRenderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid vectorscope trace input: {self:?}")
    }
}

impl std::error::Error for TraceRenderError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScopeSpace {
    Ryb,
    Cie1931,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DensityScale {
    Logarithmic,
    Linear,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AnalysisRegion {
    Circle { centre: [f32; 2], radius: f32 },
    Rectangle { min: [f32; 2], max: [f32; 2] },
}

#[must_use]
pub fn analyse(rgba: &[u8], width: u32, height: u32, resolution: usize) -> VectorscopeAnalysis {
    analyse_region_in_space(rgba, width, height, resolution, None, ScopeSpace::Ryb)
}

#[allow(dead_code)]
#[must_use]
pub fn analyse_region(
    rgba: &[u8],
    width: u32,
    height: u32,
    resolution: usize,
    region: Option<AnalysisRegion>,
) -> VectorscopeAnalysis {
    analyse_region_in_space(rgba, width, height, resolution, region, ScopeSpace::Ryb)
}

#[must_use]
pub fn analyse_cie1931(
    rgba: &[u8],
    width: u32,
    height: u32,
    resolution: usize,
) -> VectorscopeAnalysis {
    analyse_region_in_space(rgba, width, height, resolution, None, ScopeSpace::Cie1931)
}

/// # Panics
///
/// Panics if `resolution` is smaller than two or the buffer length does not
/// match its dimensions.
#[must_use]
pub fn analyse_region_in_space(
    rgba: &[u8],
    width: u32,
    height: u32,
    resolution: usize,
    region: Option<AnalysisRegion>,
    space: ScopeSpace,
) -> VectorscopeAnalysis {
    assert!(resolution > 1, "vectorscope resolution must exceed one");
    let width = width as usize;
    let height = height as usize;
    assert_eq!(
        rgba.len(),
        width * height * 4,
        "RGBA dimensions must match data"
    );

    let mut bins = vec![0_u32; resolution * resolution];
    let mut colour_sums = vec![[0.0_f32; 3]; resolution * resolution];
    let (region_min_x, region_min_y, region_max_x, region_max_y) =
        region_bounds(region, width as f32, height as f32);
    let region_width = region_max_x.saturating_sub(region_min_x);
    let region_height = region_max_y.saturating_sub(region_min_y);
    let total_pixels = region_width.saturating_mul(region_height);
    let stride = ((total_pixels as f64 / MAX_SAMPLES as f64).sqrt().ceil() as usize).max(1);
    let mut sampled_pixels = 0_usize;

    for block_y in (region_min_y..region_max_y).step_by(stride) {
        for block_x in (region_min_x..region_max_x).step_by(stride) {
            let end_y = (block_y + stride).min(height);
            let end_x = (block_x + stride).min(width);
            let mut rgb = [0.0_f32; 3];
            let mut weight = 0.0_f32;

            for y in block_y..end_y {
                for x in block_x..end_x {
                    if !region_contains(region, x as f32 + 0.5, y as f32 + 0.5) {
                        continue;
                    }
                    let index = (y * width + x) * 4;
                    let alpha = f32::from(rgba[index + 3]) / 255.0;
                    for channel in 0..3 {
                        rgb[channel] +=
                            srgb_to_linear(f32::from(rgba[index + channel]) / 255.0) * alpha;
                    }
                    weight += alpha;
                }
            }

            if weight <= f32::EPSILON {
                continue;
            }
            for channel in &mut rgb {
                *channel /= weight;
            }

            let (x, y) = plot_coordinate(rgb, space);
            let px = (x * (resolution - 1) as f32).round() as isize;
            let py = (y * (resolution - 1) as f32).round() as isize;
            if px >= 0 && py >= 0 && px < resolution as isize && py < resolution as isize {
                let bin = py as usize * resolution + px as usize;
                bins[bin] += 1;
                for channel in 0..3 {
                    colour_sums[bin][channel] += linear_to_srgb(rgb[channel]);
                }
                sampled_pixels += 1;
            }
        }
    }

    let blurred = blur_density(&bins, resolution);
    let density = normalise_density(&blurred, sampled_pixels, resolution);
    let blurred_colours = blur_colours(&colour_sums, resolution);
    let colours = blurred_colours
        .into_iter()
        .zip(&blurred)
        .map(|(sum, count)| {
            if *count <= f32::EPSILON {
                [0.0; 3]
            } else {
                [sum[0] / *count, sum[1] / *count, sum[2] / *count]
            }
        })
        .collect();
    VectorscopeAnalysis {
        space,
        resolution,
        density,
        colours,
        sampled_pixels,
    }
}

fn plot_coordinate(rgb: [f32; 3], space: ScopeSpace) -> (f32, f32) {
    match space {
        ScopeSpace::Ryb => {
            let (hue, chroma) = rgb_hue_chroma(rgb);
            let ryb_hue = rgb_hue_to_ryb_hue(hue);
            let angle = std::f32::consts::TAU * ryb_hue;
            // Match darktable's familiar orientation: red at twelve o'clock,
            // with the ring proceeding counter-clockwise on screen.
            (
                -angle.sin() * chroma * 0.5 + 0.5,
                -angle.cos() * chroma * 0.5 + 0.5,
            )
        }
        ScopeSpace::Cie1931 => {
            let [x, y] = rgb_to_cie1931_xy(rgb);
            // The useful visible gamut fits in x=[0, 0.8], y=[0, 0.9].
            (x / CIE_X_MAX, 1.0 - y / CIE_Y_MAX)
        }
    }
}

fn region_bounds(
    region: Option<AnalysisRegion>,
    width: f32,
    height: f32,
) -> (usize, usize, usize, usize) {
    let Some(region) = region else {
        return (0, 0, width as usize, height as usize);
    };
    let (min_x, min_y, max_x, max_y) = match region {
        AnalysisRegion::Circle {
            centre: [x, y],
            radius,
        } => (x - radius, y - radius, x + radius, y + radius),
        AnalysisRegion::Rectangle {
            min: [min_x, min_y],
            max: [max_x, max_y],
        } => (min_x, min_y, max_x, max_y),
    };
    (
        min_x.floor().max(0.0) as usize,
        min_y.floor().max(0.0) as usize,
        max_x.ceil().min(width) as usize,
        max_y.ceil().min(height) as usize,
    )
}

fn region_contains(region: Option<AnalysisRegion>, x: f32, y: f32) -> bool {
    match region {
        None => true,
        Some(AnalysisRegion::Circle {
            centre: [centre_x, centre_y],
            radius,
        }) => (x - centre_x).hypot(y - centre_y) <= radius.max(0.5),
        Some(AnalysisRegion::Rectangle {
            min: [min_x, min_y],
            max: [max_x, max_y],
        }) => x >= min_x && x <= max_x && y >= min_y && y <= max_y,
    }
}

/// Renders a validated vectorscope analysis into a presentation texture.
///
/// # Errors
///
/// Returns [`TraceRenderError`] when dimensions, backing arrays, intensity,
/// or sharpness do not form a finite and structurally consistent request.
pub fn render_trace(
    analysis: &VectorscopeAnalysis,
    intensity: f32,
    sharpness: f32,
    density_scale: DensityScale,
    inverse_highlight: bool,
) -> Result<ColorImage, TraceRenderError> {
    let size = analysis.resolution;
    if size < 2 {
        return Err(TraceRenderError::ResolutionTooSmall { actual: size });
    }
    let expected = size
        .checked_mul(size)
        .ok_or(TraceRenderError::ResolutionTooSmall { actual: size })?;
    if analysis.density.len() != expected {
        return Err(TraceRenderError::DensityLength {
            expected,
            actual: analysis.density.len(),
        });
    }
    if analysis.colours.len() != expected {
        return Err(TraceRenderError::ColourLength {
            expected,
            actual: analysis.colours.len(),
        });
    }
    if !intensity.is_finite() || !sharpness.is_finite() {
        return Err(TraceRenderError::NonFinitePresentationParameter);
    }
    let centre = (size - 1) as f32 * 0.5;
    let radius = centre.max(1.0);
    let mut pixels = Vec::with_capacity(size * size);

    for y in 0..size {
        for x in 0..size {
            let dx = (x as f32 - centre) / radius;
            let dy = (y as f32 - centre) / radius;
            let radial = dx.hypot(dy);
            let Some(source_coordinate) = source_coordinate(
                [x as f32 / (size - 1) as f32, y as f32 / (size - 1) as f32],
                analysis.space,
                density_scale,
            ) else {
                pixels.push(Color32::TRANSPARENT);
                continue;
            };
            if analysis.space == ScopeSpace::Ryb && radial > 1.0 {
                pixels.push(Color32::TRANSPARENT);
                continue;
            }
            let source_x = source_coordinate[0] * (size - 1) as f32;
            let source_y = source_coordinate[1] * (size - 1) as f32;
            if !(0.0..=(size - 1) as f32).contains(&source_x)
                || !(0.0..=(size - 1) as f32).contains(&source_y)
            {
                pixels.push(Color32::TRANSPARENT);
                continue;
            }
            let density = bilinear_density(&analysis.density, size, source_x, source_y);
            if density <= 0.000_1 {
                pixels.push(Color32::TRANSPARENT);
                continue;
            }

            let base = if analysis.space == ScopeSpace::Cie1931 {
                bilinear_colour(&analysis.colours, size, source_x, source_y)
            } else {
                let angle = (-dx).atan2(-dy).rem_euclid(std::f32::consts::TAU);
                let ryb_hue = angle / std::f32::consts::TAU;
                let display_hue = ryb_hue_to_rgb_hue(ryb_hue);
                hsv_to_rgb(display_hue, 0.82, 1.0)
            };
            let alpha = (density * intensity)
                .clamp(0.0, 1.0)
                .powf(sharpness.clamp(0.1, 8.0));
            let white_mix = alpha.powf(1.6) * 0.52;
            let mut colour = [
                base[0] + (1.0 - base[0]) * white_mix,
                base[1] + (1.0 - base[1]) * white_mix,
                base[2] + (1.0 - base[2]) * white_mix,
            ];
            if inverse_highlight {
                for channel in &mut colour {
                    *channel = 1.0 - *channel;
                }
            }
            pixels.push(Color32::from_rgba_unmultiplied(
                (colour[0] * 255.0) as u8,
                (colour[1] * 255.0) as u8,
                (colour[2] * 255.0) as u8,
                (alpha * 230.0) as u8,
            ));
        }
    }

    Ok(ColorImage::new([size, size], pixels))
}

#[must_use]
pub fn source_coordinate(
    output: [f32; 2],
    space: ScopeSpace,
    scale: DensityScale,
) -> Option<[f32; 2]> {
    if scale == DensityScale::Linear {
        return if output.iter().all(|value| (0.0..=1.0).contains(value)) {
            Some(output)
        } else {
            None
        };
    }
    let centre = match space {
        ScopeSpace::Ryb => [0.5, 0.5],
        ScopeSpace::Cie1931 => [0.312_7 / CIE_X_MAX, 1.0 - 0.329_0 / CIE_Y_MAX],
    };
    let delta = [output[0] - centre[0], output[1] - centre[1]];
    let radius = delta[0].hypot(delta[1]);
    if radius <= f32::EPSILON {
        return Some(centre);
    }
    let max_radius = match space {
        ScopeSpace::Ryb => 0.5,
        ScopeSpace::Cie1931 => 0.65,
    };
    let linear_radius = ((radius / max_radius * 30.0_f32.ln()).exp() - 1.0) / 29.0 * max_radius;
    let source = [
        centre[0] + delta[0] / radius * linear_radius,
        centre[1] + delta[1] / radius * linear_radius,
    ];
    if source.iter().all(|value| (0.0..=1.0).contains(value)) {
        Some(source)
    } else {
        None
    }
}

/// Convert a decoded linear RGB sample into the normalized scope coordinate.
/// The returned coordinate is in the same [0, 1] square used by scope textures.
#[must_use]
pub fn scope_coordinate(rgb: [f32; 3], space: ScopeSpace) -> [f32; 2] {
    let (x, y) = plot_coordinate(rgb, space);
    [x, y]
}

/// Apply the selected radial display transform to a linear scope coordinate.
/// This is the forward counterpart of [`source_coordinate`].
#[must_use]
pub fn display_coordinate(
    source: [f32; 2],
    space: ScopeSpace,
    scale: DensityScale,
) -> Option<[f32; 2]> {
    if scale == DensityScale::Linear {
        return if source.iter().all(|value| (0.0..=1.0).contains(value)) {
            Some(source)
        } else {
            None
        };
    }
    let centre = match space {
        ScopeSpace::Ryb => [0.5, 0.5],
        ScopeSpace::Cie1931 => [0.312_7 / CIE_X_MAX, 1.0 - 0.329_0 / CIE_Y_MAX],
    };
    let delta = [source[0] - centre[0], source[1] - centre[1]];
    let radius = delta[0].hypot(delta[1]);
    if radius <= f32::EPSILON {
        return Some(centre);
    }
    let max_radius = match space {
        ScopeSpace::Ryb => 0.5,
        ScopeSpace::Cie1931 => 0.65,
    };
    let mapped_radius = ((radius / max_radius * 29.0 + 1.0).ln() / 30.0_f32.ln()) * max_radius;
    let output = [
        centre[0] + delta[0] / radius * mapped_radius,
        centre[1] + delta[1] / radius * mapped_radius,
    ];
    if output.iter().all(|value| (0.0..=1.0).contains(value)) {
        Some(output)
    } else {
        None
    }
}

/// Render a transparent image layer for scope-to-image highlighting. Pixels
/// whose scope coordinates fall within `radius` of `centre` are painted with
/// their inverse sRGB colour, making the selected colour family visible over
/// the original photo.
/// # Panics
///
/// Panics when the buffer length does not match its dimensions.
#[must_use]
pub fn render_reverse_highlight(
    rgba: &[u8],
    width: u32,
    height: u32,
    centre: [f32; 2],
    radius: f32,
    space: ScopeSpace,
    density_scale: DensityScale,
) -> Vec<u8> {
    render_reverse_highlight_with_cancellation(
        rgba,
        width,
        height,
        centre,
        radius,
        space,
        density_scale,
        &AtomicBool::new(false),
    )
    .expect("reverse-highlight render was not cancelled")
}

/// Renders a reverse-selection overlay while observing cooperative cancellation.
///
/// Returns `None` when `cancellation` is set before or during the scan.
///
/// # Panics
///
/// Panics when the buffer length does not match its dimensions or when the
/// radius is negative.
#[allow(clippy::too_many_arguments)]
#[must_use]
pub fn render_reverse_highlight_with_cancellation(
    rgba: &[u8],
    width: u32,
    height: u32,
    centre: [f32; 2],
    radius: f32,
    space: ScopeSpace,
    density_scale: DensityScale,
    cancellation: &AtomicBool,
) -> Option<Vec<u8>> {
    let width = width as usize;
    let height = height as usize;
    assert_eq!(rgba.len(), width * height * 4);
    assert!(
        radius >= 0.0,
        "reverse-highlight radius must be non-negative"
    );
    let radius = radius.max(0.000_1);
    let mut output = vec![0_u8; rgba.len()];
    let mut processed = 0_usize;
    for y in 0..height {
        for x in 0..width {
            if processed.is_multiple_of(4_096) && cancellation.load(Ordering::Relaxed) {
                return None;
            }
            processed += 1;
            let index = (y * width + x) * 4;
            let alpha = f32::from(rgba[index + 3]) / 255.0;
            if alpha <= f32::EPSILON {
                continue;
            }
            let rgb = [
                srgb_to_linear(f32::from(rgba[index]) / 255.0),
                srgb_to_linear(f32::from(rgba[index + 1]) / 255.0),
                srgb_to_linear(f32::from(rgba[index + 2]) / 255.0),
            ];
            let Some(point) =
                display_coordinate(scope_coordinate(rgb, space), space, density_scale)
            else {
                continue;
            };
            let distance = (point[0] - centre[0]).hypot(point[1] - centre[1]);
            if distance > radius {
                continue;
            }
            // A soft edge prevents the overlay from flickering as the pointer
            // crosses individual scope bins while retaining a strong centre.
            let edge = (1.0 - distance / radius).powf(0.35);
            output[index] = 255 - rgba[index];
            output[index + 1] = 255 - rgba[index + 1];
            output[index + 2] = 255 - rgba[index + 2];
            output[index + 3] = (edge * 220.0).round() as u8;
        }
    }
    Some(output)
}

fn bilinear_density(values: &[f32], size: usize, x: f32, y: f32) -> f32 {
    let x0 = x.floor() as usize;
    let y0 = y.floor() as usize;
    let x1 = (x0 + 1).min(size - 1);
    let y1 = (y0 + 1).min(size - 1);
    let tx = x - x0 as f32;
    let ty = y - y0 as f32;
    let top = values[y0 * size + x0] * (1.0 - tx) + values[y0 * size + x1] * tx;
    let bottom = values[y1 * size + x0] * (1.0 - tx) + values[y1 * size + x1] * tx;
    top * (1.0 - ty) + bottom * ty
}

fn bilinear_colour(values: &[[f32; 3]], size: usize, x: f32, y: f32) -> [f32; 3] {
    let x0 = x.floor() as usize;
    let y0 = y.floor() as usize;
    let x1 = (x0 + 1).min(size - 1);
    let y1 = (y0 + 1).min(size - 1);
    let tx = x - x0 as f32;
    let ty = y - y0 as f32;
    let mut result = [0.0; 3];
    for channel in 0..3 {
        let top =
            values[y0 * size + x0][channel] * (1.0 - tx) + values[y0 * size + x1][channel] * tx;
        let bottom =
            values[y1 * size + x0][channel] * (1.0 - tx) + values[y1 * size + x1][channel] * tx;
        result[channel] = top * (1.0 - ty) + bottom * ty;
    }
    result
}

#[must_use]
pub fn ring_colour(turns: f32) -> Color32 {
    let hue = ryb_hue_to_rgb_hue(turns.rem_euclid(1.0));
    let rgb = hsv_to_rgb(hue, 0.72, 1.0);
    Color32::from_rgb(
        (rgb[0] * 255.0) as u8,
        (rgb[1] * 255.0) as u8,
        (rgb[2] * 255.0) as u8,
    )
}

fn srgb_to_linear(value: f32) -> f32 {
    if value <= 0.040_45 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

fn linear_to_srgb(value: f32) -> f32 {
    let value = value.max(0.0);
    if value <= 0.003_130_8 {
        value * 12.92
    } else {
        1.055 * value.powf(1.0 / 2.4) - 0.055
    }
}

fn rgb_to_cie1931_xy(rgb: [f32; 3]) -> [f32; 2] {
    // IEC 61966-2-1 sRGB/D65 to CIE XYZ, with the input already linearised.
    let x = 0.412_456_4 * rgb[0] + 0.357_576_1 * rgb[1] + 0.180_437_5 * rgb[2];
    let y = 0.212_672_9 * rgb[0] + 0.715_152_2 * rgb[1] + 0.072_175_0 * rgb[2];
    let z = 0.019_333_9 * rgb[0] + 0.119_192_0 * rgb[1] + 0.950_304_1 * rgb[2];
    let sum = x + y + z;
    if sum <= f32::EPSILON {
        [0.312_7, 0.329_0]
    } else {
        [x / sum, y / sum]
    }
}

fn rgb_hue_chroma(rgb: [f32; 3]) -> (f32, f32) {
    let max = rgb[0].max(rgb[1]).max(rgb[2]);
    let min = rgb[0].min(rgb[1]).min(rgb[2]);
    let chroma = max - min;
    if chroma <= f32::EPSILON {
        return (0.0, 0.0);
    }
    let hue_sector = if rgb[0] >= rgb[1] && rgb[0] >= rgb[2] {
        ((rgb[1] - rgb[2]) / chroma).rem_euclid(6.0)
    } else if rgb[1] >= rgb[2] {
        (rgb[2] - rgb[0]) / chroma + 2.0
    } else {
        (rgb[0] - rgb[1]) / chroma + 4.0
    };
    (hue_sector / 6.0, chroma.clamp(0.0, 1.0))
}

fn rgb_hue_to_ryb_hue(hue: f32) -> f32 {
    let second_derivatives = RGB_TO_RYB_SECOND_DERIVATIVES
        .get_or_init(|| natural_spline_second_derivatives(&RGB_HUE_KNOTS, &RYB_HUE_KNOTS));
    cubic_spline(
        hue.rem_euclid(1.0),
        &RGB_HUE_KNOTS,
        &RYB_HUE_KNOTS,
        second_derivatives,
    )
}

fn ryb_hue_to_rgb_hue(hue: f32) -> f32 {
    let second_derivatives = RYB_TO_RGB_SECOND_DERIVATIVES
        .get_or_init(|| natural_spline_second_derivatives(&RYB_HUE_KNOTS, &RGB_HUE_KNOTS));
    cubic_spline(
        hue.rem_euclid(1.0),
        &RYB_HUE_KNOTS,
        &RGB_HUE_KNOTS,
        second_derivatives,
    )
}

fn cubic_spline(value: f32, x: &[f32; 7], y: &[f32; 7], second_derivatives: &[f32; 7]) -> f32 {
    let segment = (0..6).find(|&index| value < x[index + 1]).unwrap_or(5);
    let distance = value - x[segment];
    let width = x[segment + 1] - x[segment];
    y[segment]
        + distance
            * ((y[segment + 1] - y[segment]) / width
                - (second_derivatives[segment + 1] / 6.0 + second_derivatives[segment] / 3.0)
                    * width
                + distance
                    * (0.5 * second_derivatives[segment]
                        + distance
                            * (second_derivatives[segment + 1] - second_derivatives[segment])
                            / (6.0 * width)))
}

fn natural_spline_second_derivatives(x: &[f32; 7], y: &[f32; 7]) -> [f32; 7] {
    let mut lower = [0.0_f32; 7];
    let mut diagonal = [0.0_f32; 7];
    let mut upper = [0.0_f32; 7];
    let mut right_hand_side = [0.0_f32; 7];

    diagonal[0] = 1.0;
    diagonal[6] = 1.0;
    for index in 1..6 {
        let left_width = x[index] - x[index - 1];
        let right_width = x[index + 1] - x[index];
        lower[index] = left_width / 6.0;
        diagonal[index] = (left_width + right_width) / 3.0;
        upper[index] = right_width / 6.0;
        right_hand_side[index] =
            (y[index + 1] - y[index]) / right_width - (y[index] - y[index - 1]) / left_width;
    }

    for index in 1..7 {
        let factor = lower[index] / diagonal[index - 1];
        diagonal[index] -= factor * upper[index - 1];
        right_hand_side[index] -= factor * right_hand_side[index - 1];
    }

    let mut second_derivatives = [0.0_f32; 7];
    second_derivatives[6] = right_hand_side[6] / diagonal[6];
    for index in (0..6).rev() {
        second_derivatives[index] = (right_hand_side[index]
            - upper[index] * second_derivatives[index + 1])
            / diagonal[index];
    }
    second_derivatives
}

fn hsv_to_rgb(hue: f32, saturation: f32, value: f32) -> [f32; 3] {
    let sector = hue.rem_euclid(1.0) * 6.0;
    let index = sector.floor() as i32;
    let fraction = sector - index as f32;
    let p = value * (1.0 - saturation);
    let q = value * (1.0 - saturation * fraction);
    let t = value * (1.0 - saturation * (1.0 - fraction));
    match index {
        0 => [value, t, p],
        1 => [q, value, p],
        2 => [p, value, t],
        3 => [p, q, value],
        4 => [t, p, value],
        _ => [value, p, q],
    }
}

fn blur_density(bins: &[u32], size: usize) -> Vec<f32> {
    let bins = bins.iter().map(|value| *value as f32).collect::<Vec<_>>();
    blur_float(&bins, size)
}

fn blur_colours(colours: &[[f32; 3]], size: usize) -> Vec<[f32; 3]> {
    let channels = (0..3)
        .map(|channel| {
            let plane = colours
                .iter()
                .map(|colour| colour[channel])
                .collect::<Vec<_>>();
            blur_float(&plane, size)
        })
        .collect::<Vec<_>>();
    (0..colours.len())
        .map(|index| [channels[0][index], channels[1][index], channels[2][index]])
        .collect()
}

fn blur_float(values: &[f32], size: usize) -> Vec<f32> {
    let kernel = [1.0_f32, 2.0, 1.0];
    let mut horizontal = vec![0.0_f32; values.len()];
    let mut output = vec![0.0_f32; values.len()];
    for y in 0..size {
        for x in 0..size {
            let mut sum = 0.0;
            let mut weight = 0.0;
            for (offset, kernel_weight) in kernel.iter().enumerate() {
                let sample_x = x as isize + offset as isize - 1;
                if sample_x >= 0 && sample_x < size as isize {
                    sum += values[y * size + sample_x as usize] * kernel_weight;
                    weight += kernel_weight;
                }
            }
            horizontal[y * size + x] = sum / weight;
        }
    }
    for y in 0..size {
        for x in 0..size {
            let mut sum = 0.0;
            let mut weight = 0.0;
            for (offset, kernel_weight) in kernel.iter().enumerate() {
                let sample_y = y as isize + offset as isize - 1;
                if sample_y >= 0 && sample_y < size as isize {
                    sum += horizontal[sample_y as usize * size + x] * kernel_weight;
                    weight += kernel_weight;
                }
            }
            output[y * size + x] = sum / weight;
        }
    }
    output
}

fn normalise_density(bins: &[f32], sampled_pixels: usize, size: usize) -> Vec<f32> {
    if sampled_pixels == 0 {
        return vec![0.0; bins.len()];
    }
    let area_compensation = (size * size) as f32 / sampled_pixels as f32;
    bins.iter()
        .map(|count| 1.0 - (-count * area_compensation / 12.0).exp())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn neutral_pixels_land_at_centre() {
        let image = [128, 128, 128, 255].repeat(16);
        let scope = analyse(&image, 4, 4, 33);
        let centre = 16 * 33 + 16;
        assert!(scope.density[centre] > 0.0);
    }

    #[test]
    fn transparent_pixels_do_not_contribute() {
        let image = [255, 0, 0, 0].repeat(16);
        let scope = analyse(&image, 4, 4, 33);
        assert_eq!(scope.sampled_pixels, 0);
        assert!(scope.density.iter().all(|value| *value == 0.0));
    }

    #[test]
    fn ryb_mapping_round_trips_knots() {
        for hue in RGB_HUE_KNOTS {
            let round_trip = ryb_hue_to_rgb_hue(rgb_hue_to_ryb_hue(hue.rem_euclid(1.0)));
            let expected = hue.rem_euclid(1.0);
            assert!((round_trip - expected).abs() < 1.0e-5);
        }
    }

    #[test]
    fn saturated_colour_lands_farther_out_than_neutral() {
        let (_, neutral_chroma) = rgb_hue_chroma([0.4, 0.4, 0.4]);
        let (_, red_chroma) = rgb_hue_chroma([1.0, 0.0, 0.0]);
        assert!(neutral_chroma.abs() < f32::EPSILON);
        assert!((red_chroma - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn srgb_white_maps_to_the_cie_d65_whitepoint() {
        let [x, y] = rgb_to_cie1931_xy([1.0, 1.0, 1.0]);
        assert!((x - 0.312_7).abs() < 0.001);
        assert!((y - 0.329_0).abs() < 0.001);
    }

    #[test]
    fn logarithmic_radius_expands_the_source_radius() {
        let linear = source_coordinate([0.75, 0.5], ScopeSpace::Ryb, DensityScale::Linear)
            .expect("linear coordinate should be in scope");
        let logarithmic =
            source_coordinate([0.75, 0.5], ScopeSpace::Ryb, DensityScale::Logarithmic)
                .expect("logarithmic coordinate should be in scope");
        assert!((linear[0] - 0.75).abs() < f32::EPSILON);
        assert!(logarithmic[0] < linear[0]);
    }

    #[test]
    fn radial_display_mapping_round_trips() {
        let source = [0.73, 0.42];
        let display = display_coordinate(source, ScopeSpace::Ryb, DensityScale::Logarithmic)
            .expect("source should be visible");
        let round_trip = source_coordinate(display, ScopeSpace::Ryb, DensityScale::Logarithmic)
            .expect("display coordinate should be visible");
        assert!((round_trip[0] - source[0]).abs() < 1.0e-5);
        assert!((round_trip[1] - source[1]).abs() < 1.0e-5);
    }

    #[test]
    fn linear_source_coordinates_reject_values_outside_display_domain() {
        assert_eq!(
            source_coordinate([-0.01, 0.5], ScopeSpace::Ryb, DensityScale::Linear),
            None
        );
        assert_eq!(
            source_coordinate([0.5, 1.01], ScopeSpace::Cie1931, DensityScale::Linear),
            None
        );
    }

    #[test]
    fn reverse_highlight_selects_only_matching_scope_colour() {
        let image = [
            255, 0, 0, 255, // red
            0, 0, 255, 255, // blue
        ];
        let centre = scope_coordinate([1.0, 0.0, 0.0], ScopeSpace::Ryb);
        let highlighted = render_reverse_highlight(
            &image,
            2,
            1,
            centre,
            0.01,
            ScopeSpace::Ryb,
            DensityScale::Linear,
        );
        assert!(highlighted[3] > 0, "red pixel should be selected");
        assert_eq!(highlighted[7], 0, "blue pixel should not be selected");
    }

    #[test]
    #[should_panic(expected = "reverse-highlight radius must be non-negative")]
    fn reverse_highlight_rejects_negative_radius() {
        let _ = render_reverse_highlight(
            &[255, 0, 0, 255],
            1,
            1,
            [0.5, 0.5],
            -1.0,
            ScopeSpace::Ryb,
            DensityScale::Linear,
        );
    }

    #[test]
    fn cancelled_reverse_highlight_does_not_scan_the_image() {
        let cancellation = AtomicBool::new(true);
        assert_eq!(
            render_reverse_highlight_with_cancellation(
                &[255, 0, 0, 255],
                1,
                1,
                [0.5, 0.5],
                0.01,
                ScopeSpace::Ryb,
                DensityScale::Linear,
                &cancellation,
            ),
            None
        );
    }

    #[test]
    fn cie_analysis_records_the_selected_space_and_colour_bins() {
        let image = [255, 0, 0, 255].repeat(16);
        let scope = analyse_cie1931(&image, 4, 4, 33);
        assert_eq!(scope.space, ScopeSpace::Cie1931);
        assert_eq!(scope.sampled_pixels, 16);
        assert!(scope.colours.iter().any(|colour| colour[0] > 0.8));
    }

    #[test]
    fn a_single_pixel_region_only_uses_that_pixel() {
        let mut image = [128, 128, 128, 255].repeat(9);
        image[4 * 4..4 * 4 + 4].copy_from_slice(&[255, 0, 0, 255]);
        let scope = analyse_region(
            &image,
            3,
            3,
            33,
            Some(AnalysisRegion::Circle {
                centre: [1.5, 1.5],
                radius: 0.5,
            }),
        );
        assert_eq!(scope.sampled_pixels, 1);
        assert!(scope.density.iter().any(|value| *value > 0.0));
    }

    #[test]
    fn rectangle_bounds_exclude_pixels_outside_the_region() {
        let image = [255, 0, 0, 255].repeat(16);
        let scope = analyse_region(
            &image,
            4,
            4,
            33,
            Some(AnalysisRegion::Rectangle {
                min: [0.0, 0.0],
                max: [2.0, 2.0],
            }),
        );
        assert_eq!(scope.sampled_pixels, 4);
    }

    #[test]
    fn semi_transparent_pixels_use_the_same_colour_in_analysis_and_reverse_highlighting() {
        let image = [255, 0, 0, 128];
        let analysis = analyse(&image, 1, 1, 33);
        assert_eq!(analysis.sampled_pixels, 1);

        let analysed_red = scope_coordinate([1.0, 0.0, 0.0], ScopeSpace::Ryb);
        let highlighted = render_reverse_highlight(
            &image,
            1,
            1,
            analysed_red,
            0.01,
            ScopeSpace::Ryb,
            DensityScale::Linear,
        );

        assert!(
            highlighted[3] > 0,
            "a scope point produced by analysis must select the same semi-transparent source pixel in reverse"
        );
    }

    #[test]
    fn ryb_hue_mapping_has_a_continuous_slope_at_internal_knots() {
        let knot = RGB_HUE_KNOTS[1];
        let epsilon = 0.000_1;
        let at_knot = rgb_hue_to_ryb_hue(knot);
        let left_slope = (at_knot - rgb_hue_to_ryb_hue(knot - epsilon)) / epsilon;
        let right_slope = (rgb_hue_to_ryb_hue(knot + epsilon) - at_knot) / epsilon;

        assert!(
            (left_slope - right_slope).abs() < 0.05,
            "darktable's vectorscope uses a cubic spline, whose first derivative is continuous at this knot; left={left_slope}, right={right_slope}"
        );
    }

    #[test]
    fn trace_rendering_rejects_invalid_public_analysis_shapes() {
        let zero = VectorscopeAnalysis {
            space: ScopeSpace::Ryb,
            resolution: 0,
            density: Vec::new(),
            colours: Vec::new(),
            sampled_pixels: 0,
        };
        assert!(matches!(
            render_trace(&zero, 1.0, 1.0, DensityScale::Linear, false),
            Err(TraceRenderError::ResolutionTooSmall { actual: 0 })
        ));

        let malformed = VectorscopeAnalysis {
            space: ScopeSpace::Ryb,
            resolution: 2,
            density: vec![0.0; 3],
            colours: vec![[0.0; 3]; 4],
            sampled_pixels: 0,
        };
        assert!(matches!(
            render_trace(&malformed, 1.0, 1.0, DensityScale::Linear, false),
            Err(TraceRenderError::DensityLength {
                expected: 4,
                actual: 3
            })
        ));
    }

    #[test]
    fn trace_rendering_rejects_non_finite_presentation_parameters() {
        let analysis = VectorscopeAnalysis {
            space: ScopeSpace::Ryb,
            resolution: 2,
            density: vec![0.0; 4],
            colours: vec![[0.0; 3]; 4],
            sampled_pixels: 0,
        };
        assert_eq!(
            render_trace(&analysis, f32::NAN, 1.0, DensityScale::Linear, false),
            Err(TraceRenderError::NonFinitePresentationParameter)
        );
        assert_eq!(
            render_trace(&analysis, 1.0, f32::INFINITY, DensityScale::Linear, false,),
            Err(TraceRenderError::NonFinitePresentationParameter)
        );
    }

    #[test]
    fn a_wide_single_row_reverse_scan_observes_cancellation() {
        let width = 2_000_000_u32;
        let rgba = vec![255_u8; width as usize * 4];
        let cancellation = std::sync::Arc::new(AtomicBool::new(false));
        let worker_cancellation = cancellation.clone();
        let worker = std::thread::spawn(move || {
            render_reverse_highlight_with_cancellation(
                &rgba,
                width,
                1,
                [0.5, 0.5],
                0.1,
                ScopeSpace::Ryb,
                DensityScale::Linear,
                &worker_cancellation,
            )
        });
        std::thread::sleep(std::time::Duration::from_millis(1));
        cancellation.store(true, Ordering::Relaxed);
        assert_eq!(
            worker.join().expect("reverse scan worker does not panic"),
            None
        );
    }
}
