use std::fmt;

use serde::{Deserialize, Serialize};

use crate::{ColourEncoding, Image, ImageContract, Module, ModuleKind, ModuleParameters};

pub const PIPELINE_VERSION: u32 = 1;

/// Scene-linear RGB space used by processing modules.
///
/// This is saved even while only one space is supported, so adding another
/// space extends configuration rather than reinterpreting unlabelled pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorkingSpace {
    #[default]
    LinearSrgb,
}

impl WorkingSpace {
    #[must_use]
    pub const fn image_contract(self) -> ImageContract {
        match self {
            Self::LinearSrgb => ImageContract::LINEAR_SRGB,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PipelineSnapshot {
    pub version: u32,
    pub working_space: WorkingSpace,
    pub modules: Vec<Module>,
}

#[derive(Clone, Debug)]
pub struct Pipeline {
    snapshot: PipelineSnapshot,
}

impl Default for Pipeline {
    fn default() -> Self {
        Self::from_snapshot(PipelineSnapshot {
            version: PIPELINE_VERSION,
            working_space: WorkingSpace::default(),
            modules: vec![
                enabled(ModuleParameters::InputTransform),
                enabled(ModuleParameters::OrientationAndCrop),
                enabled(ModuleParameters::WhiteBalance {
                    multipliers: [1.0; 3],
                }),
                enabled(ModuleParameters::Exposure { stops: 0.0 }),
                enabled(ModuleParameters::HighlightsAndShadows),
                enabled(ModuleParameters::Contrast { amount: 0.0 }),
                enabled(ModuleParameters::TonalCurve),
                enabled(ModuleParameters::CreativeColour),
                enabled(ModuleParameters::NoiseReduction),
                enabled(ModuleParameters::Sharpening),
                enabled(ModuleParameters::Resize),
                enabled(ModuleParameters::OutputTransform),
                enabled(ModuleParameters::QuantisationAndDither),
            ],
        })
    }
}

const fn enabled(parameters: ModuleParameters) -> Module {
    Module {
        enabled: true,
        parameters,
    }
}

impl Pipeline {
    #[must_use]
    pub const fn from_snapshot(snapshot: PipelineSnapshot) -> Self {
        Self { snapshot }
    }

    #[must_use]
    pub fn snapshot(&self) -> PipelineSnapshot {
        self.snapshot.clone()
    }

    #[must_use]
    pub fn modules(&self) -> &[Module] {
        &self.snapshot.modules
    }

    /// Processes an immutable pipeline snapshot from first module to last.
    ///
    /// # Errors
    ///
    /// Returns [`PipelineError`] if a module receives pixels with the wrong
    /// encoding, contains an invalid parameter, or produces a non-finite
    /// channel value.
    pub fn render(&self, mut image: Image) -> Result<(Image, RenderReport), PipelineError> {
        if self.snapshot.version != PIPELINE_VERSION {
            return Err(PipelineError::UnsupportedPipelineVersion {
                expected: PIPELINE_VERSION,
                actual: self.snapshot.version,
            });
        }

        for module in &self.snapshot.modules {
            module
                .validate()
                .map_err(|reason| PipelineError::InvalidParameters {
                    module: module.kind(),
                    reason,
                })?;
        }

        let mut completed = Vec::with_capacity(self.snapshot.modules.len());
        for module in &self.snapshot.modules {
            if !module.enabled {
                continue;
            }
            if let Some(expected) = module.required_encoding() {
                let actual = image.contract().encoding;
                if actual != expected {
                    return Err(PipelineError::ContractMismatch {
                        module: module.kind(),
                        expected,
                        actual,
                    });
                }
            }
            module.apply(&mut image, self.snapshot.working_space);
            if image
                .pixels()
                .iter()
                .flatten()
                .any(|value| !value.is_finite())
            {
                return Err(PipelineError::NonFiniteOutput {
                    module: module.kind(),
                });
            }
            completed.push(module.kind());
        }
        Ok((image, RenderReport { completed }))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenderReport {
    pub completed: Vec<ModuleKind>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PipelineError {
    UnsupportedPipelineVersion {
        expected: u32,
        actual: u32,
    },
    InvalidParameters {
        module: ModuleKind,
        reason: &'static str,
    },
    ContractMismatch {
        module: ModuleKind,
        expected: ColourEncoding,
        actual: ColourEncoding,
    },
    NonFiniteOutput {
        module: ModuleKind,
    },
}

impl fmt::Display for PipelineError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedPipelineVersion { expected, actual } => write!(
                formatter,
                "unsupported pipeline version {actual}; this renderer supports version {expected}"
            ),
            Self::InvalidParameters { module, reason } => {
                write!(formatter, "{module:?} has invalid parameters: {reason}")
            }
            Self::ContractMismatch {
                module,
                expected,
                actual,
            } => write!(
                formatter,
                "{module:?} requires {expected:?} pixels, received {actual:?} pixels"
            ),
            Self::NonFiniteOutput { module } => {
                write!(formatter, "{module:?} produced a non-finite channel value")
            }
        }
    }
}

impl std::error::Error for PipelineError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ImageContract;

    #[test]
    fn default_modules_follow_the_documented_starting_order() {
        let kinds: Vec<_> = Pipeline::default()
            .modules()
            .iter()
            .map(Module::kind)
            .collect();
        assert_eq!(
            kinds,
            [
                ModuleKind::InputTransform,
                ModuleKind::OrientationAndCrop,
                ModuleKind::WhiteBalance,
                ModuleKind::Exposure,
                ModuleKind::HighlightsAndShadows,
                ModuleKind::Contrast,
                ModuleKind::TonalCurve,
                ModuleKind::CreativeColour,
                ModuleKind::NoiseReduction,
                ModuleKind::Sharpening,
                ModuleKind::Resize,
                ModuleKind::OutputTransform,
                ModuleKind::QuantisationAndDither,
            ]
        );
    }

    #[test]
    fn default_pipeline_round_trips_srgb_with_neutral_parameters() {
        let source = Image::new(1, 1, vec![[0.0, 0.5, 1.0]], ImageContract::SRGB_DISPLAY).unwrap();
        let (rendered, report) = Pipeline::default().render(source).unwrap();
        for (actual, expected) in rendered.pixels()[0].into_iter().zip([0.0, 0.5, 1.0]) {
            assert!((actual - expected).abs() < 1.0e-6);
        }
        assert_eq!(rendered.contract(), ImageContract::SRGB_DISPLAY);
        assert_eq!(report.completed.len(), 13);
    }

    #[test]
    fn exposure_is_applied_in_linear_light() {
        let mut pipeline = Pipeline::default();
        let exposure = pipeline
            .snapshot
            .modules
            .iter_mut()
            .find(|module| module.kind() == ModuleKind::Exposure)
            .unwrap();
        exposure.parameters = ModuleParameters::Exposure { stops: 1.0 };
        let source = Image::new(1, 1, vec![[0.5; 3]], ImageContract::SRGB_DISPLAY).unwrap();
        let (rendered, _) = pipeline.render(source).unwrap();
        assert!((rendered.pixels()[0][0] - 0.685_836).abs() < 1.0e-5);
    }

    #[test]
    fn invalid_reordering_fails_at_the_contract_boundary() {
        let pipeline = Pipeline::from_snapshot(PipelineSnapshot {
            version: PIPELINE_VERSION,
            working_space: WorkingSpace::default(),
            modules: vec![enabled(ModuleParameters::Exposure { stops: 1.0 })],
        });
        let source = Image::new(1, 1, vec![[0.5; 3]], ImageContract::SRGB_DISPLAY).unwrap();
        assert!(matches!(
            pipeline.render(source),
            Err(PipelineError::ContractMismatch {
                module: ModuleKind::Exposure,
                ..
            })
        ));
    }

    #[test]
    fn non_finite_parameters_cannot_break_the_image_contract() {
        let pipeline = Pipeline::from_snapshot(PipelineSnapshot {
            version: PIPELINE_VERSION,
            working_space: WorkingSpace::default(),
            modules: vec![
                enabled(ModuleParameters::InputTransform),
                enabled(ModuleParameters::Exposure { stops: f32::NAN }),
            ],
        });
        let source = Image::new(1, 1, vec![[0.5; 3]], ImageContract::SRGB_DISPLAY).unwrap();
        assert_eq!(
            pipeline.render(source).unwrap_err(),
            PipelineError::InvalidParameters {
                module: ModuleKind::Exposure,
                reason: "exposure stops must be finite",
            }
        );
    }

    #[test]
    fn unsupported_pipeline_version_is_rejected() {
        let pipeline = Pipeline::from_snapshot(PipelineSnapshot {
            version: PIPELINE_VERSION + 1,
            working_space: WorkingSpace::default(),
            modules: vec![],
        });
        let source = Image::new(1, 1, vec![[0.5; 3]], ImageContract::SRGB_DISPLAY).unwrap();

        assert_eq!(
            pipeline.render(source).unwrap_err(),
            PipelineError::UnsupportedPipelineVersion {
                expected: PIPELINE_VERSION,
                actual: PIPELINE_VERSION + 1,
            }
        );
    }

    #[test]
    fn non_finite_parameters_are_rejected_even_when_the_pixel_result_is_finite() {
        let pipeline = Pipeline::from_snapshot(PipelineSnapshot {
            version: PIPELINE_VERSION,
            working_space: WorkingSpace::default(),
            modules: vec![
                enabled(ModuleParameters::InputTransform),
                enabled(ModuleParameters::Exposure {
                    stops: f32::NEG_INFINITY,
                }),
            ],
        });
        let source = Image::new(1, 1, vec![[0.5; 3]], ImageContract::SRGB_DISPLAY).unwrap();

        assert_eq!(
            pipeline.render(source).unwrap_err(),
            PipelineError::InvalidParameters {
                module: ModuleKind::Exposure,
                reason: "exposure stops must be finite",
            }
        );
    }
}
