//! GUI-independent vectorscope analysis.
//!
//! This module owns the numeric scope contract used by the experimental
//! `FocalPlot` harness. Texture construction, colour meshes, and egui drawing
//! remain outside `FocalCore`.

// Scope coordinates intentionally cross between bounded image integers and
// normalised floating-point display coordinates. The source implementation
// uses the same explicit conversions and bounds checks.
#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::excessive_precision
)]

use std::{fmt, sync::OnceLock};

use crate::CancellationToken;

pub const SCOPE_RESOLUTION: usize = 512;
const MAX_SAMPLES: usize = 1_000_000;
const CIE_X_MAX: f32 = 0.8;
const CIE_Y_MAX: f32 = 0.9;
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScopeInputContract {
    EncodedSrgb8,
}

#[derive(Clone, Debug, PartialEq)]
pub struct VectorscopeAnalysis {
    pub space: ScopeSpace,
    pub resolution: usize,
    pub density: Vec<f32>,
    /// Average decoded display colour for each plotted bin, in sRGB [0, 1].
    pub colours: Vec<[f32; 3]>,
    pub sampled_pixels: usize,
}

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

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScopeError {
    ResolutionTooSmall { actual: usize },
    PixelBufferLength { expected: usize, actual: usize },
    DimensionOverflow,
    NonFiniteRegion,
    NegativeCircleRadius,
    ReversedRectangle,
    ZeroDimension,
    Cancelled,
}

impl fmt::Display for ScopeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ResolutionTooSmall { actual } => {
                write!(
                    formatter,
                    "vectorscope resolution must exceed one, got {actual}"
                )
            }
            Self::PixelBufferLength { expected, actual } => write!(
                formatter,
                "RGBA dimensions require {expected} bytes, received {actual}"
            ),
            Self::DimensionOverflow => {
                write!(formatter, "RGBA dimensions overflow addressable memory")
            }
            Self::NonFiniteRegion => write!(formatter, "scope region contains a non-finite value"),
            Self::NegativeCircleRadius => {
                write!(formatter, "scope circle radius must not be negative")
            }
            Self::ReversedRectangle => write!(formatter, "scope rectangle bounds must be ordered"),
            Self::ZeroDimension => write!(formatter, "scope image dimensions must be non-zero"),
            Self::Cancelled => write!(formatter, "scope analysis was cancelled"),
        }
    }
}

impl std::error::Error for ScopeError {}

#[must_use]
pub fn analyse(
    rgba: &[u8],
    width: u32,
    height: u32,
    resolution: usize,
    contract: ScopeInputContract,
) -> VectorscopeAnalysis {
    analyse_region_in_space(
        rgba,
        width,
        height,
        resolution,
        None,
        ScopeSpace::Ryb,
        contract,
    )
}

#[must_use]
pub fn analyse_region(
    rgba: &[u8],
    width: u32,
    height: u32,
    resolution: usize,
    region: Option<AnalysisRegion>,
    contract: ScopeInputContract,
) -> VectorscopeAnalysis {
    analyse_region_in_space(
        rgba,
        width,
        height,
        resolution,
        region,
        ScopeSpace::Ryb,
        contract,
    )
}

#[must_use]
pub fn analyse_cie1931(
    rgba: &[u8],
    width: u32,
    height: u32,
    resolution: usize,
    contract: ScopeInputContract,
) -> VectorscopeAnalysis {
    analyse_region_in_space(
        rgba,
        width,
        height,
        resolution,
        None,
        ScopeSpace::Cie1931,
        contract,
    )
}

/// Analyses an RGBA image, panicking only through the compatibility wrapper
/// when its dimensions or region are invalid. New callers should prefer
/// [`try_analyse_region_in_space`] when input is not already trusted.
///
/// # Panics
///
/// Panics when the resolution, dimensions, pixel buffer, or selected region
/// is invalid. Use [`try_analyse_region_in_space`] for untrusted inputs.
#[must_use]
pub fn analyse_region_in_space(
    rgba: &[u8],
    width: u32,
    height: u32,
    resolution: usize,
    region: Option<AnalysisRegion>,
    space: ScopeSpace,
    contract: ScopeInputContract,
) -> VectorscopeAnalysis {
    try_analyse_region_in_space(
        rgba,
        width,
        height,
        resolution,
        region,
        space,
        contract,
        &CancellationToken::new(),
    )
    .expect("valid vectorscope analysis inputs")
}

/// Fallible vectorscope analysis for untrusted image and region boundaries.
///
/// # Errors
///
/// Returns [`ScopeError`] when the resolution, dimensions, pixel buffer, or
/// selected region is invalid.
#[allow(clippy::too_many_arguments)]
pub fn try_analyse_region_in_space(
    rgba: &[u8],
    width: u32,
    height: u32,
    resolution: usize,
    region: Option<AnalysisRegion>,
    space: ScopeSpace,
    _contract: ScopeInputContract,
    cancellation: &CancellationToken,
) -> Result<VectorscopeAnalysis, ScopeError> {
    validate_inputs(rgba, width, height, resolution, region)?;
    let width = usize::try_from(width).map_err(|_| ScopeError::DimensionOverflow)?;
    let height = usize::try_from(height).map_err(|_| ScopeError::DimensionOverflow)?;
    let bin_count = resolution
        .checked_mul(resolution)
        .ok_or(ScopeError::DimensionOverflow)?;

    let mut bins = vec![0_u32; bin_count];
    let mut colour_sums = vec![[0.0_f32; 3]; bin_count];
    let (region_min_x, region_min_y, region_max_x, region_max_y) =
        region_bounds(region, width as f32, height as f32);
    let region_width = region_max_x.saturating_sub(region_min_x);
    let region_height = region_max_y.saturating_sub(region_min_y);
    let total_pixels = region_width.saturating_mul(region_height);
    let stride = ((total_pixels as f64 / MAX_SAMPLES as f64).sqrt().ceil() as usize).max(1);
    let mut sampled_pixels = 0_usize;

    for block_y in (region_min_y..region_max_y).step_by(stride) {
        for block_x in (region_min_x..region_max_x).step_by(stride) {
            if cancellation.is_cancelled() {
                return Err(ScopeError::Cancelled);
            }
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
    Ok(VectorscopeAnalysis {
        space,
        resolution,
        density,
        colours,
        sampled_pixels,
    })
}

fn validate_inputs(
    rgba: &[u8],
    width: u32,
    height: u32,
    resolution: usize,
    region: Option<AnalysisRegion>,
) -> Result<(), ScopeError> {
    if resolution <= 1 {
        return Err(ScopeError::ResolutionTooSmall { actual: resolution });
    }
    if width == 0 || height == 0 {
        return Err(ScopeError::ZeroDimension);
    }
    let expected = usize::try_from(width)
        .ok()
        .and_then(|width| {
            usize::try_from(height)
                .ok()
                .and_then(|height| width.checked_mul(height))
        })
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or(ScopeError::DimensionOverflow)?;
    if rgba.len() != expected {
        return Err(ScopeError::PixelBufferLength {
            expected,
            actual: rgba.len(),
        });
    }
    if let Some(region) = region {
        match region {
            AnalysisRegion::Circle {
                centre: [x, y],
                radius,
            } => {
                if !x.is_finite() || !y.is_finite() || !radius.is_finite() {
                    return Err(ScopeError::NonFiniteRegion);
                }
                if radius < 0.0 {
                    return Err(ScopeError::NegativeCircleRadius);
                }
            }
            AnalysisRegion::Rectangle {
                min: [min_x, min_y],
                max: [max_x, max_y],
            } => {
                if !min_x.is_finite()
                    || !min_y.is_finite()
                    || !max_x.is_finite()
                    || !max_y.is_finite()
                {
                    return Err(ScopeError::NonFiniteRegion);
                }
                if min_x > max_x || min_y > max_y {
                    return Err(ScopeError::ReversedRectangle);
                }
            }
        }
    }
    Ok(())
}

fn plot_coordinate(rgb: [f32; 3], space: ScopeSpace) -> (f32, f32) {
    match space {
        ScopeSpace::Ryb => {
            let (hue, chroma) = rgb_hue_chroma(rgb);
            let ryb_hue = rgb_hue_to_ryb_hue(hue);
            let angle = std::f32::consts::TAU * ryb_hue;
            (
                -angle.sin() * chroma * 0.5 + 0.5,
                -angle.cos() * chroma * 0.5 + 0.5,
            )
        }
        ScopeSpace::Cie1931 => {
            let [x, y] = rgb_to_cie1931_xy(rgb);
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

/// Converts a linear RGB sample into the normalised scope coordinate.
#[must_use]
pub fn scope_coordinate(rgb: [f32; 3], space: ScopeSpace) -> [f32; 2] {
    let (x, y) = plot_coordinate(rgb, space);
    [x, y]
}

/// Applies the selected radial display transform to a linear scope coordinate.
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
    let centre = scope_centre(space);
    let delta = [output[0] - centre[0], output[1] - centre[1]];
    let radius = delta[0].hypot(delta[1]);
    if radius <= f32::EPSILON {
        return Some(centre);
    }
    let max_radius = scope_max_radius(space);
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

/// Applies the selected radial display transform in the forward direction.
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
    let centre = scope_centre(space);
    let delta = [source[0] - centre[0], source[1] - centre[1]];
    let radius = delta[0].hypot(delta[1]);
    if radius <= f32::EPSILON {
        return Some(centre);
    }
    let max_radius = scope_max_radius(space);
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

fn scope_centre(space: ScopeSpace) -> [f32; 2] {
    match space {
        ScopeSpace::Ryb => [0.5, 0.5],
        ScopeSpace::Cie1931 => [0.312_7 / CIE_X_MAX, 1.0 - 0.329_0 / CIE_Y_MAX],
    }
}

fn scope_max_radius(space: ScopeSpace) -> f32 {
    match space {
        ScopeSpace::Ryb => 0.5,
        ScopeSpace::Cie1931 => 0.65,
    }
}

/// Renders the pure RGBA reverse-selection overlay used by the harness.
///
/// # Panics
///
/// Panics when the image dimensions, pixel buffer, or selection centre is
/// invalid. Use [`try_render_reverse_highlight`] for untrusted inputs.
#[must_use]
#[allow(clippy::too_many_arguments)]
pub fn render_reverse_highlight(
    rgba: &[u8],
    width: u32,
    height: u32,
    centre: [f32; 2],
    radius: f32,
    space: ScopeSpace,
    density_scale: DensityScale,
    contract: ScopeInputContract,
) -> Vec<u8> {
    try_render_reverse_highlight(
        rgba,
        width,
        height,
        centre,
        radius,
        space,
        density_scale,
        contract,
        &CancellationToken::new(),
    )
    .expect("valid reverse-highlight inputs")
}

/// Fallible form of [`render_reverse_highlight`].
///
/// # Errors
///
/// Returns [`ScopeError`] when the image dimensions, pixel buffer, or
/// selection centre is invalid.
#[allow(clippy::too_many_arguments)]
pub fn try_render_reverse_highlight(
    rgba: &[u8],
    width: u32,
    height: u32,
    centre: [f32; 2],
    radius: f32,
    space: ScopeSpace,
    density_scale: DensityScale,
    _contract: ScopeInputContract,
    cancellation: &CancellationToken,
) -> Result<Vec<u8>, ScopeError> {
    validate_inputs(rgba, width, height, 2, None)?;
    if !centre[0].is_finite() || !centre[1].is_finite() || !radius.is_finite() {
        return Err(ScopeError::NonFiniteRegion);
    }
    if radius < 0.0 {
        return Err(ScopeError::NegativeCircleRadius);
    }
    let width = usize::try_from(width).map_err(|_| ScopeError::DimensionOverflow)?;
    let height = usize::try_from(height).map_err(|_| ScopeError::DimensionOverflow)?;
    let radius = radius.max(0.000_1);
    let mut output = vec![0_u8; rgba.len()];
    let mut processed = 0_usize;
    for y in 0..height {
        for x in 0..width {
            if processed.is_multiple_of(4_096) && cancellation.is_cancelled() {
                return Err(ScopeError::Cancelled);
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
            let edge = (1.0 - distance / radius).powf(0.35);
            output[index] = 255 - rgba[index];
            output[index + 1] = 255 - rgba[index + 1];
            output[index + 2] = 255 - rgba[index + 2];
            output[index + 3] = (edge * 220.0).round() as u8;
        }
    }
    Ok(output)
}

/// Maps standard RGB hue to the reference RYB hue arrangement.
#[must_use]
pub fn rgb_hue_to_ryb_hue(hue: f32) -> f32 {
    let second_derivatives = RGB_TO_RYB_SECOND_DERIVATIVES
        .get_or_init(|| natural_spline_second_derivatives(&RGB_HUE_KNOTS, &RYB_HUE_KNOTS));
    cubic_spline(
        hue.rem_euclid(1.0),
        &RGB_HUE_KNOTS,
        &RYB_HUE_KNOTS,
        second_derivatives,
    )
}

/// Maps RYB hue back to standard RGB hue for the display ring.
#[must_use]
pub fn ryb_hue_to_rgb_hue(hue: f32) -> f32 {
    let target = hue.rem_euclid(1.0);
    let mut low = 0.0_f32;
    let mut high = 1.0_f32;
    for _ in 0..24 {
        let middle = (low + high) * 0.5;
        if rgb_hue_to_ryb_hue(middle) < target {
            low = middle;
        } else {
            high = middle;
        }
    }
    (low + high) * 0.5
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

    fn analyse(rgba: &[u8], width: u32, height: u32, resolution: usize) -> VectorscopeAnalysis {
        super::analyse(
            rgba,
            width,
            height,
            resolution,
            ScopeInputContract::EncodedSrgb8,
        )
    }

    fn analyse_region(
        rgba: &[u8],
        width: u32,
        height: u32,
        resolution: usize,
        region: Option<AnalysisRegion>,
    ) -> VectorscopeAnalysis {
        super::analyse_region(
            rgba,
            width,
            height,
            resolution,
            region,
            ScopeInputContract::EncodedSrgb8,
        )
    }

    fn analyse_cie1931(
        rgba: &[u8],
        width: u32,
        height: u32,
        resolution: usize,
    ) -> VectorscopeAnalysis {
        super::analyse_cie1931(
            rgba,
            width,
            height,
            resolution,
            ScopeInputContract::EncodedSrgb8,
        )
    }

    fn render_reverse_highlight(
        rgba: &[u8],
        width: u32,
        height: u32,
        centre: [f32; 2],
        radius: f32,
        space: ScopeSpace,
        scale: DensityScale,
    ) -> Vec<u8> {
        super::render_reverse_highlight(
            rgba,
            width,
            height,
            centre,
            radius,
            space,
            scale,
            ScopeInputContract::EncodedSrgb8,
        )
    }

    fn try_render_reverse_highlight(
        rgba: &[u8],
        width: u32,
        height: u32,
        centre: [f32; 2],
        radius: f32,
        space: ScopeSpace,
        scale: DensityScale,
    ) -> Result<Vec<u8>, ScopeError> {
        super::try_render_reverse_highlight(
            rgba,
            width,
            height,
            centre,
            radius,
            space,
            scale,
            ScopeInputContract::EncodedSrgb8,
            &CancellationToken::new(),
        )
    }

    fn try_analyse_region_in_space(
        rgba: &[u8],
        width: u32,
        height: u32,
        resolution: usize,
        region: Option<AnalysisRegion>,
        space: ScopeSpace,
    ) -> Result<VectorscopeAnalysis, ScopeError> {
        super::try_analyse_region_in_space(
            rgba,
            width,
            height,
            resolution,
            region,
            space,
            ScopeInputContract::EncodedSrgb8,
            &CancellationToken::new(),
        )
    }

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
            assert!((round_trip - hue.rem_euclid(1.0)).abs() < 1.0e-5);
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
    fn linear_source_coordinates_reject_values_outside_the_display_domain() {
        assert_eq!(
            source_coordinate([-0.01, 0.5], ScopeSpace::Ryb, DensityScale::Linear),
            None
        );
        assert_eq!(
            source_coordinate([0.5, 1.01], ScopeSpace::Ryb, DensityScale::Linear),
            None
        );
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
    fn reverse_highlight_selects_only_matching_scope_colour() {
        let image = [255, 0, 0, 255, 0, 0, 255, 255];
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
        assert!(highlighted[3] > 0);
        assert_eq!(highlighted[7], 0);
    }

    #[test]
    fn reverse_highlight_rejects_a_negative_radius() {
        assert_eq!(
            try_render_reverse_highlight(
                &[255, 0, 0, 255],
                1,
                1,
                [0.5, 0.5],
                -0.1,
                ScopeSpace::Ryb,
                DensityScale::Linear,
            ),
            Err(ScopeError::NegativeCircleRadius)
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
        assert!(highlighted[3] > 0);
    }

    #[test]
    fn ryb_hue_mapping_has_a_continuous_slope_at_internal_knots() {
        let knot = RGB_HUE_KNOTS[1];
        let epsilon = 0.000_1;
        let at_knot = rgb_hue_to_ryb_hue(knot);
        let left_slope = (at_knot - rgb_hue_to_ryb_hue(knot - epsilon)) / epsilon;
        let right_slope = (rgb_hue_to_ryb_hue(knot + epsilon) - at_knot) / epsilon;
        assert!((left_slope - right_slope).abs() < 0.05);
    }

    #[test]
    fn invalid_analysis_boundaries_are_rejected() {
        assert_eq!(
            try_analyse_region_in_space(&[], 1, 1, 1, None, ScopeSpace::Ryb),
            Err(ScopeError::ResolutionTooSmall { actual: 1 })
        );
        assert_eq!(
            try_analyse_region_in_space(&[], 1, 1, 33, None, ScopeSpace::Ryb),
            Err(ScopeError::PixelBufferLength {
                expected: 4,
                actual: 0
            })
        );
        assert_eq!(
            try_analyse_region_in_space(&[], 0, 1, 33, None, ScopeSpace::Ryb),
            Err(ScopeError::ZeroDimension)
        );
        assert_eq!(
            try_analyse_region_in_space(&[], 1, 0, 33, None, ScopeSpace::Ryb),
            Err(ScopeError::ZeroDimension)
        );
        assert_eq!(
            try_analyse_region_in_space(
                &[0; 4],
                1,
                1,
                33,
                Some(AnalysisRegion::Rectangle {
                    min: [1.0, 0.0],
                    max: [0.0, 1.0]
                }),
                ScopeSpace::Ryb
            ),
            Err(ScopeError::ReversedRectangle)
        );
    }

    #[test]
    fn forward_and_reverse_scans_observe_preexisting_cancellation() {
        let cancellation = CancellationToken::new();
        cancellation.cancel();
        assert_eq!(
            super::try_analyse_region_in_space(
                &[255, 0, 0, 255],
                1,
                1,
                33,
                None,
                ScopeSpace::Ryb,
                ScopeInputContract::EncodedSrgb8,
                &cancellation,
            ),
            Err(ScopeError::Cancelled)
        );
        assert_eq!(
            super::try_render_reverse_highlight(
                &[255, 0, 0, 255],
                1,
                1,
                [0.5, 0.5],
                0.1,
                ScopeSpace::Ryb,
                DensityScale::Linear,
                ScopeInputContract::EncodedSrgb8,
                &cancellation,
            ),
            Err(ScopeError::Cancelled)
        );
    }

    #[test]
    fn scope_analysis_requires_an_explicit_srgb_byte_contract() {
        let analysis = super::try_analyse_region_in_space(
            &[128, 128, 128, 255],
            1,
            1,
            33,
            None,
            ScopeSpace::Ryb,
            ScopeInputContract::EncodedSrgb8,
            &CancellationToken::new(),
        )
        .unwrap();
        assert_eq!(analysis.sampled_pixels, 1);
    }

    #[test]
    fn ryb_mapping_round_trips_between_knots() {
        for index in 0..=1_000 {
            let hue = index as f32 / 1_001.0;
            let round_trip = ryb_hue_to_rgb_hue(rgb_hue_to_ryb_hue(hue));
            assert!((round_trip - hue).abs() < 1.0e-5, "hue={hue}");
        }
    }
}
