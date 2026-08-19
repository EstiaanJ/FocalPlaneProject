use std::fmt;

use serde::{Deserialize, Serialize};

/// Meaning of each three-component pixel.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChannelMeaning {
    Rgb,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ColourEncoding {
    /// The standard sRGB piecewise transfer function.
    Srgb,
    /// Scene-linear light. Values are not restricted to display range.
    Linear,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Primaries {
    Srgb,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum WhitePoint {
    D65,
}

/// Pixel semantics carried with every image rather than inferred from `f32`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImageContract {
    pub channels: ChannelMeaning,
    pub encoding: ColourEncoding,
    pub primaries: Primaries,
    pub white_point: WhitePoint,
}

impl ImageContract {
    pub const SRGB_DISPLAY: Self = Self {
        channels: ChannelMeaning::Rgb,
        encoding: ColourEncoding::Srgb,
        primaries: Primaries::Srgb,
        white_point: WhitePoint::D65,
    };

    pub const LINEAR_SRGB: Self = Self {
        encoding: ColourEncoding::Linear,
        ..Self::SRGB_DISPLAY
    };
}

#[derive(Clone, Debug, PartialEq)]
pub struct Image {
    width: u32,
    height: u32,
    pixels: Vec<[f32; 3]>,
    contract: ImageContract,
}

impl Image {
    /// Constructs an image after checking its structural and numeric invariants.
    ///
    /// # Errors
    ///
    /// Returns [`ImageError`] when the dimensions overflow, the pixel count
    /// does not match the dimensions, or any channel is non-finite.
    pub fn new(
        width: u32,
        height: u32,
        pixels: Vec<[f32; 3]>,
        contract: ImageContract,
    ) -> Result<Self, ImageError> {
        let expected = usize::try_from(width)
            .ok()
            .and_then(|width| {
                usize::try_from(height)
                    .ok()
                    .and_then(|height| width.checked_mul(height))
            })
            .ok_or(ImageError::DimensionsOverflow)?;
        if expected != pixels.len() {
            return Err(ImageError::PixelCount {
                expected,
                actual: pixels.len(),
            });
        }
        if pixels.iter().flatten().any(|value| !value.is_finite()) {
            return Err(ImageError::NonFinitePixel);
        }
        Ok(Self {
            width,
            height,
            pixels,
            contract,
        })
    }

    #[must_use]
    pub const fn width(&self) -> u32 {
        self.width
    }

    #[must_use]
    pub const fn height(&self) -> u32 {
        self.height
    }

    #[must_use]
    pub fn pixels(&self) -> &[[f32; 3]] {
        &self.pixels
    }

    pub(crate) fn pixels_mut(&mut self) -> &mut [[f32; 3]] {
        &mut self.pixels
    }

    #[must_use]
    pub const fn contract(&self) -> ImageContract {
        self.contract
    }

    pub(crate) fn set_contract(&mut self, contract: ImageContract) {
        self.contract = contract;
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImageError {
    DimensionsOverflow,
    PixelCount { expected: usize, actual: usize },
    NonFinitePixel,
}

impl fmt::Display for ImageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DimensionsOverflow => {
                write!(formatter, "image dimensions overflow addressable memory")
            }
            Self::PixelCount { expected, actual } => {
                write!(formatter, "expected {expected} pixels, received {actual}")
            }
            Self::NonFinitePixel => write!(formatter, "image contains a non-finite channel value"),
        }
    }
}

impl std::error::Error for ImageError {}
