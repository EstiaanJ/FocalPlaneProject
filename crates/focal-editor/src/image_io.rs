#![allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]

use std::{
    fmt,
    io::{self, Cursor},
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver, Sender},
};

use focal_core::{
    ColourEncoding, Image, ImageContract, ImageError, Pipeline, PipelineSnapshot, RenderContext,
    RenderQuality,
};
use image::{
    ExtendedColorType, ImageDecoder, ImageEncoder, ImageFormat, ImageReader,
    codecs::{jpeg::JpegDecoder, png::PngDecoder},
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
    #[must_use]
    pub fn flatten_onto_black(mut self) -> Self {
        for ((pixel, alpha), rgba) in self
            .pixels
            .iter_mut()
            .zip(self.alpha.iter().copied())
            .zip(self.rgba.chunks_exact_mut(4))
        {
            for (channel, byte) in pixel.iter_mut().zip(&mut rgba[..3]) {
                let linear = decode_channel(*channel, self.input_contract.encoding) * alpha;
                *channel = encode_channel(linear, self.input_contract.encoding);
                *byte = to_byte(*channel);
            }
            rgba[3] = u8::MAX;
        }
        self.alpha.fill(1.0);
        self.has_transparency = false;
        self
    }
}

pub fn decode(path: &Path) -> Result<DecodedImage, ImageIoError> {
    let bytes = std::fs::read(path).map_err(|source| ImageIoError::Open {
        path: path.to_path_buf(),
        source,
    })?;
    decode_bytes(&bytes)
}

#[cfg(test)]
fn decoded_image_from_dynamic(image: &image::DynamicImage) -> DecodedImage {
    decoded_image_from_dynamic_with_profile(image, None)
        .expect("the built-in sRGB interpretation is valid")
}

fn decode_bytes(bytes: &[u8]) -> Result<DecodedImage, ImageIoError> {
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
        _ => return Err(ImageIoError::UnsupportedFormat),
    };
    let mut image =
        image::load_from_memory_with_format(bytes, format).map_err(ImageIoError::Decode)?;
    image.apply_orientation(orientation);
    decoded_image_from_dynamic_with_profile(&image, icc.as_deref())
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
}

#[derive(Debug)]
pub struct ThumbnailResult {
    pub generation: u64,
    pub path: PathBuf,
    pub image: Result<Thumbnail, ImageIoError>,
}

pub struct ExportRequest {
    pub path: PathBuf,
    pub source: Image,
    pub snapshot: PipelineSnapshot,
}

pub struct ExportResult {
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
                let path = request.path;
                let result = Pipeline::from_snapshot(request.snapshot)
                    .render_with_context(
                        request.source,
                        &RenderContext::new(RenderQuality::Export),
                        &mut |_| {},
                    )
                    .map_err(|error| error.to_string())
                    .and_then(|(output, _)| encode_srgb_png(&path, &output));
                if result_sender.send(ExportResult { path, result }).is_err() {
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
                            let image = decode(&path);
                            (path, image)
                        }
                        LoadOperation::FlattenOntoBlack { path, image } => {
                            (path, Ok(image.flatten_onto_black()))
                        }
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
                    let result = ThumbnailResult {
                        generation: request.generation,
                        path: path.clone(),
                        image: decode_thumbnail(&path, 160),
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

pub fn decode_thumbnail(path: &Path, maximum_dimension: u32) -> Result<Thumbnail, ImageIoError> {
    let image = ImageReader::open(path)
        .map_err(|source| ImageIoError::Open {
            path: path.to_path_buf(),
            source,
        })?
        .decode()
        .map_err(ImageIoError::Decode)?
        .thumbnail(maximum_dimension, maximum_dimension)
        .to_rgba8();
    Ok(Thumbnail {
        width: image.width(),
        height: image.height(),
        rgba: image.into_raw(),
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

#[derive(Debug)]
pub enum ImageIoError {
    Open {
        path: std::path::PathBuf,
        source: io::Error,
    },
    Decode(image::ImageError),
    UnsupportedFormat,
    ColourProfile(String),
}

impl fmt::Display for ImageIoError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Open { path, source } => {
                write!(formatter, "could not open {}: {source}", path.display())
            }
            Self::Decode(source) => write!(formatter, "could not decode image: {source}"),
            Self::UnsupportedFormat => write!(formatter, "only PNG and JPEG images are supported"),
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

        let flattened = source.flatten_onto_black();
        assert!(!flattened.has_transparency);
        assert_eq!(flattened.rgba, vec![188, 0, 0, 255]);
        assert!((flattened.pixels[0][0] - 0.736_65).abs() < 0.002);
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
    fn thumbnail_worker_drops_queued_requests_from_obsolete_directories() {
        let (sender, receiver) = mpsc::channel();
        sender
            .send(ThumbnailRequest {
                generation: 1,
                path: PathBuf::from("old-b.jpg"),
            })
            .unwrap();
        sender
            .send(ThumbnailRequest {
                generation: 2,
                path: PathBuf::from("new-a.jpg"),
            })
            .unwrap();
        sender
            .send(ThumbnailRequest {
                generation: 2,
                path: PathBuf::from("new-b.jpg"),
            })
            .unwrap();

        let batch = newest_thumbnail_batch(
            ThumbnailRequest {
                generation: 1,
                path: PathBuf::from("old-a.jpg"),
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
}
