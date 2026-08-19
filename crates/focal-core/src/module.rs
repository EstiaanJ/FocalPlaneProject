use serde::{Deserialize, Serialize};

use crate::{ColourEncoding, Image, ImageContract, WorkingSpace};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModuleKind {
    InputTransform,
    OrientationAndCrop,
    WhiteBalance,
    Exposure,
    HighlightsAndShadows,
    Contrast,
    TonalCurve,
    CreativeColour,
    NoiseReduction,
    Sharpening,
    Resize,
    OutputTransform,
    QuantisationAndDither,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ModuleParameters {
    InputTransform,
    OrientationAndCrop,
    WhiteBalance { multipliers: [f32; 3] },
    Exposure { stops: f32 },
    HighlightsAndShadows,
    Contrast { amount: f32 },
    TonalCurve,
    CreativeColour,
    NoiseReduction,
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
    #[must_use]
    pub const fn kind(&self) -> ModuleKind {
        match self.parameters {
            ModuleParameters::InputTransform => ModuleKind::InputTransform,
            ModuleParameters::OrientationAndCrop => ModuleKind::OrientationAndCrop,
            ModuleParameters::WhiteBalance { .. } => ModuleKind::WhiteBalance,
            ModuleParameters::Exposure { .. } => ModuleKind::Exposure,
            ModuleParameters::HighlightsAndShadows => ModuleKind::HighlightsAndShadows,
            ModuleParameters::Contrast { .. } => ModuleKind::Contrast,
            ModuleParameters::TonalCurve => ModuleKind::TonalCurve,
            ModuleParameters::CreativeColour => ModuleKind::CreativeColour,
            ModuleParameters::NoiseReduction => ModuleKind::NoiseReduction,
            ModuleParameters::Sharpening => ModuleKind::Sharpening,
            ModuleParameters::Resize => ModuleKind::Resize,
            ModuleParameters::OutputTransform => ModuleKind::OutputTransform,
            ModuleParameters::QuantisationAndDither => ModuleKind::QuantisationAndDither,
        }
    }

    pub(crate) fn apply(&self, image: &mut Image, working_space: WorkingSpace) {
        if !self.enabled {
            return;
        }
        match self.parameters {
            ModuleParameters::InputTransform => {
                for pixel in image.pixels_mut() {
                    for value in pixel {
                        *value = srgb_to_linear(*value);
                    }
                }
                image.set_contract(working_space.image_contract());
            }
            ModuleParameters::WhiteBalance { multipliers } => {
                for pixel in image.pixels_mut() {
                    for (value, multiplier) in pixel.iter_mut().zip(multipliers) {
                        *value *= multiplier;
                    }
                }
            }
            ModuleParameters::Exposure { stops } => {
                let gain = stops.exp2();
                for value in image.pixels_mut().iter_mut().flatten() {
                    *value *= gain;
                }
            }
            ModuleParameters::Contrast { amount } => {
                // Temporary CPU-reference definition: linear contrast around
                // 18% grey. This is intentionally easy to replace after the
                // photographic behaviour has been evaluated.
                let slope = (1.0 + amount.clamp(-100.0, 100.0) / 100.0).max(0.0);
                for value in image.pixels_mut().iter_mut().flatten() {
                    *value = 0.18 + (*value - 0.18) * slope;
                }
            }
            ModuleParameters::OutputTransform => {
                for pixel in image.pixels_mut() {
                    for value in pixel {
                        *value = linear_to_srgb(*value);
                    }
                }
                image.set_contract(ImageContract::SRGB_DISPLAY);
            }
            ModuleParameters::OrientationAndCrop
            | ModuleParameters::HighlightsAndShadows
            | ModuleParameters::TonalCurve
            | ModuleParameters::CreativeColour
            | ModuleParameters::NoiseReduction
            | ModuleParameters::Sharpening
            | ModuleParameters::Resize
            | ModuleParameters::QuantisationAndDither => {}
        }
    }

    #[must_use]
    pub(crate) const fn required_encoding(&self) -> Option<ColourEncoding> {
        match self.parameters {
            ModuleParameters::InputTransform => Some(ColourEncoding::Srgb),
            ModuleParameters::OutputTransform
            | ModuleParameters::WhiteBalance { .. }
            | ModuleParameters::Exposure { .. }
            | ModuleParameters::HighlightsAndShadows
            | ModuleParameters::Contrast { .. }
            | ModuleParameters::TonalCurve
            | ModuleParameters::CreativeColour
            | ModuleParameters::NoiseReduction
            | ModuleParameters::Sharpening => Some(ColourEncoding::Linear),
            ModuleParameters::OrientationAndCrop
            | ModuleParameters::Resize
            | ModuleParameters::QuantisationAndDither => None,
        }
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
