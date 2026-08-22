use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::{
    CancellationToken, CurveMode, CurveSet, Image, ImageContract, WorkingSpace,
    pipeline::RenderImplementation, processing,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModuleKind {
    InputTransform,
    OrientationAndCrop,
    WhiteBalance,
    Exposure,
    HighlightsAndShadows,
    Contrast,
    TonalCurve,
    LocalContrast,
    Saturation,
    CreativeColour,
    NoiseReduction,
    Sharpening,
    Resize,
    OutputTransform,
    QuantisationAndDither,
}

/// A non-destructive, axis-aligned crop in the straightened image canvas.
/// Coordinates are normalised to the uncropped source dimensions.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct CropSettings {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub rotation_degrees: f32,
}

impl CropSettings {
    #[must_use]
    pub const fn full_image() -> Self {
        Self {
            left: 0.0,
            top: 0.0,
            right: 1.0,
            bottom: 1.0,
            rotation_degrees: 0.0,
        }
    }

    /// Returns the largest centred crop with the source aspect ratio which
    /// remains wholly inside the source after straightening.
    #[must_use]
    pub fn largest_safe_straightened(aspect: f32, rotation_degrees: f32) -> Self {
        let angle = rotation_degrees.to_radians().abs();
        let (sin, cos) = angle.sin_cos();
        let horizontal_scale = aspect / (cos * aspect + sin);
        let vertical_scale = 1.0 / (sin * aspect + cos);
        let scale = horizontal_scale.min(vertical_scale).clamp(0.0, 1.0);
        Self {
            left: (1.0 - scale) * 0.5,
            top: (1.0 - scale) * 0.5,
            right: (1.0 + scale) * 0.5,
            bottom: (1.0 + scale) * 0.5,
            rotation_degrees,
        }
    }

    #[must_use]
    pub fn is_safe_for_aspect(self, aspect: f32) -> bool {
        validate_crop(self).is_ok()
            && aspect.is_finite()
            && aspect > 0.0
            && crop_fits_rotated_source(self, aspect)
    }

    /// Uniformly shrinks the crop around its centre until its rotated corners
    /// fit the source, preserving both its centre and aspect ratio.
    #[must_use]
    pub fn shrink_to_safe(self, aspect: f32) -> Self {
        if self.is_safe_for_aspect(aspect) {
            return self;
        }
        let centre_x = (self.left + self.right) * 0.5;
        let centre_y = (self.top + self.bottom) * 0.5;
        let width = self.right - self.left;
        let height = self.bottom - self.top;
        let mut low = 0.0;
        let mut high = 1.0;
        let scaled = |scale: f32| Self {
            left: centre_x - width * scale * 0.5,
            top: centre_y - height * scale * 0.5,
            right: centre_x + width * scale * 0.5,
            bottom: centre_y + height * scale * 0.5,
            rotation_degrees: self.rotation_degrees,
        };
        for _ in 0..24 {
            let middle = (low + high) * 0.5;
            if scaled(middle).is_safe_for_aspect(aspect) {
                low = middle;
            } else {
                high = middle;
            }
        }
        scaled(low)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ModuleParameters {
    InputTransform,
    OrientationAndCrop { crop: Option<CropSettings> },
    WhiteBalance { warmth: f32, tint: f32 },
    Exposure { stops: f32 },
    HighlightsAndShadows,
    Contrast { amount: f32 },
    TonalCurve { curves: CurveSet, mode: CurveMode },
    LocalContrast { amount: f32, radius: f32 },
    Saturation { amount: f32 },
    CreativeColour,
    NoiseReduction { luminance: f32, colour: f32 },
    Sharpening,
    Resize,
    OutputTransform,
    QuantisationAndDither,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Module {
    pub enabled: bool,
    pub parameters: ModuleParameters,
}

impl Module {
    pub(crate) fn apply_with_implementation(
        &self,
        image: &mut Image,
        working_space: WorkingSpace,
        cancellation: &CancellationToken,
        implementation: RenderImplementation,
    ) -> Result<(), ()> {
        match implementation {
            RenderImplementation::Reference => self.apply(image, working_space, cancellation),
            RenderImplementation::OptimizedCpu => {
                self.apply_optimized(image, working_space, cancellation)
            }
        }
    }

    #[must_use]
    pub const fn kind(&self) -> ModuleKind {
        match self.parameters {
            ModuleParameters::InputTransform => ModuleKind::InputTransform,
            ModuleParameters::OrientationAndCrop { .. } => ModuleKind::OrientationAndCrop,
            ModuleParameters::WhiteBalance { .. } => ModuleKind::WhiteBalance,
            ModuleParameters::Exposure { .. } => ModuleKind::Exposure,
            ModuleParameters::HighlightsAndShadows => ModuleKind::HighlightsAndShadows,
            ModuleParameters::Contrast { .. } => ModuleKind::Contrast,
            ModuleParameters::TonalCurve { .. } => ModuleKind::TonalCurve,
            ModuleParameters::LocalContrast { .. } => ModuleKind::LocalContrast,
            ModuleParameters::Saturation { .. } => ModuleKind::Saturation,
            ModuleParameters::CreativeColour => ModuleKind::CreativeColour,
            ModuleParameters::NoiseReduction { .. } => ModuleKind::NoiseReduction,
            ModuleParameters::Sharpening => ModuleKind::Sharpening,
            ModuleParameters::Resize => ModuleKind::Resize,
            ModuleParameters::OutputTransform => ModuleKind::OutputTransform,
            ModuleParameters::QuantisationAndDither => ModuleKind::QuantisationAndDither,
        }
    }

    pub(crate) fn apply(
        &self,
        image: &mut Image,
        working_space: WorkingSpace,
        cancellation: &CancellationToken,
    ) -> Result<(), ()> {
        if !self.enabled {
            return Ok(());
        }
        match self.parameters {
            ModuleParameters::InputTransform => {
                let source_contract = image.contract();
                for pixel in image.pixels_mut().iter_mut() {
                    if cancellation.is_cancelled() {
                        return Err(());
                    }
                    *pixel = if source_contract == ImageContract::SRGB_DISPLAY {
                        linear_srgb_to_adobe_rgb(pixel.map(srgb_to_linear))
                    } else {
                        pixel.map(adobe_rgb_to_linear)
                    };
                }
                image.set_contract(working_space.image_contract());
            }
            ModuleParameters::WhiteBalance { warmth, tint } => {
                processing::white_balance(image, warmth, tint, cancellation)?;
            }
            ModuleParameters::Exposure { stops } => {
                let gain = stops.exp2();
                for pixel in image.pixels_mut().iter_mut() {
                    if cancellation.is_cancelled() {
                        return Err(());
                    }
                    for value in pixel {
                        *value *= gain;
                    }
                }
            }
            ModuleParameters::Contrast { amount } => {
                processing::contrast(image, amount, cancellation)?;
            }
            ModuleParameters::TonalCurve { ref curves, mode } => {
                for pixel in image.pixels_mut().iter_mut() {
                    if cancellation.is_cancelled() {
                        return Err(());
                    }
                    let encoded = pixel.map(linear_to_adobe_rgb);
                    *pixel = curves.apply(mode, encoded).map(adobe_rgb_to_linear);
                }
            }
            ModuleParameters::LocalContrast { amount, radius } => {
                processing::local_contrast(image, amount, radius, cancellation)?;
            }
            ModuleParameters::Saturation { amount } => {
                processing::saturation(image, amount, cancellation)?;
            }
            ModuleParameters::NoiseReduction { luminance, colour } => {
                processing::noise_reduction(image, luminance, colour, cancellation)?;
            }
            ModuleParameters::OutputTransform => {
                for pixel in image.pixels_mut().iter_mut() {
                    if cancellation.is_cancelled() {
                        return Err(());
                    }
                    let linear_srgb = linear_adobe_rgb_to_srgb(*pixel);
                    *pixel = linear_srgb.map(|value| linear_to_srgb(value).clamp(0.0, 1.0));
                }
                image.set_contract(ImageContract::SRGB_DISPLAY);
            }
            ModuleParameters::OrientationAndCrop { crop } => {
                if let Some(crop) = crop {
                    apply_crop(image, crop, cancellation)?;
                }
            }
            ModuleParameters::HighlightsAndShadows
            | ModuleParameters::CreativeColour
            | ModuleParameters::Sharpening
            | ModuleParameters::Resize
            | ModuleParameters::QuantisationAndDither => {}
        }
        Ok(())
    }

    fn apply_optimized(
        &self,
        image: &mut Image,
        working_space: WorkingSpace,
        cancellation: &CancellationToken,
    ) -> Result<(), ()> {
        if !self.enabled {
            return Ok(());
        }
        match self.parameters {
            ModuleParameters::InputTransform => {
                let source_contract = image.contract();
                parallel_pixels(image, cancellation, |pixel| {
                    *pixel = if source_contract == ImageContract::SRGB_DISPLAY {
                        linear_srgb_to_adobe_rgb(pixel.map(srgb_to_linear))
                    } else {
                        pixel.map(adobe_rgb_to_linear)
                    };
                })?;
                image.set_contract(working_space.image_contract());
            }
            ModuleParameters::WhiteBalance { warmth, tint } => {
                processing::white_balance_optimized(image, warmth, tint, cancellation)?;
            }
            ModuleParameters::Exposure { stops } => {
                let gain = stops.exp2();
                parallel_pixels(image, cancellation, |pixel| {
                    for value in pixel {
                        *value *= gain;
                    }
                })?;
            }
            ModuleParameters::TonalCurve { ref curves, mode } => {
                parallel_pixels(image, cancellation, |pixel| {
                    let encoded = pixel.map(linear_to_adobe_rgb);
                    *pixel = curves.apply(mode, encoded).map(adobe_rgb_to_linear);
                })?;
            }
            ModuleParameters::OutputTransform => {
                parallel_pixels(image, cancellation, |pixel| {
                    let linear_srgb = linear_adobe_rgb_to_srgb(*pixel);
                    *pixel = linear_srgb.map(|value| linear_to_srgb(value).clamp(0.0, 1.0));
                })?;
                image.set_contract(ImageContract::SRGB_DISPLAY);
            }
            ModuleParameters::Saturation { amount } => {
                processing::saturation_optimized(image, amount, cancellation)?;
            }
            // These stages retain their proven implementation until their
            // parallel kernels have independent parity and cancellation tests.
            ModuleParameters::Contrast { amount } => {
                processing::contrast(image, amount, cancellation)?;
            }
            ModuleParameters::LocalContrast { amount, radius } => {
                processing::local_contrast(image, amount, radius, cancellation)?;
            }
            ModuleParameters::NoiseReduction { luminance, colour } => {
                processing::noise_reduction(image, luminance, colour, cancellation)?;
            }
            ModuleParameters::OrientationAndCrop { crop } => {
                if let Some(crop) = crop {
                    apply_crop(image, crop, cancellation)?;
                }
            }
            ModuleParameters::HighlightsAndShadows
            | ModuleParameters::CreativeColour
            | ModuleParameters::Sharpening
            | ModuleParameters::Resize
            | ModuleParameters::QuantisationAndDither => {}
        }
        Ok(())
    }

    #[must_use]
    pub(crate) const fn is_placeholder(&self) -> bool {
        matches!(
            self.parameters,
            ModuleParameters::HighlightsAndShadows
                | ModuleParameters::CreativeColour
                | ModuleParameters::Sharpening
                | ModuleParameters::Resize
                | ModuleParameters::QuantisationAndDither
        )
    }

    pub(crate) fn validate(&self) -> Result<(), &'static str> {
        match self.parameters {
            ModuleParameters::WhiteBalance { warmth, tint } => {
                validate_percentage(warmth, "white-balance warmth must be between -100 and 100")?;
                validate_percentage(tint, "white-balance tint must be between -100 and 100")
            }
            ModuleParameters::Exposure { stops } => stops
                .is_finite()
                .then_some(())
                .ok_or("exposure stops must be finite"),
            ModuleParameters::Contrast { amount } => {
                if !amount.is_finite() {
                    Err("contrast amount must be finite")
                } else if !(-100.0..=100.0).contains(&amount) {
                    Err("contrast amount must be between -100 and 100")
                } else {
                    Ok(())
                }
            }
            ModuleParameters::LocalContrast { amount, radius } => {
                validate_percentage(amount, "local-contrast amount must be between -100 and 100")?;
                if !radius.is_finite() {
                    Err("local-contrast radius must be finite")
                } else if !(1.0..=256.0).contains(&radius) {
                    Err("local-contrast radius must be between 1 and 256 pixels")
                } else {
                    Ok(())
                }
            }
            ModuleParameters::NoiseReduction { luminance, colour } => {
                validate_strength(luminance, true)?;
                validate_strength(colour, false)
            }
            ModuleParameters::Saturation { amount } => {
                validate_percentage(amount, "saturation amount must be between -100 and 100")
            }
            ModuleParameters::OrientationAndCrop { crop } => crop.map_or(Ok(()), validate_crop),
            ModuleParameters::InputTransform
            | ModuleParameters::HighlightsAndShadows
            | ModuleParameters::TonalCurve { .. }
            | ModuleParameters::CreativeColour
            | ModuleParameters::Sharpening
            | ModuleParameters::Resize
            | ModuleParameters::OutputTransform
            | ModuleParameters::QuantisationAndDither => Ok(()),
        }
    }

    #[must_use]
    pub(crate) const fn required_contract(
        &self,
        working_space: WorkingSpace,
    ) -> Option<ImageContract> {
        match self.parameters {
            ModuleParameters::OutputTransform
            | ModuleParameters::WhiteBalance { .. }
            | ModuleParameters::Exposure { .. }
            | ModuleParameters::HighlightsAndShadows
            | ModuleParameters::Contrast { .. }
            | ModuleParameters::TonalCurve { .. }
            | ModuleParameters::LocalContrast { .. }
            | ModuleParameters::Saturation { .. }
            | ModuleParameters::CreativeColour
            | ModuleParameters::NoiseReduction { .. }
            | ModuleParameters::Sharpening => Some(working_space.image_contract()),
            ModuleParameters::InputTransform
            | ModuleParameters::OrientationAndCrop { .. }
            | ModuleParameters::Resize
            | ModuleParameters::QuantisationAndDither => None,
        }
    }

    #[allow(clippy::cast_precision_loss)]
    pub(crate) fn validate_for_image(&self, image: &Image) -> Result<(), &'static str> {
        if matches!(self.parameters, ModuleParameters::InputTransform)
            && !matches!(
                image.contract(),
                ImageContract::SRGB_DISPLAY | ImageContract::ADOBE_RGB_CURVE
            )
        {
            return Err("input transform requires encoded sRGB or encoded Adobe RGB");
        }
        let ModuleParameters::OrientationAndCrop { crop: Some(crop) } = self.parameters else {
            return Ok(());
        };
        let aspect = image.width() as f32 / image.height() as f32;
        if crop_fits_rotated_source(crop, aspect) {
            Ok(())
        } else {
            Err("crop rectangle extends beyond the original image after rotation")
        }
    }
}

fn parallel_pixels(
    image: &mut Image,
    cancellation: &CancellationToken,
    operation: impl Fn(&mut [f32; 3]) + Sync + Send,
) -> Result<(), ()> {
    image
        .pixels_mut()
        .par_chunks_mut(2_048)
        .try_for_each(|chunk| {
            if cancellation.is_cancelled() {
                return Err(());
            }
            for pixel in chunk {
                operation(pixel);
            }
            Ok(())
        })
}

fn validate_crop(crop: CropSettings) -> Result<(), &'static str> {
    let values = [
        crop.left,
        crop.top,
        crop.right,
        crop.bottom,
        crop.rotation_degrees,
    ];
    if values.iter().any(|value| !value.is_finite()) {
        return Err("crop values must be finite");
    }
    if !(0.0..=1.0).contains(&crop.left)
        || !(0.0..=1.0).contains(&crop.top)
        || !(0.0..=1.0).contains(&crop.right)
        || !(0.0..=1.0).contains(&crop.bottom)
        || crop.left >= crop.right
        || crop.top >= crop.bottom
    {
        return Err("crop rectangle must be ordered inside the normalised image bounds");
    }
    if !(-45.0..=45.0).contains(&crop.rotation_degrees) {
        return Err("crop rotation must be between -45 and 45 degrees");
    }
    Ok(())
}

fn inverse_rotate_normalised(
    point: [f32; 2],
    centre: [f32; 2],
    degrees: f32,
    aspect: f32,
) -> [f32; 2] {
    let angle = -degrees.to_radians();
    let (sin, cos) = angle.sin_cos();
    let x = (point[0] - centre[0]) * aspect;
    let y = point[1] - centre[1];
    [
        (cos * x - sin * y) / aspect + centre[0],
        sin * x + cos * y + centre[1],
    ]
}

fn crop_fits_rotated_source(crop: CropSettings, aspect: f32) -> bool {
    let centre = [
        (crop.left + crop.right) * 0.5,
        (crop.top + crop.bottom) * 0.5,
    ];
    [
        [crop.left, crop.top],
        [crop.right, crop.top],
        [crop.right, crop.bottom],
        [crop.left, crop.bottom],
    ]
    .into_iter()
    .map(|point| inverse_rotate_normalised(point, centre, crop.rotation_degrees, aspect))
    .all(|point| (0.0..=1.0).contains(&point[0]) && (0.0..=1.0).contains(&point[1]))
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)]
fn apply_crop(
    image: &mut Image,
    crop: CropSettings,
    cancellation: &CancellationToken,
) -> Result<(), ()> {
    let source_width = image.width();
    let source_height = image.height();
    let aspect = source_width as f32 / source_height as f32;
    let centre = [
        (crop.left + crop.right) * 0.5,
        (crop.top + crop.bottom) * 0.5,
    ];
    let output_width = ((crop.right - crop.left) * source_width as f32)
        .round()
        .max(1.0) as u32;
    let output_height = ((crop.bottom - crop.top) * source_height as f32)
        .round()
        .max(1.0) as u32;
    let mut pixels = Vec::with_capacity(output_width as usize * output_height as usize);
    for y in 0..output_height {
        if cancellation.is_cancelled() {
            return Err(());
        }
        let normalised_y =
            crop.top + (y as f32 + 0.5) / output_height as f32 * (crop.bottom - crop.top);
        for x in 0..output_width {
            let normalised_x =
                crop.left + (x as f32 + 0.5) / output_width as f32 * (crop.right - crop.left);
            let source = inverse_rotate_normalised(
                [normalised_x, normalised_y],
                centre,
                crop.rotation_degrees,
                aspect,
            );
            let source_x = source[0] * source_width as f32 - 0.5;
            let source_y = source[1] * source_height as f32 - 0.5;
            pixels.push(bilinear_sample(image, source_x, source_y));
        }
    }
    *image = Image::new(output_width, output_height, pixels, image.contract())
        .expect("validated crop always constructs a finite, non-empty image");
    Ok(())
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)]
fn bilinear_sample(image: &Image, x: f32, y: f32) -> [f32; 3] {
    let x = x.clamp(0.0, image.width().saturating_sub(1) as f32);
    let y = y.clamp(0.0, image.height().saturating_sub(1) as f32);
    let x0 = x.floor() as u32;
    let y0 = y.floor() as u32;
    let x1 = (x0 + 1).min(image.width() - 1);
    let y1 = (y0 + 1).min(image.height() - 1);
    let tx = x - x0 as f32;
    let ty = y - y0 as f32;
    let pixel = |x: u32, y: u32| image.pixels()[(y * image.width() + x) as usize];
    let top = mix_pixel(pixel(x0, y0), pixel(x1, y0), tx);
    let bottom = mix_pixel(pixel(x0, y1), pixel(x1, y1), tx);
    mix_pixel(top, bottom, ty)
}

fn mix_pixel(a: [f32; 3], b: [f32; 3], amount: f32) -> [f32; 3] {
    std::array::from_fn(|channel| a[channel] + (b[channel] - a[channel]) * amount)
}

fn validate_percentage(value: f32, range_error: &'static str) -> Result<(), &'static str> {
    if !value.is_finite() {
        Err("adjustment value must be finite")
    } else if !(-100.0..=100.0).contains(&value) {
        Err(range_error)
    } else {
        Ok(())
    }
}

fn validate_strength(value: f32, luminance: bool) -> Result<(), &'static str> {
    if !value.is_finite() {
        Err("noise-reduction strength must be finite")
    } else if !(0.0..=100.0).contains(&value) {
        if luminance {
            Err("noise-reduction luminance must be between 0 and 100")
        } else {
            Err("noise-reduction colour must be between 0 and 100")
        }
    } else {
        Ok(())
    }
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

fn linear_to_adobe_rgb(value: f32) -> f32 {
    value.max(0.0).powf(1.0 / 2.199_218_8)
}

fn adobe_rgb_to_linear(value: f32) -> f32 {
    value.clamp(0.0, 1.0).powf(2.199_218_8)
}

fn linear_srgb_to_adobe_rgb(rgb: [f32; 3]) -> [f32; 3] {
    [
        0.715_126 * rgb[0] + 0.284_874 * rgb[1],
        0.000_000 * rgb[0] + 1.000_000 * rgb[1],
        0.000_000 * rgb[0] + 0.041_162 * rgb[1] + 0.958_838 * rgb[2],
    ]
}

fn linear_adobe_rgb_to_srgb(rgb: [f32; 3]) -> [f32; 3] {
    [
        1.398_355 * rgb[0] - 0.398_355 * rgb[1],
        rgb[1],
        -0.042_929 * rgb[1] + 1.042_929 * rgb[2],
    ]
}

/// Reports pixels which reach the output display boundary. Highlight clipping
/// remains channel-based, while low-light
/// clipping is based on lightness so a bright saturated colour with zero in a
/// secondary channel is not mistaken for a black pixel. This must be
/// calculated before the transform clamps the values, otherwise the evidence
/// of clipping is lost.
pub(crate) fn output_clipping_masks(image: &Image) -> (Vec<bool>, Vec<bool>) {
    image
        .pixels()
        .iter()
        .map(|pixel| {
            let linear_srgb = linear_adobe_rgb_to_srgb(*pixel);
            let non_negative = linear_srgb.map(|value| value.max(0.0));
            let lightness =
                0.212_6 * non_negative[0] + 0.715_2 * non_negative[1] + 0.072_2 * non_negative[2];
            (
                linear_srgb.iter().any(|value| *value >= 1.0),
                lightness <= 0.0,
            )
        })
        .unzip()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn module(parameters: ModuleParameters) -> Module {
        Module {
            enabled: true,
            parameters,
        }
    }

    #[test]
    fn percentage_adjustments_accept_limits_and_reject_both_sides() {
        let cases = [
            (-100.0, true),
            (-100.001, false),
            (100.0, true),
            (100.001, false),
        ];
        for (value, valid) in cases {
            assert_eq!(
                module(ModuleParameters::Saturation { amount: value })
                    .validate()
                    .is_ok(),
                valid,
                "saturation={value}"
            );
            assert_eq!(
                module(ModuleParameters::WhiteBalance {
                    warmth: value,
                    tint: 0.0,
                })
                .validate()
                .is_ok(),
                valid,
                "warmth={value}"
            );
            assert_eq!(
                module(ModuleParameters::LocalContrast {
                    amount: value,
                    radius: 1.0,
                })
                .validate()
                .is_ok(),
                valid,
                "local contrast={value}"
            );
            assert_eq!(
                module(ModuleParameters::Contrast { amount: value })
                    .validate()
                    .is_ok(),
                valid,
                "contrast={value}"
            );
        }
    }

    #[test]
    fn white_balance_tint_accepts_percentage_limits_and_rejects_both_sides() {
        let cases = [
            (-100.0, true),
            (-100.001, false),
            (100.0, true),
            (100.001, false),
        ];
        for (value, valid) in cases {
            assert_eq!(
                module(ModuleParameters::WhiteBalance {
                    warmth: 0.0,
                    tint: value,
                })
                .validate()
                .is_ok(),
                valid,
                "tint={value}"
            );
        }
    }

    #[test]
    fn local_contrast_radius_accepts_limits_and_rejects_both_sides() {
        for (value, valid) in [(1.0, true), (0.999, false), (256.0, true), (256.001, false)] {
            assert_eq!(
                module(ModuleParameters::LocalContrast {
                    amount: 0.0,
                    radius: value,
                })
                .validate()
                .is_ok(),
                valid,
                "radius={value}"
            );
        }
    }

    #[test]
    fn noise_reduction_limits_accept_zero_and_hundred_only() {
        for (value, valid) in [
            (0.0, true),
            (-0.001, false),
            (100.0, true),
            (100.001, false),
        ] {
            assert_eq!(
                module(ModuleParameters::NoiseReduction {
                    luminance: value,
                    colour: 0.0,
                })
                .validate()
                .is_ok(),
                valid,
                "luminance={value}"
            );
            assert_eq!(
                module(ModuleParameters::NoiseReduction {
                    luminance: 0.0,
                    colour: value,
                })
                .validate()
                .is_ok(),
                valid,
                "colour={value}"
            );
        }
    }

    #[test]
    fn exposure_accepts_finite_values_and_rejects_non_finite_values() {
        for (value, valid) in [
            (f32::NEG_INFINITY, false),
            (-1.0, true),
            (0.0, true),
            (1.0, true),
            (f32::INFINITY, false),
        ] {
            assert_eq!(
                module(ModuleParameters::Exposure { stops: value })
                    .validate()
                    .is_ok(),
                valid,
                "exposure={value}"
            );
        }
    }

    #[test]
    fn adjustment_validation_rejects_non_finite_percentage_values() {
        for value in [f32::NEG_INFINITY, f32::NAN, f32::INFINITY] {
            assert!(
                module(ModuleParameters::Saturation { amount: value })
                    .validate()
                    .is_err(),
                "saturation={value}"
            );
        }
    }

    #[test]
    fn required_contracts_distinguish_working_space_stages_from_boundary_stages() {
        let working = WorkingSpace::LinearAdobeRgb.image_contract();
        let working_parameters = [
            ModuleParameters::WhiteBalance {
                warmth: 0.0,
                tint: 0.0,
            },
            ModuleParameters::Exposure { stops: 0.0 },
            ModuleParameters::HighlightsAndShadows,
            ModuleParameters::Contrast { amount: 0.0 },
            ModuleParameters::TonalCurve {
                curves: CurveSet::default(),
                mode: CurveMode::LinkedRgb,
            },
            ModuleParameters::LocalContrast {
                amount: 0.0,
                radius: 1.0,
            },
            ModuleParameters::Saturation { amount: 0.0 },
            ModuleParameters::CreativeColour,
            ModuleParameters::NoiseReduction {
                luminance: 0.0,
                colour: 0.0,
            },
            ModuleParameters::Sharpening,
            ModuleParameters::OutputTransform,
        ];
        for parameters in working_parameters {
            assert_eq!(
                module(parameters).required_contract(WorkingSpace::LinearAdobeRgb),
                Some(working)
            );
        }

        let boundary_parameters = [
            ModuleParameters::InputTransform,
            ModuleParameters::OrientationAndCrop { crop: None },
            ModuleParameters::Resize,
            ModuleParameters::QuantisationAndDither,
        ];
        for parameters in boundary_parameters {
            assert_eq!(
                module(parameters).required_contract(WorkingSpace::LinearAdobeRgb),
                None
            );
        }
    }

    #[test]
    fn crop_safety_rejects_non_finite_and_non_positive_aspects() {
        let crop = CropSettings::full_image();
        for aspect in [0.0, -1.0, f32::NAN, f32::INFINITY] {
            assert!(!crop.is_safe_for_aspect(aspect), "aspect={aspect}");
        }
    }
}
