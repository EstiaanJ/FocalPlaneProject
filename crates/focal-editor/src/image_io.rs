#![allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]

use std::{
    fmt,
    io::{self, Cursor},
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver, Sender},
};

use focal_core::{
    CancellationToken, ColourEncoding, Image, ImageContract, ImageError, Pipeline,
    PipelineSnapshot, RenderContext, RenderQuality,
};
use image::{
    ExtendedColorType, ImageDecoder, ImageEncoder, ImageFormat,
    codecs::{jpeg::JpegDecoder, png::PngDecoder, tiff::TiffDecoder},
};
use moxcms::{ColorProfile, Layout, TransformOptions};

/// A decoded, displayable source image owned by the editor.
#[derive(Clone, Debug, PartialEq)]
pub struct DecodedImage {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    pub pixels: Vec<[f32; 3]>,
    pub alpha: Vec<f32>,
    pub input_contract: ImageContract,
    pub has_transparency: bool,
}

impl DecodedImage {
    /// Converts the decoded source to the opaque RGB contract used by the
    /// current `FocalCore` prototype.
    pub fn to_core_image(&self) -> Result<Image, ImageError> {
        Image::new(
            self.width,
            self.height,
            self.pixels.clone(),
            self.input_contract,
        )
    }

    /// Flattens transparency onto black in linear light.
    #[must_use = "the image or flattening error must be handled"]
    #[cfg(test)]
    pub fn flatten_onto_black(self) -> Result<Self, ImageIoError> {
        self.flatten_onto_black_with_cancellation(&CancellationToken::new())
    }

    fn flatten_onto_black_with_cancellation(
        mut self,
        cancellation: &CancellationToken,
    ) -> Result<Self, ImageIoError> {
        for ((pixel, alpha), rgba) in self
            .pixels
            .iter_mut()
            .zip(self.alpha.iter().copied())
            .zip(self.rgba.chunks_exact_mut(4))
        {
            if cancellation.is_cancelled() {
                return Err(ImageIoError::Cancelled);
            }
            for channel in pixel.iter_mut() {
                let linear = decode_channel(*channel, self.input_contract.encoding) * alpha;
                *channel = encode_channel(linear, self.input_contract.encoding);
            }
            rgba[3] = u8::MAX;
        }
        self.alpha.fill(1.0);
        self.has_transparency = false;
        self.rgba = display_rgba_from_working_pixels(&self.pixels, self.input_contract)?;
        Ok(self)
    }
}

fn decode_with_cancellation(
    path: &Path,
    cancellation: &CancellationToken,
) -> Result<DecodedImage, ImageIoError> {
    if is_raf_path(path) {
        let rendered = focal_io::decode_xt5_camera_neutral(path, cancellation)
            .map_err(|error| ImageIoError::Raw(error.to_string()))?;
        return Ok(DecodedImage {
            width: rendered.width,
            height: rendered.height,
            rgba: rendered.rgba,
            pixels: rendered.pixels,
            alpha: vec![
                1.0;
                usize::try_from(rendered.width)
                    .ok()
                    .and_then(|width| usize::try_from(rendered.height)
                        .ok()
                        .and_then(|height| width.checked_mul(height)))
                    .ok_or_else(|| ImageIoError::Raw(
                        "RAW dimensions overflow addressable memory".to_owned()
                    ))?
            ],
            input_contract: ImageContract::SRGB_DISPLAY,
            has_transparency: false,
        });
    }
    let bytes = std::fs::read(path).map_err(|source| ImageIoError::Open {
        path: path.to_path_buf(),
        source,
    })?;
    ensure_not_cancelled(cancellation)?;
    decode_bytes_with_cancellation(&bytes, cancellation)
}

fn is_raf_path(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("raf"))
}

#[cfg(test)]
fn decoded_image_from_dynamic(image: &image::DynamicImage) -> DecodedImage {
    decoded_image_from_dynamic_with_profile(image, None)
        .expect("the built-in sRGB interpretation is valid")
}

#[cfg(test)]
fn decode_bytes(bytes: &[u8]) -> Result<DecodedImage, ImageIoError> {
    decode_bytes_with_cancellation(bytes, &CancellationToken::new())
}

fn decode_bytes_with_cancellation(
    bytes: &[u8],
    cancellation: &CancellationToken,
) -> Result<DecodedImage, ImageIoError> {
    ensure_not_cancelled(cancellation)?;
    let format = image::guess_format(bytes).map_err(ImageIoError::Decode)?;
    let (icc, orientation) = match format {
        ImageFormat::Jpeg => {
            let mut decoder = JpegDecoder::new(Cursor::new(bytes)).map_err(ImageIoError::Decode)?;
            (
                decoder.icc_profile().map_err(ImageIoError::Decode)?,
                decoder.orientation().map_err(ImageIoError::Decode)?,
            )
        }
        ImageFormat::Png => {
            let mut decoder = PngDecoder::new(Cursor::new(bytes)).map_err(ImageIoError::Decode)?;
            (
                decoder.icc_profile().map_err(ImageIoError::Decode)?,
                decoder.orientation().map_err(ImageIoError::Decode)?,
            )
        }
        ImageFormat::Tiff => {
            let mut decoder = TiffDecoder::new(Cursor::new(bytes)).map_err(ImageIoError::Decode)?;
            (
                decoder.icc_profile().map_err(ImageIoError::Decode)?,
                decoder.orientation().map_err(ImageIoError::Decode)?,
            )
        }
        _ => return Err(ImageIoError::UnsupportedFormat),
    };
    let mut image =
        image::load_from_memory_with_format(bytes, format).map_err(ImageIoError::Decode)?;
    ensure_not_cancelled(cancellation)?;
    image.apply_orientation(orientation);
    let decoded = decoded_image_from_dynamic_with_profile(&image, icc.as_deref())?;
    ensure_not_cancelled(cancellation)?;
    Ok(decoded)
}

fn decoded_image_from_dynamic_with_profile(
    image: &image::DynamicImage,
    icc: Option<&[u8]>,
) -> Result<DecodedImage, ImageIoError> {
    let width = image.width();
    let height = image.height();
    let source_rgba = image.to_rgba32f().into_raw();
    let alpha = source_rgba
        .chunks_exact(4)
        .map(|pixel| pixel[3])
        .collect::<Vec<_>>();
    let has_transparency = alpha.iter().any(|value| *value < 1.0);
    let (working_rgba, display_rgba, input_contract) = if let Some(icc) = icc {
        let source = ColorProfile::new_from_slice(icc)
            .map_err(|error| ImageIoError::ColourProfile(error.to_string()))?;
        let transform = |destination: &ColorProfile| -> Result<Vec<f32>, ImageIoError> {
            let executor = source
                .create_transform_f32(
                    Layout::Rgba,
                    destination,
                    Layout::Rgba,
                    TransformOptions::default(),
                )
                .map_err(|error| ImageIoError::ColourProfile(error.to_string()))?;
            let mut output = vec![0.0; source_rgba.len()];
            executor
                .transform(&source_rgba, &mut output)
                .map_err(|error| ImageIoError::ColourProfile(error.to_string()))?;
            Ok(output)
        };
        (
            transform(&ColorProfile::new_adobe_rgb())?,
            transform(&ColorProfile::new_srgb())?,
            ImageContract::ADOBE_RGB_CURVE,
        )
    } else {
        (
            source_rgba.clone(),
            source_rgba,
            ImageContract::SRGB_DISPLAY,
        )
    };
    let pixels = working_rgba
        .chunks_exact(4)
        .map(|pixel| {
            [
                pixel[0].clamp(0.0, 1.0),
                pixel[1].clamp(0.0, 1.0),
                pixel[2].clamp(0.0, 1.0),
            ]
        })
        .collect();
    let rgba = display_rgba
        .chunks_exact(4)
        .flat_map(|pixel| pixel.iter().copied().map(to_byte))
        .collect();

    Ok(DecodedImage {
        width,
        height,
        rgba,
        pixels,
        alpha,
        input_contract,
        has_transparency,
    })
}

#[derive(Debug)]
pub struct LoadRequest {
    pub generation: u64,
    pub operation: LoadOperation,
    pub cancellation: CancellationToken,
}

#[derive(Debug)]
pub enum LoadOperation {
    Decode(PathBuf),
    FlattenOntoBlack { path: PathBuf, image: DecodedImage },
}

#[derive(Debug)]
pub struct LoadResult {
    pub generation: u64,
    pub path: PathBuf,
    pub image: Result<DecodedImage, ImageIoError>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Thumbnail {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

#[derive(Debug)]
pub struct ThumbnailRequest {
    pub generation: u64,
    pub path: PathBuf,
    pub cancellation: CancellationToken,
}

#[derive(Debug)]
pub struct ThumbnailResult {
    pub generation: u64,
    pub path: PathBuf,
    pub image: Result<Thumbnail, ImageIoError>,
}

pub struct ExportRequest {
    pub generation: u64,
    pub path: PathBuf,
    pub source: Image,
    pub snapshot: PipelineSnapshot,
    pub cancellation: CancellationToken,
}

pub struct ExportResult {
    pub generation: u64,
    pub path: PathBuf,
    pub result: Result<(), String>,
}

pub fn spawn_exporter() -> (Sender<ExportRequest>, Receiver<ExportResult>) {
    let (request_sender, request_receiver) = mpsc::channel::<ExportRequest>();
    let (result_sender, result_receiver) = mpsc::channel();
    std::thread::Builder::new()
        .name("focal-editor-export".to_owned())
        .spawn(move || {
            while let Ok(request) = request_receiver.recv() {
                let generation = request.generation;
                let path = request.path;
                let cancellation = request.cancellation;
                let result = Pipeline::from_snapshot(request.snapshot)
                    .render_with_context(
                        request.source,
                        &RenderContext::with_cancellation(
                            RenderQuality::Export,
                            cancellation.clone(),
                        ),
                        &mut |_| {},
                    )
                    .map_err(|error| error.to_string())
                    .and_then(|(output, _)| {
                        if cancellation.is_cancelled() {
                            Err("export cancelled".to_owned())
                        } else {
                            encode_srgb_png(&path, &output)
                        }
                    });
                if result_sender
                    .send(ExportResult {
                        generation,
                        path,
                        result,
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .expect("export worker thread should start");
    (request_sender, result_receiver)
}

fn encode_srgb_png(path: &Path, output_image: &Image) -> Result<(), String> {
    let file = std::fs::File::create(path).map_err(|error| error.to_string())?;
    let mut encoder = image::codecs::png::PngEncoder::new(std::io::BufWriter::new(file));
    encoder
        .set_icc_profile(
            ColorProfile::new_srgb()
                .encode()
                .map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?;
    let rgba = output_image
        .pixels()
        .iter()
        .flat_map(|pixel| {
            pixel
                .iter()
                .copied()
                .map(to_byte)
                .chain(std::iter::once(u8::MAX))
        })
        .collect::<Vec<_>>();
    encoder
        .write_image(
            &rgba,
            output_image.width(),
            output_image.height(),
            ExtendedColorType::Rgba8,
        )
        .map_err(|error| error.to_string())
}

/// Starts the image decoder away from the egui thread.
pub fn spawn_loader() -> (Sender<LoadRequest>, Receiver<LoadResult>) {
    let (request_sender, request_receiver) = mpsc::channel::<LoadRequest>();
    let (result_sender, result_receiver) = mpsc::channel();
    std::thread::Builder::new()
        .name("focal-editor-loader".to_owned())
        .spawn(move || {
            while let Ok(request) = request_receiver.recv() {
                let result_sender = result_sender.clone();
                std::thread::spawn(move || {
                    let (path, image) = match request.operation {
                        LoadOperation::Decode(path) => {
                            let image = decode_with_cancellation(&path, &request.cancellation);
                            (path, image)
                        }
                        LoadOperation::FlattenOntoBlack { path, image } => (
                            path,
                            image.flatten_onto_black_with_cancellation(&request.cancellation),
                        ),
                    };
                    let result = LoadResult {
                        generation: request.generation,
                        path: path.clone(),
                        image,
                    };
                    let _ = result_sender.send(result);
                });
            }
        })
        .expect("image loader thread should start");
    (request_sender, result_receiver)
}

pub fn spawn_thumbnail_loader() -> (Sender<ThumbnailRequest>, Receiver<ThumbnailResult>) {
    let (request_sender, request_receiver) = mpsc::channel::<ThumbnailRequest>();
    let (result_sender, result_receiver) = mpsc::channel();
    std::thread::Builder::new()
        .name("focal-editor-thumbnails".to_owned())
        .spawn(move || {
            while let Ok(first) = request_receiver.recv() {
                for request in newest_thumbnail_batch(first, &request_receiver) {
                    let path = request.path;
                    let cancellation = request.cancellation;
                    let result = ThumbnailResult {
                        generation: request.generation,
                        path: path.clone(),
                        image: decode_thumbnail_with_cancellation(&path, 160, &cancellation),
                    };
                    if result_sender.send(result).is_err() {
                        return;
                    }
                }
            }
        })
        .expect("thumbnail loader thread should start");
    (request_sender, result_receiver)
}

fn newest_thumbnail_batch(
    first: ThumbnailRequest,
    receiver: &Receiver<ThumbnailRequest>,
) -> Vec<ThumbnailRequest> {
    let mut requests = vec![first];
    requests.extend(receiver.try_iter());
    let newest_generation = requests
        .iter()
        .map(|request| request.generation)
        .max()
        .unwrap_or(0);
    requests.retain(|request| request.generation == newest_generation);
    requests
}

#[cfg(test)]
fn decode_thumbnail(path: &Path, maximum_dimension: u32) -> Result<Thumbnail, ImageIoError> {
    decode_thumbnail_with_cancellation(path, maximum_dimension, &CancellationToken::new())
}

fn decode_thumbnail_with_cancellation(
    path: &Path,
    maximum_dimension: u32,
    cancellation: &CancellationToken,
) -> Result<Thumbnail, ImageIoError> {
    if is_raf_path(path) {
        let image = focal_io::decode_xt5_thumbnail(path, maximum_dimension, cancellation)
            .map_err(|error| ImageIoError::Raw(error.to_string()))?;
        return Ok(Thumbnail {
            width: image.width,
            height: image.height,
            rgba: image.rgba,
        });
    }
    let bytes = std::fs::read(path).map_err(|source| ImageIoError::Open {
        path: path.to_path_buf(),
        source,
    })?;
    ensure_not_cancelled(cancellation)?;
    let format = image::guess_format(&bytes).map_err(ImageIoError::Decode)?;
    let (orientation, icc) = match format {
        ImageFormat::Jpeg => {
            let mut decoder =
                JpegDecoder::new(Cursor::new(&bytes)).map_err(ImageIoError::Decode)?;
            (
                decoder.orientation().map_err(ImageIoError::Decode)?,
                decoder.icc_profile().map_err(ImageIoError::Decode)?,
            )
        }
        ImageFormat::Png => {
            let mut decoder = PngDecoder::new(Cursor::new(&bytes)).map_err(ImageIoError::Decode)?;
            (
                decoder.orientation().map_err(ImageIoError::Decode)?,
                decoder.icc_profile().map_err(ImageIoError::Decode)?,
            )
        }
        ImageFormat::Tiff => {
            let mut decoder =
                TiffDecoder::new(Cursor::new(&bytes)).map_err(ImageIoError::Decode)?;
            (
                decoder.orientation().map_err(ImageIoError::Decode)?,
                decoder.icc_profile().map_err(ImageIoError::Decode)?,
            )
        }
        _ => return Err(ImageIoError::UnsupportedFormat),
    };
    let mut image =
        image::load_from_memory_with_format(&bytes, format).map_err(ImageIoError::Decode)?;
    ensure_not_cancelled(cancellation)?;
    image.apply_orientation(orientation);
    let image = image
        .thumbnail(maximum_dimension, maximum_dimension)
        .to_rgba8();
    let width = image.width();
    let height = image.height();
    let rgba = display_rgba_from_source_pixels(&image.into_raw(), icc.as_deref())?;
    ensure_not_cancelled(cancellation)?;
    Ok(Thumbnail {
        width,
        height,
        rgba,
    })
}

fn srgb_to_linear(value: f32) -> f32 {
    if value <= 0.040_45 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

fn linear_to_srgb(value: f32) -> f32 {
    if value <= 0.003_130_8 {
        value * 12.92
    } else {
        1.055 * value.powf(1.0 / 2.4) - 0.055
    }
}

fn display_rgba_from_working_pixels(
    pixels: &[[f32; 3]],
    contract: ImageContract,
) -> Result<Vec<u8>, ImageIoError> {
    if contract == ImageContract::ADOBE_RGB_CURVE {
        let input = pixels
            .iter()
            .flat_map(|pixel| [pixel[0], pixel[1], pixel[2], 1.0])
            .collect::<Vec<_>>();
        let source = ColorProfile::new_adobe_rgb();
        let destination = ColorProfile::new_srgb();
        let executor = source
            .create_transform_f32(
                Layout::Rgba,
                &destination,
                Layout::Rgba,
                TransformOptions::default(),
            )
            .map_err(|error| ImageIoError::ColourProfile(error.to_string()))?;
        let mut output = vec![0.0; input.len()];
        executor
            .transform(&input, &mut output)
            .map_err(|error| ImageIoError::ColourProfile(error.to_string()))?;
        Ok(output
            .chunks_exact(4)
            .flat_map(|pixel| pixel.iter().copied().map(to_byte))
            .collect())
    } else {
        Ok(pixels
            .iter()
            .flat_map(|pixel| {
                pixel
                    .iter()
                    .copied()
                    .map(to_byte)
                    .chain(std::iter::once(u8::MAX))
            })
            .collect())
    }
}

fn display_rgba_from_source_pixels(
    rgba: &[u8],
    icc: Option<&[u8]>,
) -> Result<Vec<u8>, ImageIoError> {
    let Some(icc) = icc else {
        return Ok(rgba.to_vec());
    };
    let source = ColorProfile::new_from_slice(icc)
        .map_err(|error| ImageIoError::ColourProfile(error.to_string()))?;
    let destination = ColorProfile::new_srgb();
    let executor = source
        .create_transform_f32(
            Layout::Rgba,
            &destination,
            Layout::Rgba,
            TransformOptions::default(),
        )
        .map_err(|error| ImageIoError::ColourProfile(error.to_string()))?;
    let input = rgba
        .chunks_exact(4)
        .flat_map(|pixel| pixel.iter().map(|value| f32::from(*value) / 255.0))
        .collect::<Vec<_>>();
    let mut output = vec![0.0; input.len()];
    executor
        .transform(&input, &mut output)
        .map_err(|error| ImageIoError::ColourProfile(error.to_string()))?;
    Ok(output
        .chunks_exact(4)
        .flat_map(|pixel| pixel.iter().copied().map(to_byte))
        .collect())
}

fn decode_channel(value: f32, encoding: ColourEncoding) -> f32 {
    match encoding {
        ColourEncoding::Srgb => srgb_to_linear(value),
        ColourEncoding::AdobeRgb => value.max(0.0).powf(2.199_218_8),
        ColourEncoding::Linear => value,
    }
}

fn encode_channel(value: f32, encoding: ColourEncoding) -> f32 {
    match encoding {
        ColourEncoding::Srgb => linear_to_srgb(value),
        ColourEncoding::AdobeRgb => value.max(0.0).powf(1.0 / 2.199_218_8),
        ColourEncoding::Linear => value,
    }
}

fn to_byte(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

fn ensure_not_cancelled(cancellation: &CancellationToken) -> Result<(), ImageIoError> {
    if cancellation.is_cancelled() {
        Err(ImageIoError::Cancelled)
    } else {
        Ok(())
    }
}

#[derive(Debug)]
pub enum ImageIoError {
    Open {
        path: std::path::PathBuf,
        source: io::Error,
    },
    Decode(image::ImageError),
    Cancelled,
    UnsupportedFormat,
    Raw(String),
    ColourProfile(String),
}

impl fmt::Display for ImageIoError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Open { path, source } => {
                write!(formatter, "could not open {}: {source}", path.display())
            }
            Self::Decode(source) => write!(formatter, "could not decode image: {source}"),
            Self::Cancelled => formatter.write_str("image operation cancelled"),
            Self::UnsupportedFormat => {
                write!(
                    formatter,
                    "only PNG, JPEG, TIFF, and Fujifilm X-T5 RAF images are supported"
                )
            }
            Self::Raw(source) => write!(formatter, "could not develop RAW image: {source}"),
            Self::ColourProfile(source) => {
                write!(
                    formatter,
                    "could not interpret embedded ICC profile: {source}"
                )
            }
        }
    }
}

impl std::error::Error for ImageIoError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raf_extension_detection_is_case_insensitive() {
        assert!(is_raf_path(Path::new("capture.RAF")));
        assert!(is_raf_path(Path::new("capture.raf")));
        assert!(!is_raf_path(Path::new("capture.jpg")));
    }

    #[test]
    #[ignore = "uses the local 38 MP X-T5 reference fixture"]
    fn xt5_raf_opens_through_the_editor_decode_boundary() {
        let path = Path::new("../../test-image/X-T5_RAW/PROVIA_JPG.RAF");
        let image = decode_with_cancellation(path, &CancellationToken::new()).unwrap();
        assert_eq!((image.width, image.height), (7728, 5152));
        assert_eq!(image.input_contract, ImageContract::SRGB_DISPLAY);
        assert!(!image.has_transparency);
        assert_eq!(image.pixels.len(), 7728 * 5152);
        assert_eq!(image.rgba.len(), 7728 * 5152 * 4);
        let thumbnail = decode_thumbnail_with_cancellation(path, 160, &CancellationToken::new())
            .expect("the embedded RAF preview should provide a thumbnail");
        assert!(thumbnail.width <= 160 && thumbnail.height <= 160);
        assert_eq!(
            thumbnail.rgba.len(),
            usize::try_from(thumbnail.width * thumbnail.height * 4).unwrap()
        );
    }

    #[test]
    fn transparent_pixel_is_flattened_in_linear_light() {
        let source = DecodedImage {
            width: 1,
            height: 1,
            rgba: vec![255, 0, 0, 128],
            pixels: vec![[1.0, 0.0, 0.0]],
            alpha: vec![128.0 / 255.0],
            input_contract: ImageContract::SRGB_DISPLAY,
            has_transparency: true,
        };

        let flattened = source.flatten_onto_black().unwrap();
        assert!(!flattened.has_transparency);
        assert_eq!(flattened.rgba, vec![188, 0, 0, 255]);
        assert!((flattened.pixels[0][0] - 0.736_65).abs() < 0.002);
    }

    #[test]
    fn cancelled_image_boundaries_stop_before_decode_or_flattening() {
        let cancellation = CancellationToken::new();
        cancellation.cancel();
        assert!(matches!(
            decode_bytes_with_cancellation(&[], &cancellation),
            Err(ImageIoError::Cancelled)
        ));

        let source = DecodedImage {
            width: 1,
            height: 1,
            rgba: vec![255, 0, 0, 128],
            pixels: vec![[1.0, 0.0, 0.0]],
            alpha: vec![0.5],
            input_contract: ImageContract::SRGB_DISPLAY,
            has_transparency: true,
        };
        assert!(matches!(
            source.flatten_onto_black_with_cancellation(&cancellation),
            Err(ImageIoError::Cancelled)
        ));
    }

    #[test]
    fn opaque_pixel_keeps_srgb_values() {
        let source = DecodedImage {
            width: 1,
            height: 1,
            rgba: vec![128, 64, 32, 255],
            pixels: vec![[128.0 / 255.0, 64.0 / 255.0, 32.0 / 255.0]],
            alpha: vec![1.0],
            input_contract: ImageContract::SRGB_DISPLAY,
            has_transparency: false,
        };

        let image = source.to_core_image().unwrap();
        assert_eq!(image.contract(), ImageContract::SRGB_DISPLAY);
        assert_eq!(
            image.pixels(),
            &[[128.0 / 255.0, 64.0 / 255.0, 32.0 / 255.0]]
        );
    }

    #[test]
    fn sixteen_bit_samples_keep_more_than_eight_bit_processing_precision() {
        let image = image::RgbImage::from_raw(2, 1, vec![128, 0, 0, 128, 0, 0]).unwrap();
        let eight_bit = decoded_image_from_dynamic(&image::DynamicImage::ImageRgb8(image));
        let image = image::ImageBuffer::<image::Rgb<u16>, _>::from_raw(
            2,
            1,
            vec![32_768, 0, 0, 32_769, 0, 0],
        )
        .unwrap();
        let sixteen_bit = decoded_image_from_dynamic(&image::DynamicImage::ImageRgb16(image));

        assert!((eight_bit.pixels[0][0] - eight_bit.pixels[1][0]).abs() < f32::EPSILON);
        assert!((sixteen_bit.pixels[0][0] - sixteen_bit.pixels[1][0]).abs() > f32::EPSILON);
        assert_eq!(&sixteen_bit.rgba[..4], &sixteen_bit.rgba[4..]);
    }

    #[test]
    fn embedded_adobe_rgb_profile_enters_core_with_an_adobe_contract() {
        let image = image::DynamicImage::ImageRgb8(
            image::RgbImage::from_raw(1, 1, vec![128, 64, 32]).unwrap(),
        );
        let profile = ColorProfile::new_adobe_rgb().encode().unwrap();
        let decoded = decoded_image_from_dynamic_with_profile(&image, Some(&profile)).unwrap();

        assert_eq!(decoded.input_contract, ImageContract::ADOBE_RGB_CURVE);
        assert_eq!(
            decoded.to_core_image().unwrap().contract(),
            ImageContract::ADOBE_RGB_CURVE
        );
    }

    #[test]
    fn flattening_an_adobe_image_keeps_display_rgba_in_srgb() {
        let image = image::DynamicImage::ImageRgba8(
            image::RgbaImage::from_raw(1, 1, vec![128, 64, 32, 128]).unwrap(),
        );
        let profile = ColorProfile::new_adobe_rgb().encode().unwrap();
        let flattened = decoded_image_from_dynamic_with_profile(&image, Some(&profile))
            .unwrap()
            .flatten_onto_black()
            .unwrap();

        let source = ColorProfile::new_adobe_rgb();
        let destination = ColorProfile::new_srgb();
        let executor = source
            .create_transform_f32(
                Layout::Rgba,
                &destination,
                Layout::Rgba,
                TransformOptions::default(),
            )
            .unwrap();
        let input = [
            flattened.pixels[0][0],
            flattened.pixels[0][1],
            flattened.pixels[0][2],
            1.0,
        ];
        let mut output = [0.0; 4];
        executor.transform(&input, &mut output).unwrap();
        let expected = output.map(to_byte);

        assert_eq!(flattened.rgba, expected.to_vec());
    }

    #[test]
    fn sixteen_bit_alpha_is_not_quantised_before_transparency_detection() {
        let image = image::ImageBuffer::<image::Rgba<u16>, _>::from_raw(
            1,
            1,
            vec![u16::MAX, 0, 0, u16::MAX - 1],
        )
        .unwrap();
        let decoded = decoded_image_from_dynamic(&image::DynamicImage::ImageRgba16(image));

        assert!(decoded.has_transparency);
        assert!(decoded.alpha[0] < 1.0);
    }

    #[test]
    fn transfer_helpers_cover_encoding_boundaries_and_clamping() {
        for value in [0.0, 0.001, 0.040_45, 0.18, 1.0] {
            assert!((srgb_to_linear(linear_to_srgb(value)) - value).abs() < 1.0e-5);
        }
        for encoding in [
            ColourEncoding::Srgb,
            ColourEncoding::AdobeRgb,
            ColourEncoding::Linear,
        ] {
            let decoded = decode_channel(0.5, encoding);
            let encoded = encode_channel(decoded, encoding);
            assert!((encoded - 0.5).abs() < 1.0e-5, "encoding={encoding:?}");
        }
        assert!(decode_channel(-1.0, ColourEncoding::AdobeRgb).abs() < f32::EPSILON);
        assert!(encode_channel(-1.0, ColourEncoding::AdobeRgb).abs() < f32::EPSILON);
        assert_eq!(to_byte(-1.0), 0);
        assert_eq!(to_byte(0.0), 0);
        assert_eq!(to_byte(1.0), 255);
        assert_eq!(to_byte(2.0), 255);
        assert_eq!(to_byte(0.5), 128);
    }

    #[test]
    fn display_conversion_preserves_alpha_and_supports_profiled_and_unprofiled_pixels() {
        let pixels = [[0.0, 0.5, 1.0]];
        assert_eq!(
            display_rgba_from_working_pixels(&pixels, ImageContract::SRGB_DISPLAY).unwrap(),
            vec![0, 128, 255, 255]
        );
        let profiled =
            display_rgba_from_working_pixels(&pixels, ImageContract::ADOBE_RGB_CURVE).unwrap();
        assert_eq!(profiled.len(), 4);
        assert_eq!(profiled[3], 255);

        let rgba = [1, 2, 3, 4];
        assert_eq!(display_rgba_from_source_pixels(&rgba, None).unwrap(), rgba);
        assert!(matches!(
            display_rgba_from_source_pixels(&rgba, Some(&[0, 1, 2])),
            Err(ImageIoError::ColourProfile(_))
        ));
    }

    #[test]
    fn tiff_decode_enters_the_same_explicit_srgb_boundary_as_png_and_jpeg() {
        let bytes = {
            let mut cursor = Cursor::new(Vec::new());
            let encoder = image::codecs::tiff::TiffEncoder::new(&mut cursor);
            encoder
                .write_image(&[255, 0, 0, 0, 255, 0], 2, 1, ExtendedColorType::Rgb8)
                .unwrap();
            cursor.into_inner()
        };

        let decoded = decode_bytes(&bytes).unwrap();
        assert_eq!((decoded.width, decoded.height), (2, 1));
        assert_eq!(decoded.input_contract, ImageContract::SRGB_DISPLAY);
        assert_eq!(decoded.pixels.len(), 2);
        assert_eq!(decoded.rgba.len(), 8);
    }

    #[test]
    fn tiff_decode_preserves_sixteen_bit_precision_and_transparency() {
        let bytes = {
            let mut cursor = Cursor::new(Vec::new());
            let encoder = image::codecs::tiff::TiffEncoder::new(&mut cursor);
            let samples = [32_768_u16, 0, 0, 32_768, 32_769, 0, 0, 65_535];
            let bytes = samples
                .into_iter()
                .flat_map(u16::to_ne_bytes)
                .collect::<Vec<_>>();
            encoder
                .write_image(&bytes, 2, 1, ExtendedColorType::Rgba16)
                .unwrap();
            cursor.into_inner()
        };

        let decoded = decode_bytes(&bytes).unwrap();
        assert_eq!((decoded.width, decoded.height), (2, 1));
        assert!(decoded.has_transparency);
        assert!(decoded.alpha[0] < 1.0);
        assert!(decoded.pixels[0][0] < decoded.pixels[1][0]);
    }

    #[test]
    fn tiff_embedded_icc_profile_enters_the_canonical_adobe_boundary() {
        let bytes = {
            let mut cursor = Cursor::new(Vec::new());
            let mut encoder = image::codecs::tiff::TiffEncoder::new(&mut cursor);
            encoder
                .set_icc_profile(ColorProfile::new_adobe_rgb().encode().unwrap())
                .unwrap();
            encoder
                .write_image(&[128, 64, 32], 1, 1, ExtendedColorType::Rgb8)
                .unwrap();
            cursor.into_inner()
        };

        let decoded = decode_bytes(&bytes).unwrap();
        assert_eq!(decoded.input_contract, ImageContract::ADOBE_RGB_CURVE);
        assert_eq!(decoded.pixels.len(), 1);
    }

    #[test]
    fn tiff_orientation_is_applied_exactly_once_at_decode() {
        // A minimal little-endian 2x1 RGB TIFF with Orientation=6. The
        // decoder must expose the oriented 1x2 image to the rest of the app.
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"II");
        bytes.extend_from_slice(&42_u16.to_le_bytes());
        bytes.extend_from_slice(&8_u32.to_le_bytes());
        bytes.extend_from_slice(&11_u16.to_le_bytes());
        {
            let mut entry = |tag: u16, kind: u16, count: u32, value: u32| {
                bytes.extend_from_slice(&tag.to_le_bytes());
                bytes.extend_from_slice(&kind.to_le_bytes());
                bytes.extend_from_slice(&count.to_le_bytes());
                bytes.extend_from_slice(&value.to_le_bytes());
            };
            entry(256, 4, 1, 2);
            entry(257, 4, 1, 1);
            entry(258, 3, 3, 146);
            entry(259, 3, 1, 1);
            entry(262, 3, 1, 2);
            entry(273, 4, 1, 152);
            entry(274, 3, 1, 6);
            entry(277, 3, 1, 3);
            entry(278, 4, 1, 1);
            entry(279, 4, 1, 6);
            entry(284, 3, 1, 1);
        }
        bytes.extend_from_slice(&0_u32.to_le_bytes());
        bytes.extend_from_slice(&[8, 0, 8, 0, 8, 0]);
        bytes.extend_from_slice(&[255, 0, 0, 0, 0, 255]);

        let decoded = decode_bytes(&bytes).unwrap();
        assert_eq!((decoded.width, decoded.height), (1, 2));
        assert_eq!(decoded.pixels.len(), 2);
    }

    #[test]
    fn thumbnail_worker_drops_queued_requests_from_obsolete_directories() {
        let (sender, receiver) = mpsc::channel();
        sender
            .send(ThumbnailRequest {
                generation: 1,
                path: PathBuf::from("old-b.jpg"),
                cancellation: CancellationToken::new(),
            })
            .unwrap();
        sender
            .send(ThumbnailRequest {
                generation: 2,
                path: PathBuf::from("new-a.jpg"),
                cancellation: CancellationToken::new(),
            })
            .unwrap();
        sender
            .send(ThumbnailRequest {
                generation: 2,
                path: PathBuf::from("new-b.jpg"),
                cancellation: CancellationToken::new(),
            })
            .unwrap();

        let batch = newest_thumbnail_batch(
            ThumbnailRequest {
                generation: 1,
                path: PathBuf::from("old-a.jpg"),
                cancellation: CancellationToken::new(),
            },
            &receiver,
        );

        assert_eq!(batch.len(), 2);
        assert!(batch.iter().all(|request| request.generation == 2));
    }

    #[test]
    fn png_export_embeds_an_srgb_icc_profile() {
        let path = std::env::temp_dir().join(format!(
            "focal-editor-profiled-export-{}.png",
            std::process::id()
        ));
        let image = Image::new(1, 1, vec![[0.25, 0.5, 0.75]], ImageContract::SRGB_DISPLAY).unwrap();
        encode_srgb_png(&path, &image).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        let mut decoder = PngDecoder::new(Cursor::new(bytes)).unwrap();
        let profile = decoder.icc_profile().unwrap().unwrap();
        assert!(ColorProfile::new_from_slice(&profile).is_ok());
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn png_orientation_is_applied_exactly_once_at_decode() {
        let pixels = [255_u8, 0, 0, 0, 0, 255];
        let tiff = [
            b'I', b'I', 42, 0, 8, 0, 0, 0, 1, 0, 0x12, 0x01, 3, 0, 1, 0, 0, 0, 6, 0, 0, 0, 0, 0, 0,
            0,
        ];
        let mut png = Vec::new();
        let mut encoder = image::codecs::png::PngEncoder::new(&mut png);
        encoder.set_exif_metadata(tiff.to_vec()).unwrap();
        encoder
            .write_image(&pixels, 2, 1, ExtendedColorType::Rgb8)
            .unwrap();

        let decoded = decode_bytes(&png).unwrap();
        assert_eq!((decoded.width, decoded.height), (1, 2));
    }

    #[test]
    fn thumbnail_decode_applies_png_orientation() {
        let pixels = [255_u8, 0, 0, 0, 0, 255];
        let tiff = [
            b'I', b'I', 42, 0, 8, 0, 0, 0, 1, 0, 0x12, 0x01, 3, 0, 1, 0, 0, 0, 6, 0, 0, 0, 0, 0, 0,
            0,
        ];
        let mut png = Vec::new();
        let mut encoder = image::codecs::png::PngEncoder::new(&mut png);
        encoder.set_exif_metadata(tiff.to_vec()).unwrap();
        encoder
            .write_image(&pixels, 2, 1, ExtendedColorType::Rgb8)
            .unwrap();
        let path = std::env::temp_dir().join(format!(
            "focal-editor-thumbnail-orientation-{}.png",
            std::process::id()
        ));
        std::fs::write(&path, png).unwrap();

        let thumbnail = decode_thumbnail(&path, 2).unwrap();
        assert_eq!((thumbnail.width, thumbnail.height), (1, 2));
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn thumbnail_decode_converts_embedded_icc_to_display_srgb() {
        let profile = ColorProfile::new_adobe_rgb().encode().unwrap();
        let mut png = Vec::new();
        let mut encoder = image::codecs::png::PngEncoder::new(&mut png);
        encoder.set_icc_profile(profile).unwrap();
        encoder
            .write_image(&[128, 64, 32, 255], 1, 1, ExtendedColorType::Rgba8)
            .unwrap();
        let expected = decode_bytes(&png).unwrap();
        let path = std::env::temp_dir().join(format!(
            "focal-editor-thumbnail-profile-{}.png",
            std::process::id()
        ));
        std::fs::write(&path, png).unwrap();

        let thumbnail = decode_thumbnail(&path, 1).unwrap();
        assert_eq!(thumbnail.rgba, expected.rgba);
        std::fs::remove_file(path).unwrap();
    }
}
