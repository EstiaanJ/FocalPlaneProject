#![allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]

use std::{
    fmt, io,
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver, Sender},
};

use focal_core::{Image, ImageContract, ImageError};
use image::ImageReader;

/// A decoded, displayable source image owned by the editor.
#[derive(Clone, Debug, PartialEq)]
pub struct DecodedImage {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    pub pixels: Vec<[f32; 3]>,
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
            ImageContract::SRGB_DISPLAY,
        )
    }

    /// Flattens transparency onto black in linear light.
    #[must_use]
    pub fn flatten_onto_black(mut self) -> Self {
        for (pixel, rgba) in self.pixels.iter_mut().zip(self.rgba.chunks_exact_mut(4)) {
            let alpha = f32::from(rgba[3]) / 255.0;
            for (channel, byte) in pixel.iter_mut().zip(&mut rgba[..3]) {
                let linear = srgb_to_linear(*channel) * alpha;
                *channel = linear_to_srgb(linear);
                *byte = to_byte(*channel);
            }
            rgba[3] = u8::MAX;
        }
        self.has_transparency = false;
        self
    }
}

pub fn decode(path: &Path) -> Result<DecodedImage, ImageIoError> {
    let image = ImageReader::open(path)
        .map_err(|source| ImageIoError::Open {
            path: path.to_path_buf(),
            source,
        })?
        .decode()
        .map_err(ImageIoError::Decode)?
        .to_rgba8();

    let width = image.width();
    let height = image.height();
    let rgba = image.into_raw();
    let has_transparency = rgba.chunks_exact(4).any(|pixel| pixel[3] != u8::MAX);
    let pixels = rgba
        .chunks_exact(4)
        .map(|pixel| {
            [
                f32::from(pixel[0]) / 255.0,
                f32::from(pixel[1]) / 255.0,
                f32::from(pixel[2]) / 255.0,
            ]
        })
        .collect();

    Ok(DecodedImage {
        width,
        height,
        rgba,
        pixels,
        has_transparency,
    })
}

#[derive(Debug)]
pub struct LoadRequest {
    pub generation: u64,
    pub path: PathBuf,
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

/// Starts the image decoder away from the egui thread.
pub fn spawn_loader() -> (Sender<LoadRequest>, Receiver<LoadResult>) {
    let (request_sender, request_receiver) = mpsc::channel::<LoadRequest>();
    let (result_sender, result_receiver) = mpsc::channel();
    std::thread::Builder::new()
        .name("focal-editor-loader".to_owned())
        .spawn(move || {
            while let Ok(mut request) = request_receiver.recv() {
                while let Ok(newer) = request_receiver.try_recv() {
                    request = newer;
                }
                let path = request.path;
                let result = LoadResult {
                    generation: request.generation,
                    path: path.clone(),
                    image: decode(&path),
                };
                if result_sender.send(result).is_err() {
                    break;
                }
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
}

impl fmt::Display for ImageIoError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Open { path, source } => {
                write!(formatter, "could not open {}: {source}", path.display())
            }
            Self::Decode(source) => write!(formatter, "could not decode image: {source}"),
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
}
