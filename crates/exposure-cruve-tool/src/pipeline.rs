//! A small, explicit CPU reference pipeline for the curve prototype.
//!
//! The input boundary accepts PNG and JPEG. ICC and common JPEG EXIF colour
//! space metadata are inspected, but the selected input space is always
//! explicit before pixels enter the curve pipeline.

#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]

use std::{
    io::Cursor,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use image::{
    ColorType, ImageDecoder, ImageError, ImageFormat,
    codecs::{jpeg::JpegDecoder, png::PngDecoder},
    metadata::Orientation,
};

use crate::curve::{CurveInterpolation, CurveMode, CurveSet, LuminanceDefinition, luma};

const ADOBE_RGB_GAMMA: f32 = 2.199_218_8;
const HISTOGRAM_BINS: usize = 128;
const MAX_HISTOGRAM_SAMPLES: usize = 32_768;

#[derive(Debug)]
pub enum PipelineError {
    Io(std::io::Error),
    Image(ImageError),
    UnsupportedFormat,
    InvalidDimensions,
    PngEncode(png::EncodingError),
}

impl std::fmt::Display for PipelineError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "could not read image: {error}"),
            Self::Image(error) => write!(formatter, "image decode failed: {error}"),
            Self::UnsupportedFormat => write!(formatter, "only PNG and JPEG images are supported"),
            Self::InvalidDimensions => write!(formatter, "image dimensions are invalid"),
            Self::PngEncode(error) => write!(formatter, "PNG encode failed: {error}"),
        }
    }
}

impl std::error::Error for PipelineError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputColourSpace {
    Srgb,
    AdobeRgb,
}

impl InputColourSpace {
    pub const ALL: [Self; 2] = [Self::Srgb, Self::AdobeRgb];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Srgb => "sRGB",
            Self::AdobeRgb => "Adobe RGB",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputFormat {
    Png,
    Jpeg,
}

impl InputFormat {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Png => "PNG",
            Self::Jpeg => "JPEG",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HistogramCalculation {
    FullResolution,
    PreviewSampled,
}

impl HistogramCalculation {
    pub const ALL: [Self; 2] = [Self::FullResolution, Self::PreviewSampled];

    pub const fn label(self) -> &'static str {
        match self {
            Self::FullResolution => "All pixels",
            Self::PreviewSampled => "Preview sample",
        }
    }

    pub const fn description(self) -> &'static str {
        match self {
            Self::FullResolution => "Use every decoded pixel for the binned histogram.",
            Self::PreviewSampled => "Use a bounded sample for faster preview updates.",
        }
    }
}

#[derive(Clone, Debug)]
pub struct EmbeddedProfile {
    pub label: String,
    pub byte_length: usize,
    pub detected_colour_space: Option<InputColourSpace>,
    pub detection_source: String,
}

#[derive(Clone, Debug)]
pub struct SourceImage {
    pub width: u32,
    pub height: u32,
    pub pixels: Arc<Vec<[f32; 3]>>,
    pub profile: EmbeddedProfile,
    pub format: InputFormat,
    pub bit_depth: u8,
}

#[derive(Clone, Debug)]
pub struct PreparedImage {
    pub width: u32,
    pub height: u32,
    /// Values after input-space interpretation, colour conversion, gamut
    /// clipping, and sRGB-like encoding. This is the editable curve domain.
    pub curve_domain: Vec<[f32; 3]>,
    pub before_rgba: Vec<u8>,
    pub source_pixels: Arc<Vec<[f32; 3]>>,
    pub profile: EmbeddedProfile,
    pub format: InputFormat,
    pub bit_depth: u8,
    pub input_colour_space: InputColourSpace,
}

#[derive(Clone, Debug)]
pub struct Histogram {
    /// Rec. 709 luminance counts. The graph intentionally uses one neutral
    /// luminance definition for both RGB and luminance editing modes so the
    /// axes remain comparable.
    pub bins: Vec<f32>,
    pub approximate: bool,
    pub calculation: HistogramCalculation,
}

impl Histogram {
    pub fn max(&self) -> f32 {
        self.bins.iter().copied().fold(0.0, f32::max).max(1.0)
    }
}

#[derive(Clone, Debug)]
pub struct PipelineSnapshot {
    pub mode: CurveMode,
    pub curves: CurveSet,
    pub luminance: LuminanceDefinition,
    pub interpolation: CurveInterpolation,
    pub histogram_calculation: HistogramCalculation,
}

#[derive(Clone, Debug)]
pub struct RenderedPreview {
    pub width: u32,
    pub height: u32,
    pub before_rgba: Vec<u8>,
    pub after_rgba: Vec<u8>,
    pub input_histogram: Histogram,
    pub output_histogram: Histogram,
    pub duration_ms: u128,
}

pub fn decode_image_file(path: &Path) -> Result<SourceImage, PipelineError> {
    let bytes = std::fs::read(path).map_err(PipelineError::Io)?;
    decode_image_bytes(&bytes)
}

pub fn decode_image_bytes(bytes: &[u8]) -> Result<SourceImage, PipelineError> {
    let image_format = image::guess_format(bytes).map_err(PipelineError::Image)?;
    let format = match image_format {
        ImageFormat::Png => InputFormat::Png,
        ImageFormat::Jpeg => InputFormat::Jpeg,
        _ => return Err(PipelineError::UnsupportedFormat),
    };

    let (dimensions, icc_profile, exif_metadata, has_png_srgb_chunk, orientation) = match format {
        InputFormat::Png => {
            let mut decoder = PngDecoder::new(Cursor::new(bytes)).map_err(PipelineError::Image)?;
            let dimensions = decoder.dimensions();
            let icc = decoder.icc_profile().map_err(PipelineError::Image)?;
            let exif = decoder.exif_metadata().map_err(PipelineError::Image)?;
            (
                dimensions,
                icc,
                exif,
                has_png_srgb_chunk(bytes),
                Orientation::NoTransforms,
            )
        }
        InputFormat::Jpeg => {
            let mut decoder = JpegDecoder::new(Cursor::new(bytes)).map_err(PipelineError::Image)?;
            let dimensions = decoder.dimensions();
            let icc = decoder.icc_profile().map_err(PipelineError::Image)?;
            let exif = decoder.exif_metadata().map_err(PipelineError::Image)?;
            let orientation = decoder.orientation().map_err(PipelineError::Image)?;
            (dimensions, icc, exif, false, orientation)
        }
    };
    if dimensions.0 == 0 || dimensions.1 == 0 {
        return Err(PipelineError::InvalidDimensions);
    }

    let detected_from_icc = icc_profile.as_deref().and_then(detect_icc_colour_space);
    let detected_from_exif = exif_metadata.as_deref().and_then(detect_exif_colour_space);
    let detected_from_png = has_png_srgb_chunk.then_some(InputColourSpace::Srgb);
    let detected_colour_space = detected_from_icc
        .or(detected_from_exif)
        .or(detected_from_png);
    let profile = EmbeddedProfile {
        label: icc_profile
            .as_deref()
            .map_or_else(|| "No embedded ICC profile".to_owned(), identify_profile),
        byte_length: icc_profile.as_ref().map_or(0, Vec::len),
        detected_colour_space,
        detection_source: if let Some(colour_space) = detected_from_icc {
            format!("embedded ICC metadata → {}", colour_space.label())
        } else if let Some(colour_space) = detected_from_exif {
            format!("EXIF ColorSpace metadata → {}", colour_space.label())
        } else if let Some(colour_space) = detected_from_png {
            format!("PNG sRGB chunk metadata → {}", colour_space.label())
        } else if icc_profile.is_some() {
            "embedded ICC profile (space not recognised)".to_owned()
        } else {
            "no recognised colour-space metadata".to_owned()
        },
    };

    let mut image =
        image::load_from_memory_with_format(bytes, image_format).map_err(PipelineError::Image)?;
    // `load_from_memory_with_format` decodes the JPEG pixels but does not
    // apply the EXIF display orientation. Do it before collecting pixels so
    // the preview dimensions and pixel order agree with what the user sees.
    image.apply_orientation(orientation);
    let dimensions = (image.width(), image.height());
    let bit_depth = bit_depth(image.color());
    let image_buffer = image.to_rgb32f();
    let pixels = image_buffer
        .pixels()
        .map(|pixel| {
            [
                pixel[0].clamp(0.0, 1.0),
                pixel[1].clamp(0.0, 1.0),
                pixel[2].clamp(0.0, 1.0),
            ]
        })
        .collect();

    Ok(SourceImage {
        width: dimensions.0,
        height: dimensions.1,
        pixels: Arc::new(pixels),
        profile,
        format,
        bit_depth,
    })
}

pub fn prepare(source: &SourceImage, input_colour_space: InputColourSpace) -> PreparedImage {
    prepare_pixels(
        source.width,
        source.height,
        source.pixels.clone(),
        source.profile.clone(),
        source.format,
        source.bit_depth,
        input_colour_space,
    )
}

pub fn reprepare(source: &PreparedImage, input_colour_space: InputColourSpace) -> PreparedImage {
    prepare_pixels(
        source.width,
        source.height,
        source.source_pixels.clone(),
        source.profile.clone(),
        source.format,
        source.bit_depth,
        input_colour_space,
    )
}

fn prepare_pixels(
    width: u32,
    height: u32,
    source_pixels: Arc<Vec<[f32; 3]>>,
    profile: EmbeddedProfile,
    format: InputFormat,
    bit_depth: u8,
    input_colour_space: InputColourSpace,
) -> PreparedImage {
    let mut curve_domain = Vec::with_capacity(source_pixels.len());
    let mut before_rgba = Vec::with_capacity(source_pixels.len() * 4);
    for encoded in source_pixels.iter().copied() {
        let srgb_encoded = input_to_srgb_curve_domain(encoded, input_colour_space);
        curve_domain.push(srgb_encoded);
        before_rgba.extend_from_slice(&[
            to_byte(srgb_encoded[0]),
            to_byte(srgb_encoded[1]),
            to_byte(srgb_encoded[2]),
            255,
        ]);
    }

    PreparedImage {
        width,
        height,
        curve_domain,
        before_rgba,
        source_pixels,
        profile,
        format,
        bit_depth,
        input_colour_space,
    }
}

pub fn render<F: FnMut(f32)>(
    prepared: &PreparedImage,
    snapshot: &PipelineSnapshot,
    cancelled: &AtomicBool,
    mut progress: F,
) -> Option<RenderedPreview> {
    let started = std::time::Instant::now();
    let mut after_rgba = Vec::with_capacity(prepared.curve_domain.len() * 4);
    let mut input_histogram = Histogram::new(snapshot.histogram_calculation);
    let mut output_histogram = Histogram::new(snapshot.histogram_calculation);
    progress(0.0);

    for (index, rgb) in prepared.curve_domain.iter().copied().enumerate() {
        if cancelled.load(Ordering::Relaxed) {
            return None;
        }
        let adjusted = snapshot.curves.apply_with_luminance_and_interpolation(
            snapshot.mode,
            rgb,
            snapshot.luminance,
            snapshot.interpolation,
        );
        if snapshot
            .histogram_calculation
            .includes_pixel(index, prepared.curve_domain.len())
        {
            input_histogram.add(rgb);
            output_histogram.add(adjusted);
        }
        after_rgba.extend_from_slice(&[
            to_byte(adjusted[0]),
            to_byte(adjusted[1]),
            to_byte(adjusted[2]),
            255,
        ]);

        if index % (prepared.width as usize).max(1) == 0 {
            progress(index as f32 / prepared.curve_domain.len().max(1) as f32);
        }
    }
    progress(1.0);

    Some(RenderedPreview {
        width: prepared.width,
        height: prepared.height,
        before_rgba: prepared.before_rgba.clone(),
        after_rgba,
        input_histogram,
        output_histogram,
        duration_ms: started.elapsed().as_millis(),
    })
}

pub fn encode_srgb_png(preview: &RenderedPreview) -> Result<Vec<u8>, PipelineError> {
    use png::{BitDepth, ColorType, Encoder, SrgbRenderingIntent};

    let mut bytes = Vec::new();
    {
        let mut encoder = Encoder::new(&mut bytes, preview.width, preview.height);
        encoder.set_color(ColorType::Rgba);
        encoder.set_depth(BitDepth::Eight);
        encoder.set_source_srgb(SrgbRenderingIntent::Perceptual);
        let mut writer = encoder.write_header().map_err(PipelineError::PngEncode)?;
        writer
            .write_image_data(&preview.after_rgba)
            .map_err(PipelineError::PngEncode)?;
    }
    Ok(bytes)
}

impl Histogram {
    fn new(calculation: HistogramCalculation) -> Self {
        Self {
            bins: vec![0.0; HISTOGRAM_BINS],
            // The graph uses quantised bins over a preview, so this is
            // intentionally labelled approximate in the interface.
            approximate: true,
            calculation,
        }
    }

    fn add(&mut self, rgb: [f32; 3]) {
        let index = histogram_index(luma(rgb));
        self.bins[index] += 1.0;
    }
}

impl HistogramCalculation {
    fn includes_pixel(self, index: usize, total: usize) -> bool {
        match self {
            Self::FullResolution => true,
            Self::PreviewSampled => {
                let stride = total.div_ceil(MAX_HISTOGRAM_SAMPLES).max(1);
                index.is_multiple_of(stride)
            }
        }
    }
}

fn bit_depth(colour_type: ColorType) -> u8 {
    match colour_type {
        ColorType::L16 | ColorType::La16 | ColorType::Rgb16 | ColorType::Rgba16 => 16,
        _ => 8,
    }
}

fn identify_profile(bytes: &[u8]) -> String {
    if detect_icc_colour_space(bytes) == Some(InputColourSpace::AdobeRgb) {
        "Adobe RGB (1998) ICC profile".to_owned()
    } else if detect_icc_colour_space(bytes) == Some(InputColourSpace::Srgb) {
        "sRGB ICC profile".to_owned()
    } else {
        "Embedded ICC profile".to_owned()
    }
}

fn detect_icc_colour_space(bytes: &[u8]) -> Option<InputColourSpace> {
    let adobe = bytes.windows(5).any(|window| window == b"Adobe")
        || bytes.windows(3).any(|window| window == b"A98")
        || bytes.windows(10).any(|window| window == b"\0A\0d\0o\0b\0e");
    if adobe {
        return Some(InputColourSpace::AdobeRgb);
    }

    let srgb = bytes.windows(4).any(|window| window == b"sRGB")
        || bytes.windows(8).any(|window| window == b"\0s\0R\0G\0B");
    srgb.then_some(InputColourSpace::Srgb)
}

fn detect_exif_colour_space(bytes: &[u8]) -> Option<InputColourSpace> {
    if bytes.len() < 8 {
        return None;
    }
    let little_endian = match &bytes[0..2] {
        b"II" => true,
        b"MM" => false,
        _ => return None,
    };
    if read_u16(bytes, 2, little_endian)? != 42 {
        return None;
    }
    let ifd_offset = usize::try_from(read_u32(bytes, 4, little_endian)?).ok()?;
    parse_exif_ifd(bytes, ifd_offset, little_endian, 0)
}

fn has_png_srgb_chunk(bytes: &[u8]) -> bool {
    const PNG_SIGNATURE: &[u8; 8] = b"\x89PNG\r\n\x1a\n";
    if !bytes.starts_with(PNG_SIGNATURE) {
        return false;
    }

    let mut offset = PNG_SIGNATURE.len();
    while let Some(end) = offset.checked_add(12) {
        if end > bytes.len() {
            return false;
        }
        let length = usize::try_from(u32::from_be_bytes(
            bytes[offset..offset + 4]
                .try_into()
                .expect("PNG length is four bytes"),
        ))
        .ok();
        let Some(length) = length else {
            return false;
        };
        let Some(chunk_end) = offset
            .checked_add(12)
            .and_then(|value| value.checked_add(length))
        else {
            return false;
        };
        if chunk_end > bytes.len() {
            return false;
        }
        if &bytes[offset + 4..offset + 8] == b"sRGB" {
            return length == 1;
        }
        if &bytes[offset + 4..offset + 8] == b"IEND" {
            return false;
        }
        offset = chunk_end;
    }
    false
}

fn parse_exif_ifd(
    bytes: &[u8],
    offset: usize,
    little_endian: bool,
    depth: u8,
) -> Option<InputColourSpace> {
    if depth > 2 || offset + 2 > bytes.len() {
        return None;
    }
    let count = usize::from(read_u16(bytes, offset, little_endian)?);
    for index in 0..count {
        let entry = offset.checked_add(2 + index.checked_mul(12)?)?;
        if entry + 12 > bytes.len() {
            return None;
        }
        let tag = read_u16(bytes, entry, little_endian)?;
        let value_type = read_u16(bytes, entry + 2, little_endian)?;
        if tag == 0xA001 && value_type == 3 {
            return match read_u16(bytes, entry + 8, little_endian)? {
                1 => Some(InputColourSpace::Srgb),
                2 => Some(InputColourSpace::AdobeRgb),
                _ => None,
            };
        }
        if tag == 0x8769 && value_type == 4 {
            let child_offset = usize::try_from(read_u32(bytes, entry + 8, little_endian)?).ok()?;
            if let Some(space) = parse_exif_ifd(bytes, child_offset, little_endian, depth + 1) {
                return Some(space);
            }
        }
    }
    None
}

fn read_u16(bytes: &[u8], offset: usize, little_endian: bool) -> Option<u16> {
    let bytes = bytes.get(offset..offset + 2)?;
    Some(if little_endian {
        u16::from_le_bytes(bytes.try_into().ok()?)
    } else {
        u16::from_be_bytes(bytes.try_into().ok()?)
    })
}

fn read_u32(bytes: &[u8], offset: usize, little_endian: bool) -> Option<u32> {
    let bytes = bytes.get(offset..offset + 4)?;
    Some(if little_endian {
        u32::from_le_bytes(bytes.try_into().ok()?)
    } else {
        u32::from_be_bytes(bytes.try_into().ok()?)
    })
}

fn histogram_index(value: f32) -> usize {
    ((value.clamp(0.0, 1.0) * (HISTOGRAM_BINS - 1) as f32).round() as usize).min(HISTOGRAM_BINS - 1)
}

fn to_byte(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

fn input_to_srgb_curve_domain(encoded: [f32; 3], input_colour_space: InputColourSpace) -> [f32; 3] {
    match input_colour_space {
        InputColourSpace::Srgb => encoded.map(|channel| channel.clamp(0.0, 1.0)),
        InputColourSpace::AdobeRgb => adobe_rgb_to_srgb_curve_domain(encoded),
    }
}

fn adobe_rgb_to_srgb_curve_domain(encoded: [f32; 3]) -> [f32; 3] {
    let adobe_linear = encoded.map(|channel| channel.clamp(0.0, 1.0).powf(ADOBE_RGB_GAMMA));

    // Adobe RGB (1998), D65 primaries to linear sRGB, D65. This is a compact
    // reference transform for the controlled fixture, not a general ICC
    // colour-management engine.
    let linear_srgb = [
        2.041_369 * adobe_linear[0] - 0.564_946_4 * adobe_linear[1] - 0.344_694_4 * adobe_linear[2],
        -0.969_266 * adobe_linear[0] + 1.876_010_8 * adobe_linear[1] + 0.041_556 * adobe_linear[2],
        0.013_447_4 * adobe_linear[0] - 0.118_389_7 * adobe_linear[1]
            + 1.015_409_6 * adobe_linear[2],
    ];

    // Gamut handling is explicit and deliberately simple for this prototype:
    // clip in linear sRGB before encoding. A perceptual gamut mapper belongs
    // outside the editable curve once the colour-management choice is settled.
    linear_srgb.map(encode_srgb)
}

fn encode_srgb(linear: f32) -> f32 {
    let linear = linear.clamp(0.0, 1.0);
    if linear <= 0.003_130_8 {
        12.92 * linear
    } else {
        1.055 * linear.powf(1.0 / 2.4) - 0.055
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicBool;

    use image::{ImageBuffer, Rgb, codecs::jpeg::JpegEncoder};

    use super::{
        HistogramCalculation, InputColourSpace, InputFormat, LuminanceDefinition, PipelineSnapshot,
        decode_image_bytes, encode_srgb_png, prepare, render,
    };
    use crate::curve::{CurveMode, CurveSet};

    #[test]
    fn controlled_fixture_is_sixteen_bit_and_profiled() {
        let source = decode_image_bytes(include_bytes!(concat!(
            env!("OUT_DIR"),
            "/controlled_adobe_rgb.png"
        )))
        .expect("fixture decodes");
        assert_eq!(source.format, InputFormat::Png);
        assert_eq!(source.bit_depth, 16);
        assert_eq!(
            source.profile.detected_colour_space,
            Some(InputColourSpace::AdobeRgb)
        );
        assert_eq!(source.pixels.len(), (source.width * source.height) as usize);
    }

    #[test]
    fn jpeg_input_is_decoded_as_eight_bit_when_metadata_is_absent() {
        let image = ImageBuffer::from_pixel(2, 2, Rgb([32_u8, 128_u8, 240_u8]));
        let mut bytes = Vec::new();
        JpegEncoder::new_with_quality(&mut bytes, 90)
            .encode_image(&image)
            .expect("encode JPEG fixture");
        let source = decode_image_bytes(&bytes).expect("JPEG decodes");
        assert_eq!(source.format, InputFormat::Jpeg);
        assert_eq!(source.bit_depth, 8);
        assert_eq!(source.profile.detected_colour_space, None);
    }

    #[test]
    fn jpeg_exif_colour_space_is_detected() {
        let image = ImageBuffer::from_pixel(2, 2, Rgb([32_u8, 128_u8, 240_u8]));
        let mut jpeg = Vec::new();
        JpegEncoder::new_with_quality(&mut jpeg, 90)
            .encode_image(&image)
            .expect("encode JPEG fixture");

        // APP1 EXIF with a little-endian TIFF containing ColorSpace = 2
        // (Adobe RGB). The JPEG decoder exposes the payload from the TIFF
        // header, so the six-byte Exif prefix belongs only to the APP1 block.
        let tiff = [
            b'I', b'I', 42, 0, 8, 0, 0, 0, // TIFF header and IFD offset
            1, 0, // one IFD entry
            1, 0xA0, 3, 0, 1, 0, 0, 0, 2, 0, 0, 0, // ColorSpace = Adobe RGB
            0, 0, 0, 0, // next IFD
        ];
        let mut exif = b"Exif\0\0".to_vec();
        exif.extend_from_slice(&tiff);
        let segment_length = u16::try_from(exif.len() + 2).expect("small EXIF fixture");
        let mut with_exif = vec![0xFF, 0xD8, 0xFF, 0xE1];
        with_exif.extend_from_slice(&segment_length.to_be_bytes());
        with_exif.extend_from_slice(&exif);
        with_exif.extend_from_slice(&jpeg[2..]);

        let source = decode_image_bytes(&with_exif).expect("JPEG with EXIF decodes");
        assert_eq!(
            source.profile.detected_colour_space,
            Some(InputColourSpace::AdobeRgb)
        );
    }

    #[test]
    fn jpeg_exif_orientation_is_applied_before_pixels_are_exposed() {
        let image = ImageBuffer::from_fn(2, 1, |x, _| {
            if x == 0 {
                Rgb([255_u8, 0, 0])
            } else {
                Rgb([0_u8, 0, 255])
            }
        });
        let mut jpeg = Vec::new();
        JpegEncoder::new_with_quality(&mut jpeg, 90)
            .encode_image(&image)
            .expect("encode JPEG fixture");

        // APP1 EXIF with orientation 6 (rotate 90° clockwise) and sRGB
        // ColorSpace metadata in the same little-endian IFD.
        let tiff = [
            b'I', b'I', 42, 0, 8, 0, 0, 0, // TIFF header and IFD offset
            2, 0, // two IFD entries
            0x12, 0x01, 3, 0, 1, 0, 0, 0, 6, 0, 0, 0, // Orientation = 6
            1, 0xA0, 3, 0, 1, 0, 0, 0, 1, 0, 0, 0, // ColorSpace = sRGB
            0, 0, 0, 0, // next IFD
        ];
        let mut exif = b"Exif\0\0".to_vec();
        exif.extend_from_slice(&tiff);
        let segment_length = u16::try_from(exif.len() + 2).expect("small EXIF fixture");
        let mut with_exif = vec![0xFF, 0xD8, 0xFF, 0xE1];
        with_exif.extend_from_slice(&segment_length.to_be_bytes());
        with_exif.extend_from_slice(&exif);
        with_exif.extend_from_slice(&jpeg[2..]);

        let source = decode_image_bytes(&with_exif).expect("oriented JPEG decodes");
        assert_eq!((source.width, source.height), (1, 2));
        assert_eq!(
            source.profile.detected_colour_space,
            Some(InputColourSpace::Srgb)
        );
    }

    #[test]
    fn png_srgb_chunk_is_used_when_no_profile_is_embedded() {
        let mut bytes = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut bytes, 1, 1);
            encoder.set_color(png::ColorType::Rgb);
            encoder.set_depth(png::BitDepth::Eight);
            encoder.set_source_srgb(png::SrgbRenderingIntent::Perceptual);
            let mut writer = encoder.write_header().expect("PNG header encodes");
            writer
                .write_image_data(&[32, 128, 240])
                .expect("PNG data encodes");
        }

        let source = decode_image_bytes(&bytes).expect("PNG with sRGB chunk decodes");
        assert_eq!(
            source.profile.detected_colour_space,
            Some(InputColourSpace::Srgb)
        );
        assert!(source.profile.detection_source.contains("PNG sRGB"));
    }

    #[test]
    fn selected_srgb_space_bypasses_adobe_conversion() {
        let source = decode_image_bytes(include_bytes!(concat!(
            env!("OUT_DIR"),
            "/controlled_adobe_rgb.png"
        )))
        .expect("fixture decodes");
        let adobe = prepare(&source, InputColourSpace::AdobeRgb);
        let srgb = prepare(&source, InputColourSpace::Srgb);
        assert!(
            adobe.curve_domain[100]
                .iter()
                .zip(srgb.curve_domain[100])
                .any(|(adobe, srgb)| (adobe - srgb).abs() > 1e-6)
        );
    }

    #[test]
    fn identity_render_preserves_before_and_after_pixels() {
        let source = decode_image_bytes(include_bytes!(concat!(
            env!("OUT_DIR"),
            "/controlled_adobe_rgb.png"
        )))
        .expect("fixture decodes");
        let prepared = prepare(&source, InputColourSpace::AdobeRgb);
        let snapshot = PipelineSnapshot {
            mode: CurveMode::LinkedRgb,
            curves: CurveSet::default(),
            luminance: LuminanceDefinition::Rec709,
            interpolation: crate::curve::CurveInterpolation::Smooth,
            histogram_calculation: HistogramCalculation::FullResolution,
        };
        let rendered = render(&prepared, &snapshot, &AtomicBool::new(false), |_| {})
            .expect("render completes");
        assert_eq!(rendered.before_rgba, rendered.after_rgba);
    }

    #[test]
    fn histogram_calculation_can_switch_between_all_pixels_and_a_bounded_sample() {
        let source = decode_image_bytes(include_bytes!(concat!(
            env!("OUT_DIR"),
            "/controlled_adobe_rgb.png"
        )))
        .expect("fixture decodes");
        let prepared = prepare(&source, InputColourSpace::AdobeRgb);
        let render_for = |calculation| {
            render(
                &prepared,
                &PipelineSnapshot {
                    mode: CurveMode::LinkedRgb,
                    curves: CurveSet::default(),
                    luminance: LuminanceDefinition::Rec709,
                    interpolation: crate::curve::CurveInterpolation::Smooth,
                    histogram_calculation: calculation,
                },
                &AtomicBool::new(false),
                |_| {},
            )
            .expect("render completes")
        };
        let full = render_for(HistogramCalculation::FullResolution);
        let sampled = render_for(HistogramCalculation::PreviewSampled);
        let full_count: f32 = full.input_histogram.bins.iter().copied().sum();
        let sampled_count: f32 = sampled.input_histogram.bins.iter().copied().sum();
        assert!((full_count - source.pixels.len() as f32).abs() < f32::EPSILON);
        assert!((sampled_count - (source.pixels.len() / 2) as f32).abs() < f32::EPSILON);
        assert_eq!(
            sampled.input_histogram.calculation,
            HistogramCalculation::PreviewSampled
        );
    }

    #[test]
    fn histograms_use_rec709_luminance_for_all_curve_modes() {
        let source = super::SourceImage {
            width: 2,
            height: 1,
            pixels: std::sync::Arc::new(vec![[1.0, 0.0, 0.0], [0.0, 1.0, 0.0]]),
            profile: super::EmbeddedProfile {
                label: "test".to_owned(),
                byte_length: 0,
                detected_colour_space: Some(InputColourSpace::Srgb),
                detection_source: "test".to_owned(),
            },
            format: InputFormat::Png,
            bit_depth: 8,
        };
        let prepared = prepare(&source, InputColourSpace::Srgb);
        let rendered = render(
            &prepared,
            &PipelineSnapshot {
                mode: CurveMode::PerChannelRgb,
                curves: CurveSet::default(),
                luminance: LuminanceDefinition::Rec709,
                interpolation: crate::curve::CurveInterpolation::Smooth,
                histogram_calculation: HistogramCalculation::FullResolution,
            },
            &AtomicBool::new(false),
            |_| {},
        )
        .expect("render completes");
        let occupied: Vec<usize> = rendered
            .input_histogram
            .bins
            .iter()
            .enumerate()
            .filter_map(|(index, count)| (*count > 0.0).then_some(index))
            .collect();
        assert_eq!(occupied, vec![27, 91]);
    }

    #[test]
    fn exported_png_is_tagged_as_srgb() {
        let preview = super::RenderedPreview {
            width: 1,
            height: 1,
            before_rgba: vec![0, 0, 0, 255],
            after_rgba: vec![32, 96, 224, 255],
            input_histogram: super::Histogram {
                bins: vec![0.0; 128],
                approximate: true,
                calculation: HistogramCalculation::FullResolution,
            },
            output_histogram: super::Histogram {
                bins: vec![0.0; 128],
                approximate: true,
                calculation: HistogramCalculation::FullResolution,
            },
            duration_ms: 0,
        };
        let bytes = encode_srgb_png(&preview).expect("PNG encodes");
        let reader = png::Decoder::new(std::io::Cursor::new(bytes))
            .read_info()
            .expect("PNG decodes");
        assert_eq!(
            reader.info().srgb,
            Some(png::SrgbRenderingIntent::Perceptual)
        );
    }

    #[test]
    #[ignore = "known bug FP-CURVE-001: Adobe RGB is clipped to sRGB before the editable curve"]
    fn wide_gamut_values_remain_distinct_until_after_the_editable_curve() {
        let source = super::SourceImage {
            width: 2,
            height: 1,
            pixels: std::sync::Arc::new(vec![[0.8, 0.0, 0.0], [1.0, 0.0, 0.0]]),
            profile: super::EmbeddedProfile {
                label: "test Adobe RGB".to_owned(),
                byte_length: 0,
                detected_colour_space: Some(InputColourSpace::AdobeRgb),
                detection_source: "test".to_owned(),
            },
            format: InputFormat::Png,
            bit_depth: 16,
        };

        let prepared = prepare(&source, InputColourSpace::AdobeRgb);
        assert!(
            prepared.curve_domain[0][0] < prepared.curve_domain[1][0],
            "premature sRGB gamut clipping makes distinct Adobe RGB reds identical before the user curve"
        );
    }
}
