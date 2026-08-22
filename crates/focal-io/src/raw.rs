use std::{fmt, path::Path};

use focal_core::CancellationToken;
use rawler::{
    RawImageData,
    decoders::RawDecodeParams,
    imgop::{
        matrix::{multiply, normalize, pseudo_inverse},
        xyz::{Illuminant, SRGB_TO_XYZ_D65},
    },
    rawimage::RawPhotometricInterpretation,
};
use rayon::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RawThumbnail {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// Extracts the camera-rendered JPEG embedded in an X-T5 RAF for thumbnail use.
///
/// # Errors
///
/// Returns an error if the embedded preview cannot be decoded or cancellation
/// is observed at a file boundary.
pub fn decode_xt5_thumbnail(
    path: &Path,
    maximum_dimension: u32,
    cancellation: &CancellationToken,
) -> Result<RawThumbnail, RawDecodeError> {
    ensure_not_cancelled(cancellation)?;
    let preview = rawler::analyze::extract_thumbnail_pixels(path, &RawDecodeParams::default())
        .map_err(|error| RawDecodeError::Decoder {
            message: error.to_string(),
        })?
        .thumbnail(maximum_dimension, maximum_dimension)
        .to_rgba8();
    ensure_not_cancelled(cancellation)?;
    Ok(RawThumbnail {
        width: preview.width(),
        height: preview.height(),
        rgba: preview.into_raw(),
    })
}

/// Version of the initial X-T5 Camera-Neutral rendering fitted to the supplied
/// firmware-4.00 Standard JPEG reference.
pub const XT5_CAMERA_NEUTRAL_VERSION: u32 = 3;

/// Opaque, display-encoded output from the X-T5 default rendering.
#[derive(Clone, Debug, PartialEq)]
pub struct CameraNeutralImage {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<[f32; 3]>,
    pub rgba: Vec<u8>,
    pub rendering_version: u32,
}

/// Decodes and develops an X-T5 RAF using the current Camera-Neutral baseline.
///
/// The baseline is intrinsic input rendering, applied before editable
/// adjustments and independently of presets.
///
/// # Errors
///
/// Returns an error for unsupported RAW files, invalid output, decoder
/// failures, or cancellation observed at a processing boundary.
#[allow(clippy::too_many_lines)]
pub fn decode_xt5_camera_neutral(
    path: &Path,
    cancellation: &CancellationToken,
) -> Result<CameraNeutralImage, RawDecodeError> {
    ensure_not_cancelled(cancellation)?;
    let mut raw = rawler::decode_file(path).map_err(|error| RawDecodeError::Decoder {
        message: error.to_string(),
    })?;
    ensure_xt5(&raw)?;
    ensure_not_cancelled(cancellation)?;
    let cfa = match &raw.photometric {
        RawPhotometricInterpretation::Cfa(configuration) => configuration.cfa.clone(),
        _ => return Err(RawDecodeError::UnsupportedPhotometricInterpretation),
    };
    if cfa.width != 6 || cfa.height != 6 {
        return Err(RawDecodeError::UnsupportedCfa {
            width: cfa.width,
            height: cfa.height,
        });
    }
    let matrix = raw
        .color_matrix
        .get(&Illuminant::D65)
        .or_else(|| raw.color_matrix.values().next())
        .ok_or(RawDecodeError::MissingColourMatrix)?
        .clone();
    if matrix.len() != 9 {
        return Err(RawDecodeError::InvalidColourMatrix {
            components: matrix.len(),
        });
    }
    let xyz_to_camera = [
        [matrix[0], matrix[1], matrix[2]],
        [matrix[3], matrix[4], matrix[5]],
        [matrix[6], matrix[7], matrix[8]],
    ];
    let camera_to_rgb = pseudo_inverse(normalize(multiply(&xyz_to_camera, &SRGB_TO_XYZ_D65)));
    let white_balance = raw.wb_coeffs;
    if white_balance[..3].iter().any(|value| !value.is_finite()) {
        return Err(RawDecodeError::InvalidWhiteBalance);
    }
    let crop_area = raw
        .crop_area
        .unwrap_or_else(|| rawler::imgop::Rect::new(rawler::imgop::Point::zero(), raw.dim()));
    raw.apply_scaling()
        .map_err(|error| RawDecodeError::Decoder {
            message: error.to_string(),
        })?;
    let mosaic = raw.data.as_f32().into_owned();
    let kernels = xtrans_interpolation_kernels(&cfa);
    ensure_not_cancelled(cancellation)?;
    let width = u32::try_from(crop_area.d.w).map_err(|_| RawDecodeError::DimensionsOverflow)?;
    let height = u32::try_from(crop_area.d.h).map_err(|_| RawDecodeError::DimensionsOverflow)?;
    let expected = usize::try_from(width)
        .ok()
        .and_then(|width| {
            usize::try_from(height)
                .ok()
                .and_then(|height| width.checked_mul(height))
        })
        .ok_or(RawDecodeError::DimensionsOverflow)?;
    let pixels = (0..expected)
        .into_par_iter()
        .map(|index| {
            if index % 16_384 == 0 {
                ensure_not_cancelled(cancellation)?;
            }
            let output_x = index % crop_area.d.w;
            let output_y = index / crop_area.d.w;
            let sensor_x = crop_area.p.x + output_x;
            let sensor_y = crop_area.p.y + output_y;
            let pixel =
                demosaic_xtrans_pixel(&mosaic, raw.width, sensor_x, sensor_y, &cfa, &kernels);
            let camera = [
                pixel[0] * white_balance[0],
                pixel[1] * white_balance[1],
                pixel[2] * white_balance[2],
            ];
            let rendered = camera_neutral_v3(camera_neutral_v2(
                [
                    camera_to_rgb[0][0] * camera[0]
                        + camera_to_rgb[0][1] * camera[1]
                        + camera_to_rgb[0][2] * camera[2],
                    camera_to_rgb[1][0] * camera[0]
                        + camera_to_rgb[1][1] * camera[1]
                        + camera_to_rgb[1][2] * camera[2],
                    camera_to_rgb[2][0] * camera[0]
                        + camera_to_rgb[2][1] * camera[1]
                        + camera_to_rgb[2][2] * camera[2],
                ]
                .map(srgb_encode),
            ));
            Ok(rendered)
        })
        .collect::<Result<Vec<_>, RawDecodeError>>()?;
    ensure_not_cancelled(cancellation)?;
    if pixels.iter().flatten().any(|value| !value.is_finite()) {
        return Err(RawDecodeError::NonFiniteSample);
    }
    let rgba = pixels
        .iter()
        .flat_map(|pixel| pixel.map(to_byte).into_iter().chain([u8::MAX]))
        .collect();
    Ok(CameraNeutralImage {
        width,
        height,
        pixels,
        rgba,
        rendering_version: XT5_CAMERA_NEUTRAL_VERSION,
    })
}

fn srgb_encode(value: f32) -> f32 {
    let value = value.max(0.0);
    if value <= 0.003_130_8 {
        12.92 * value
    } else {
        1.055 * value.powf(1.0 / 2.4) - 0.055
    }
}

const CAMERA_NEUTRAL_V2: [[f32; 3]; 10] = [
    [-0.006_646_704, -0.130_738_42, -0.132_839_98],
    [1.387_126_6, 0.218_376_92, 0.159_401_91],
    [-1.508_112_8, 1.433_508_6, -0.151_455_36],
    [1.219_672_9, 0.354_379_77, 1.975_367_5],
    [0.020_824_183, -0.086_859_93, 0.233_700_45],
    [-3.827_079_3, 1.751_503_5, -0.459_343_22],
    [-2.190_287_6, 0.985_891_8, 0.275_282_26],
    [6.399_607, -1.005_365, -0.164_195_22],
    [-2.697_126_4, 0.181_222_62, -0.752_900_6],
    [3.139_131_5, -2.386_403_3, 0.372_804],
];

fn camera_neutral_v2([r, g, b]: [f32; 3]) -> [f32; 3] {
    let features = [1.0, r, g, b, r * r, g * g, b * b, r * g, r * b, g * b];
    let mut output = [0.0; 3];
    for (feature, coefficients) in features.into_iter().zip(CAMERA_NEUTRAL_V2) {
        for (channel, coefficient) in output.iter_mut().zip(coefficients) {
            *channel += feature * coefficient;
        }
    }
    output.map(|value| value.clamp(0.0, 1.0))
}

const CAMERA_NEUTRAL_V3: [[f32; 3]; 20] = [
    [-0.051_556_732, 0.023_255_989, 0.015_651_666],
    [1.228_086_7, 0.059_205_364, 0.145_928_89],
    [0.362_139_46, 0.864_640_65, 0.211_541_76],
    [-0.547_718_1, -0.241_257_15, 0.374_248_74],
    [-0.000_351_205, 0.051_622_95, -0.114_352_39],
    [1.644_022_3, 1.255_568_9, 0.108_418_79],
    [1.647_186_6, 0.672_182_5, 1.747_721],
    [-1.324_797_6, -0.810_732_96, -0.924_097_5],
    [0.604_283_4, 0.417_843_97, 0.372_398_5],
    [-1.705_218_2, -0.434_285_94, -0.117_360_055],
    [-0.007_351_607, -0.243_238_73, -0.092_525_356],
    [-2.654_820_2, -1.386_515_1, -0.415_302_37],
    [-1.455_662_4, -0.672_566_6, -1.541_256_7],
    [-1.125_602, 0.514_669_2, 0.450_512_2],
    [0.293_256_34, 0.180_890_63, 0.105_820_4],
    [2.419_898_3, 0.391_915_98, 0.660_549_1],
    [-1.227_188_2, -0.936_580_8, -0.623_467_45],
    [-1.653_918, -1.489_529_3, -1.014_210_5],
    [2.704_054, 1.246_957_3, 0.673_028_77],
    [1.549_503, 1.346_554_6, 0.779_080_7],
];

fn camera_neutral_v3([r, g, b]: [f32; 3]) -> [f32; 3] {
    let features = [
        1.0,
        r,
        g,
        b,
        r * r,
        g * g,
        b * b,
        r * g,
        r * b,
        g * b,
        r * r * r,
        g * g * g,
        b * b * b,
        r * r * g,
        r * r * b,
        g * g * r,
        g * g * b,
        b * b * r,
        b * b * g,
        r * g * b,
    ];
    let mut output = [0.0; 3];
    for (feature, coefficients) in features.into_iter().zip(CAMERA_NEUTRAL_V3) {
        for (channel, coefficient) in output.iter_mut().zip(coefficients) {
            *channel += feature * coefficient;
        }
    }
    output.map(|value| value.clamp(0.0, 1.0))
}

#[derive(Clone, Copy, Debug)]
struct InterpolationSample {
    dx: i32,
    dy: i32,
    weight: f32,
}

fn xtrans_interpolation_kernels(cfa: &rawler::CFA) -> Vec<Vec<InterpolationSample>> {
    let mut kernels = Vec::with_capacity(cfa.width * cfa.height * 3);
    for phase_y in 0..cfa.height {
        for phase_x in 0..cfa.width {
            for channel in 0..3 {
                let mut candidates = Vec::new();
                for dy in -3_i32..=3 {
                    for dx in -3_i32..=3 {
                        if dx == 0 && dy == 0 {
                            continue;
                        }
                        let x = (i32::try_from(phase_x).unwrap_or(0) + dx)
                            .rem_euclid(i32::try_from(cfa.width).unwrap_or(1))
                            as usize;
                        let y = (i32::try_from(phase_y).unwrap_or(0) + dy)
                            .rem_euclid(i32::try_from(cfa.height).unwrap_or(1))
                            as usize;
                        if cfa.color_at(y, x) == channel {
                            let distance_squared = dx * dx + dy * dy;
                            candidates.push((dx, dy, distance_squared));
                        }
                    }
                }
                let nearest = candidates
                    .iter()
                    .map(|candidate| candidate.2)
                    .min()
                    .unwrap_or(1);
                let mut selected = candidates
                    .into_iter()
                    .filter(|candidate| candidate.2 <= nearest + 2)
                    .map(|(dx, dy, distance_squared)| InterpolationSample {
                        dx,
                        dy,
                        weight: 1.0
                            / f32::from(u16::try_from(distance_squared).unwrap_or(u16::MAX)),
                    })
                    .collect::<Vec<_>>();
                let total_weight = selected.iter().map(|sample| sample.weight).sum::<f32>();
                for sample in &mut selected {
                    sample.weight /= total_weight;
                }
                kernels.push(selected);
            }
        }
    }
    kernels
}

fn demosaic_xtrans_pixel(
    mosaic: &[f32],
    sensor_width: usize,
    x: usize,
    y: usize,
    cfa: &rawler::CFA,
    kernels: &[Vec<InterpolationSample>],
) -> [f32; 3] {
    let measured_channel = cfa.color_at(y, x);
    let measured = mosaic[y * sensor_width + x];
    let phase = (y % cfa.height) * cfa.width + x % cfa.width;
    std::array::from_fn(|channel| {
        if channel == measured_channel {
            measured
        } else {
            kernels[phase * 3 + channel]
                .iter()
                .map(|sample| {
                    let sample_x =
                        usize::try_from(i32::try_from(x).unwrap_or(0) + sample.dx).unwrap_or(x);
                    let sample_y =
                        usize::try_from(i32::try_from(y).unwrap_or(0) + sample.dy).unwrap_or(y);
                    mosaic[sample_y * sensor_width + sample_x] * sample.weight
                })
                .sum()
        }
    })
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn to_byte(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

fn ensure_not_cancelled(cancellation: &CancellationToken) -> Result<(), RawDecodeError> {
    if cancellation.is_cancelled() {
        Err(RawDecodeError::Cancelled)
    } else {
        Ok(())
    }
}

/// Decoded and black-level-corrected camera mosaic.
///
/// Values are scene-linear sensor samples. They have not been demosaiced,
/// white-balanced, transformed to display primaries, tone-mapped, or sharpened.
#[derive(Clone, Debug, PartialEq)]
pub struct RawSensorImage {
    pub width: u32,
    pub height: u32,
    pub samples: Vec<f32>,
    pub cfa_width: usize,
    pub cfa_height: usize,
    pub cfa_colours: Vec<usize>,
    pub white_balance: [f32; 4],
    pub xyz_to_camera: [[f32; 3]; 4],
    pub active_area: Option<RawRectangle>,
    pub crop_area: Option<RawRectangle>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RawRectangle {
    pub x: usize,
    pub y: usize,
    pub width: usize,
    pub height: usize,
}

/// Decodes an X-T5 RAF into normalised scene-linear sensor samples.
///
/// # Errors
///
/// Returns an error for unsupported cameras, invalid dimensions or metadata,
/// non-integer mosaics, and decoder failures.
pub fn decode_xt5_raf(path: &Path) -> Result<RawSensorImage, RawDecodeError> {
    let mut raw = rawler::decode_file(path).map_err(|error| RawDecodeError::Decoder {
        message: error.to_string(),
    })?;
    ensure_xt5(&raw)?;
    if raw.cpp != 1 {
        return Err(RawDecodeError::UnsupportedSensorLayout {
            components: raw.cpp,
        });
    }
    let cfa = match &raw.photometric {
        RawPhotometricInterpretation::Cfa(configuration) => &configuration.cfa,
        _ => return Err(RawDecodeError::UnsupportedPhotometricInterpretation),
    };
    let cfa_width = cfa.width;
    let cfa_height = cfa.height;
    let cfa_colours = (0..cfa_height)
        .flat_map(|row| (0..cfa_width).map(move |column| cfa.color_at(row, column)))
        .collect::<Vec<_>>();
    raw.apply_scaling()
        .map_err(|error| RawDecodeError::Decoder {
            message: error.to_string(),
        })?;
    let samples = match raw.data {
        RawImageData::Float(samples) => samples,
        RawImageData::Integer(_) => return Err(RawDecodeError::UnnormalisedIntegerData),
    };
    if samples.iter().any(|sample| !sample.is_finite()) {
        return Err(RawDecodeError::NonFiniteSample);
    }
    let width = u32::try_from(raw.width).map_err(|_| RawDecodeError::DimensionsOverflow)?;
    let height = u32::try_from(raw.height).map_err(|_| RawDecodeError::DimensionsOverflow)?;
    let expected = raw
        .width
        .checked_mul(raw.height)
        .ok_or(RawDecodeError::DimensionsOverflow)?;
    if samples.len() != expected {
        return Err(RawDecodeError::SampleCount {
            expected,
            actual: samples.len(),
        });
    }
    Ok(RawSensorImage {
        width,
        height,
        samples,
        cfa_width,
        cfa_height,
        cfa_colours,
        white_balance: raw.wb_coeffs,
        xyz_to_camera: raw.xyz_to_cam,
        active_area: raw.active_area.map(rectangle),
        crop_area: raw.crop_area.map(rectangle),
    })
}

fn ensure_xt5(raw: &rawler::RawImage) -> Result<(), RawDecodeError> {
    if raw.clean_make == "Fujifilm" && raw.clean_model == "X-T5" {
        Ok(())
    } else {
        Err(RawDecodeError::UnsupportedCamera {
            make: raw.clean_make.clone(),
            model: raw.clean_model.clone(),
        })
    }
}

const fn rectangle(rectangle: rawler::imgop::Rect) -> RawRectangle {
    RawRectangle {
        x: rectangle.p.x,
        y: rectangle.p.y,
        width: rectangle.d.w,
        height: rectangle.d.h,
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RawDecodeError {
    Decoder { message: String },
    Cancelled,
    UnsupportedCamera { make: String, model: String },
    UnsupportedSensorLayout { components: usize },
    UnsupportedCfa { width: usize, height: usize },
    UnsupportedPhotometricInterpretation,
    MissingColourMatrix,
    InvalidColourMatrix { components: usize },
    InvalidWhiteBalance,
    UnnormalisedIntegerData,
    NonFiniteSample,
    DimensionsOverflow,
    SampleCount { expected: usize, actual: usize },
}

impl fmt::Display for RawDecodeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Decoder { message } => write!(formatter, "RAW decoder failed: {message}"),
            Self::Cancelled => formatter.write_str("RAW operation cancelled"),
            Self::UnsupportedCamera { make, model } => {
                write!(formatter, "unsupported RAW camera: {make} {model}")
            }
            Self::UnsupportedSensorLayout { components } => {
                write!(
                    formatter,
                    "unsupported RAW sensor layout with {components} components"
                )
            }
            Self::UnsupportedCfa { width, height } => {
                write!(
                    formatter,
                    "unsupported RAW CFA dimensions: {width}x{height}"
                )
            }
            Self::UnsupportedPhotometricInterpretation => {
                write!(
                    formatter,
                    "RAW does not contain a supported colour-filter mosaic"
                )
            }
            Self::MissingColourMatrix => formatter.write_str("RAW has no camera colour matrix"),
            Self::InvalidColourMatrix { components } => write!(
                formatter,
                "RAW camera colour matrix has {components} components instead of 9"
            ),
            Self::InvalidWhiteBalance => {
                formatter.write_str("RAW has invalid camera white-balance coefficients")
            }
            Self::UnnormalisedIntegerData => write!(formatter, "RAW samples were not normalised"),
            Self::NonFiniteSample => write!(formatter, "RAW contains a non-finite sample"),
            Self::DimensionsOverflow => {
                write!(formatter, "RAW dimensions overflow addressable memory")
            }
            Self::SampleCount { expected, actual } => {
                write!(
                    formatter,
                    "expected {expected} RAW samples, received {actual}"
                )
            }
        }
    }
}

impl std::error::Error for RawDecodeError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn srgb_encoding_is_monotonic_and_handles_scene_values() {
        assert!(srgb_encode(-0.1).abs() < f32::EPSILON);
        assert!(srgb_encode(0.1) > srgb_encode(0.01));
        assert!(srgb_encode(2.0) > 1.0);
    }

    #[test]
    fn camera_neutral_v2_output_is_bounded() {
        for input in [[0.0; 3], [0.1; 3], [0.5, 0.2, 0.9], [1.0; 3]] {
            assert!(
                camera_neutral_v2(input)
                    .into_iter()
                    .all(|value| (0.0..=1.0).contains(&value))
            );
        }
    }

    #[test]
    fn camera_neutral_v3_output_is_bounded() {
        for input in [[0.0; 3], [0.1; 3], [0.5, 0.2, 0.9], [1.0; 3]] {
            assert!(
                camera_neutral_v3(input)
                    .into_iter()
                    .all(|value| (0.0..=1.0).contains(&value))
            );
        }
    }

    #[test]
    fn xtrans_interpolation_reconstructs_constant_colour_without_a_cfa_grid() {
        let cfa = rawler::CFA::new("GGRGGBGGBGGRBRGRGBGGBGGRGGRGGBRBGBRG");
        let width = 18;
        let height = 18;
        let channel_values = [0.2, 0.5, 0.8];
        let cfa_ref = &cfa;
        let mosaic = (0..height)
            .flat_map(|y| (0..width).map(move |x| channel_values[cfa_ref.color_at(y, x)]))
            .collect::<Vec<_>>();
        let kernels = xtrans_interpolation_kernels(&cfa);
        for y in 4..14 {
            for x in 4..14 {
                let pixel = demosaic_xtrans_pixel(&mosaic, width, x, y, &cfa, &kernels);
                for (actual, expected) in pixel.into_iter().zip(channel_values) {
                    assert!((actual - expected).abs() < 1.0e-6);
                }
            }
        }
    }

    #[test]
    fn cancelled_render_is_rejected_before_file_access() {
        let cancellation = CancellationToken::new();
        cancellation.cancel();
        assert_eq!(
            decode_xt5_camera_neutral(Path::new("does-not-exist.RAF"), &cancellation),
            Err(RawDecodeError::Cancelled)
        );
    }
}
