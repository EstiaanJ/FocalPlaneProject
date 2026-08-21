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
    /// The Adobe RGB (1998) gamma encoding used by the bounded MVP curve.
    AdobeRgb,
    /// Scene-linear light. Values are not restricted to display range.
    Linear,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Primaries {
    Srgb,
    AdobeRgb,
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

    pub const ADOBE_RGB_CURVE: Self = Self {
        channels: ChannelMeaning::Rgb,
        encoding: ColourEncoding::AdobeRgb,
        primaries: Primaries::AdobeRgb,
        white_point: WhitePoint::D65,
    };

    pub const LINEAR_ADOBE_RGB: Self = Self {
        encoding: ColourEncoding::Linear,
        ..Self::ADOBE_RGB_CURVE
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
    /// does not match the dimensions, any dimension is zero, or any channel
    /// is non-finite.
    pub fn new(
        width: u32,
        height: u32,
        pixels: Vec<[f32; 3]>,
        contract: ImageContract,
    ) -> Result<Self, ImageError> {
        if width == 0 || height == 0 {
            return Err(ImageError::ZeroDimension);
        }
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
        if contract.encoding != ColourEncoding::Linear
            && pixels
                .iter()
                .flatten()
                .any(|value| !(0.0..=1.0).contains(value))
        {
            return Err(ImageError::OutOfRangeEncodedPixel);
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
    ZeroDimension,
    PixelCount { expected: usize, actual: usize },
    NonFinitePixel,
    OutOfRangeEncodedPixel,
}

impl fmt::Display for ImageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DimensionsOverflow => {
                write!(formatter, "image dimensions overflow addressable memory")
            }
            Self::ZeroDimension => write!(formatter, "image dimensions must be non-zero"),
            Self::PixelCount { expected, actual } => {
                write!(formatter, "expected {expected} pixels, received {actual}")
            }
            Self::NonFinitePixel => write!(formatter, "image contains a non-finite channel value"),
            Self::OutOfRangeEncodedPixel => {
                write!(
                    formatter,
                    "encoded image channels must be between zero and one"
                )
            }
        }
    }
}

impl std::error::Error for ImageError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_dimensions_are_rejected() {
        assert_eq!(
            Image::new(0, 1, Vec::new(), ImageContract::SRGB_DISPLAY).unwrap_err(),
            ImageError::ZeroDimension
        );
        assert_eq!(
            Image::new(1, 0, Vec::new(), ImageContract::SRGB_DISPLAY).unwrap_err(),
            ImageError::ZeroDimension
        );
    }

    #[test]
    fn structural_and_numeric_image_errors_remain_distinct() {
        assert_eq!(
            Image::new(2, 1, vec![[0.0; 3]], ImageContract::SRGB_DISPLAY).unwrap_err(),
            ImageError::PixelCount {
                expected: 2,
                actual: 1,
            }
        );
        assert_eq!(
            Image::new(1, 1, vec![[f32::NAN; 3]], ImageContract::SRGB_DISPLAY).unwrap_err(),
            ImageError::NonFinitePixel
        );
    }

    #[test]
    fn encoded_contracts_are_bounded_but_linear_contracts_are_not() {
        assert!(Image::new(1, 1, vec![[0.0; 3]], ImageContract::SRGB_DISPLAY).is_ok());
        assert!(Image::new(1, 1, vec![[1.0; 3]], ImageContract::SRGB_DISPLAY).is_ok());
        assert_eq!(
            Image::new(1, 1, vec![[1.01, 0.0, 0.0]], ImageContract::SRGB_DISPLAY,).unwrap_err(),
            ImageError::OutOfRangeEncodedPixel
        );
        assert_eq!(
            Image::new(1, 1, vec![[-0.01, 0.0, 0.0]], ImageContract::SRGB_DISPLAY).unwrap_err(),
            ImageError::OutOfRangeEncodedPixel
        );
        assert!(
            Image::new(
                1,
                1,
                vec![[1.01, -0.01, 0.0]],
                ImageContract::LINEAR_ADOBE_RGB,
            )
            .is_ok()
        );
    }
}
