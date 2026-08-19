use std::fmt;

use serde::{Deserialize, Serialize};

use crate::{
    Image, ImageContract, Module, ModuleKind, ModuleParameters, ProgressReporter, RenderContext,
    RenderProgress, RenderQuality,
};

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
    /// image contract, contains an invalid parameter, or produces a
    /// non-finite channel value.
    pub fn render(&self, image: Image) -> Result<(Image, RenderReport), PipelineError> {
        let context = RenderContext::new(RenderQuality::Export);
        let mut ignore_progress = |_| {};
        self.render_with_context(image, &context, &mut ignore_progress)
    }

    /// Processes an immutable pipeline snapshot with cooperative execution
    /// controls. The caller owns request identity and may discard events from
    /// obsolete requests before presenting them.
    ///
    /// # Errors
    ///
    /// Returns [`PipelineError`] when the snapshot, parameters, image
    /// contract, output, or cancellation state is invalid.
    pub fn render_with_context<P: ProgressReporter>(
        &self,
        mut image: Image,
        context: &RenderContext,
        progress: &mut P,
    ) -> Result<(Image, RenderReport), PipelineError> {
        if self.snapshot.version != PIPELINE_VERSION {
            return Err(PipelineError::UnsupportedPipelineVersion {
                expected: PIPELINE_VERSION,
                actual: self.snapshot.version,
            });
        }

        self.validate_modules()?;

        let cancellation = context.cancellation_token();
        if cancellation.is_cancelled() {
            return Err(cancellation_error(None, None));
        }

        let total_stages = self
            .snapshot
            .modules
            .iter()
            .filter(|module| module.enabled)
            .count();
        report_progress(progress, 0, total_stages, None, None);
        if cancellation.is_cancelled() {
            return Err(cancellation_error(None, None));
        }

        let mut stages = Vec::with_capacity(total_stages);
        for (module_index, module) in self.snapshot.modules.iter().enumerate() {
            if !module.enabled {
                continue;
            }
            if cancellation.is_cancelled() {
                return Err(cancellation_error(Some(module.kind()), Some(module_index)));
            }
            report_progress(
                progress,
                stages.len(),
                total_stages,
                Some(module.kind()),
                Some(module_index),
            );
            if let Some(expected) = module.required_contract(self.snapshot.working_space) {
                let actual = image.contract();
                if actual != expected {
                    return Err(PipelineError::ContractMismatch {
                        module: module.kind(),
                        module_index,
                        expected,
                        actual,
                    });
                }
            }
            module
                .apply(&mut image, self.snapshot.working_space, &cancellation)
                .map_err(|()| cancellation_error(Some(module.kind()), Some(module_index)))?;
            if cancellation.is_cancelled() {
                return Err(cancellation_error(Some(module.kind()), Some(module_index)));
            }
            if image
                .pixels()
                .iter()
                .flatten()
                .any(|value| !value.is_finite())
            {
                return Err(PipelineError::NonFiniteOutput {
                    module: module.kind(),
                    module_index,
                });
            }
            stages.push(RenderStageReport {
                module_index,
                module: module.kind(),
                status: if module.is_placeholder() {
                    RenderStageStatus::Placeholder
                } else {
                    RenderStageStatus::Processed
                },
            });
            report_progress(progress, stages.len(), total_stages, None, None);
            if cancellation.is_cancelled() {
                return Err(cancellation_error(Some(module.kind()), Some(module_index)));
            }
        }
        if total_stages == 0 {
            report_empty_pipeline_completion(progress);
            if cancellation.is_cancelled() {
                return Err(cancellation_error(None, None));
            }
        }
        Ok((image, RenderReport { stages }))
    }

    fn validate_modules(&self) -> Result<(), PipelineError> {
        for (module_index, module) in self.snapshot.modules.iter().enumerate() {
            module
                .validate()
                .map_err(|reason| PipelineError::InvalidParameters {
                    module: module.kind(),
                    module_index,
                    reason,
                })?;
        }
        Ok(())
    }
}

#[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
fn progress_fraction(completed_stages: usize, total_stages: usize) -> f32 {
    let completed_stages = completed_stages.min(total_stages) as f64;
    let total_stages = total_stages.max(1) as f64;
    (completed_stages / total_stages) as f32
}

fn report_progress<P: ProgressReporter>(
    progress: &mut P,
    completed_stages: usize,
    total_stages: usize,
    current_module: Option<ModuleKind>,
    current_module_index: Option<usize>,
) {
    progress.report(RenderProgress {
        fraction: progress_fraction(completed_stages, total_stages),
        completed_stages,
        total_stages,
        current_module,
        current_module_index,
    });
}

fn report_empty_pipeline_completion<P: ProgressReporter>(progress: &mut P) {
    progress.report(RenderProgress {
        fraction: 1.0,
        completed_stages: 0,
        total_stages: 0,
        current_module: None,
        current_module_index: None,
    });
}

fn cancellation_error(module: Option<ModuleKind>, module_index: Option<usize>) -> PipelineError {
    PipelineError::Cancelled {
        module,
        module_index,
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenderReport {
    pub stages: Vec<RenderStageReport>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenderStageReport {
    pub module_index: usize,
    pub module: ModuleKind,
    pub status: RenderStageStatus,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RenderStageStatus {
    Processed,
    Placeholder,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PipelineError {
    UnsupportedPipelineVersion {
        expected: u32,
        actual: u32,
    },
    InvalidParameters {
        module: ModuleKind,
        module_index: usize,
        reason: &'static str,
    },
    Cancelled {
        module: Option<ModuleKind>,
        module_index: Option<usize>,
    },
    ContractMismatch {
        module: ModuleKind,
        module_index: usize,
        expected: ImageContract,
        actual: ImageContract,
    },
    NonFiniteOutput {
        module: ModuleKind,
        module_index: usize,
    },
}

impl fmt::Display for PipelineError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedPipelineVersion { expected, actual } => write!(
                formatter,
                "unsupported pipeline version {actual}; this renderer supports version {expected}"
            ),
            Self::InvalidParameters {
                module,
                module_index,
                reason,
            } => {
                write!(
                    formatter,
                    "module {module_index} ({module:?}) has invalid parameters: {reason}"
                )
            }
            Self::Cancelled {
                module: Some(module),
                module_index: Some(module_index),
            } => write!(
                formatter,
                "render cancelled during {module:?} at module {module_index}"
            ),
            Self::Cancelled { .. } => write!(formatter, "render cancelled"),
            Self::ContractMismatch {
                module,
                module_index,
                expected,
                actual,
            } => write!(
                formatter,
                "module {module_index} ({module:?}) requires contract {expected:?}, received {actual:?}"
            ),
            Self::NonFiniteOutput {
                module,
                module_index,
            } => {
                write!(
                    formatter,
                    "module {module_index} ({module:?}) produced a non-finite channel value"
                )
            }
        }
    }
}

impl std::error::Error for PipelineError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CancellationToken, ImageContract};

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
        assert_eq!(report.stages.len(), 13);
        assert_eq!(
            report
                .stages
                .iter()
                .filter(|stage| stage.status == RenderStageStatus::Processed)
                .count(),
            5
        );
        assert_eq!(
            report
                .stages
                .iter()
                .filter(|stage| stage.status == RenderStageStatus::Placeholder)
                .count(),
            8
        );
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
    fn module_contract_checks_include_more_than_encoding() {
        let source = Image::new(1, 1, vec![[0.5; 3]], ImageContract::LINEAR_SRGB).unwrap();
        let error = Pipeline::default().render(source).unwrap_err();

        assert_eq!(
            error,
            PipelineError::ContractMismatch {
                module: ModuleKind::InputTransform,
                module_index: 0,
                expected: ImageContract::SRGB_DISPLAY,
                actual: ImageContract::LINEAR_SRGB,
            }
        );
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
                module_index: 1,
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
                module_index: 1,
                reason: "exposure stops must be finite",
            }
        );
    }

    #[test]
    fn contrast_outside_the_documented_range_is_rejected() {
        let pipeline = Pipeline::from_snapshot(PipelineSnapshot {
            version: PIPELINE_VERSION,
            working_space: WorkingSpace::default(),
            modules: vec![enabled(ModuleParameters::Contrast { amount: 100.1 })],
        });
        let source = Image::new(1, 1, vec![[0.5; 3]], ImageContract::LINEAR_SRGB).unwrap();

        assert_eq!(
            pipeline.render(source).unwrap_err(),
            PipelineError::InvalidParameters {
                module: ModuleKind::Contrast,
                module_index: 0,
                reason: "contrast amount must be between -100 and 100",
            }
        );
    }

    #[test]
    fn negative_white_balance_multipliers_are_rejected() {
        let pipeline = Pipeline::from_snapshot(PipelineSnapshot {
            version: PIPELINE_VERSION,
            working_space: WorkingSpace::default(),
            modules: vec![enabled(ModuleParameters::WhiteBalance {
                multipliers: [1.0, -0.1, 1.0],
            })],
        });
        let source = Image::new(1, 1, vec![[0.5; 3]], ImageContract::LINEAR_SRGB).unwrap();

        assert_eq!(
            pipeline.render(source).unwrap_err(),
            PipelineError::InvalidParameters {
                module: ModuleKind::WhiteBalance,
                module_index: 0,
                reason: "white-balance multipliers must be finite and non-negative",
            }
        );
    }

    #[test]
    fn render_context_reports_ordered_stage_progress() {
        let source = Image::new(
            2,
            1,
            vec![[0.0, 0.5, 1.0], [0.25, 0.5, 0.75]],
            ImageContract::SRGB_DISPLAY,
        )
        .unwrap();
        let context = RenderContext::new(RenderQuality::Preview);
        assert_eq!(context.quality(), RenderQuality::Preview);
        let mut updates = Vec::new();

        Pipeline::default()
            .render_with_context(source, &context, &mut |update: RenderProgress| {
                updates.push(update);
            })
            .unwrap();

        assert!(updates.first().unwrap().fraction.abs() < f32::EPSILON);
        assert_eq!(updates.first().unwrap().current_module, None);
        assert!((updates.last().unwrap().fraction - 1.0).abs() < f32::EPSILON);
        assert_eq!(updates.last().unwrap().completed_stages, 13);
        assert!(updates.iter().any(|update| {
            update.current_module == Some(ModuleKind::Exposure)
                && update.current_module_index == Some(3)
        }));
        assert!(
            updates
                .iter()
                .all(|update| (0.0..=1.0).contains(&update.fraction))
        );
    }

    #[test]
    fn render_reports_completion_progress_only_once() {
        let pipeline = Pipeline::from_snapshot(PipelineSnapshot {
            version: PIPELINE_VERSION,
            working_space: WorkingSpace::default(),
            modules: vec![enabled(ModuleParameters::OrientationAndCrop)],
        });
        let source = Image::new(1, 1, vec![[0.5; 3]], ImageContract::SRGB_DISPLAY).unwrap();
        let mut updates = Vec::new();

        pipeline
            .render_with_context(
                source,
                &RenderContext::default(),
                &mut |update: RenderProgress| updates.push(update),
            )
            .unwrap();

        assert_eq!(
            updates
                .iter()
                .filter(|update| (update.fraction - 1.0).abs() < f32::EPSILON)
                .count(),
            1
        );
    }

    #[test]
    fn preview_and_export_quality_have_identical_initial_semantics() {
        let source = Image::new(
            2,
            1,
            vec![[0.0, 0.5, 1.0], [0.25, 0.5, 0.75]],
            ImageContract::SRGB_DISPLAY,
        )
        .unwrap();
        let preview = Pipeline::default()
            .render_with_context(
                source.clone(),
                &RenderContext::new(RenderQuality::Preview),
                &mut |_update: RenderProgress| {},
            )
            .unwrap()
            .0;
        let export = Pipeline::default()
            .render_with_context(
                source,
                &RenderContext::new(RenderQuality::Export),
                &mut |_update: RenderProgress| {},
            )
            .unwrap()
            .0;

        assert_eq!(preview, export);
    }

    #[test]
    fn empty_pipeline_reports_a_completed_render() {
        let pipeline = Pipeline::from_snapshot(PipelineSnapshot {
            version: PIPELINE_VERSION,
            working_space: WorkingSpace::default(),
            modules: Vec::new(),
        });
        let source = Image::new(1, 1, vec![[0.5; 3]], ImageContract::SRGB_DISPLAY).unwrap();
        let mut updates = Vec::new();

        let (_, report) = pipeline
            .render_with_context(
                source,
                &RenderContext::default(),
                &mut |update: RenderProgress| updates.push(update),
            )
            .unwrap();

        assert!(report.stages.is_empty());
        assert_eq!(updates.len(), 2);
        assert!(updates[0].fraction.abs() < f32::EPSILON);
        assert!((updates[1].fraction - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn cancelled_empty_pipeline_does_not_report_success() {
        let pipeline = Pipeline::from_snapshot(PipelineSnapshot {
            version: PIPELINE_VERSION,
            working_space: WorkingSpace::default(),
            modules: Vec::new(),
        });
        let source = Image::new(1, 1, vec![[0.5; 3]], ImageContract::SRGB_DISPLAY).unwrap();
        let token = CancellationToken::new();
        token.cancel();
        let context = RenderContext::with_cancellation(RenderQuality::Preview, token);

        assert_eq!(
            pipeline
                .render_with_context(source, &context, &mut |_update: RenderProgress| {})
                .unwrap_err(),
            PipelineError::Cancelled {
                module: None,
                module_index: None,
            }
        );
    }

    #[test]
    fn cancellation_during_initial_progress_cancels_an_empty_pipeline() {
        let pipeline = Pipeline::from_snapshot(PipelineSnapshot {
            version: PIPELINE_VERSION,
            working_space: WorkingSpace::default(),
            modules: Vec::new(),
        });
        let source = Image::new(1, 1, vec![[0.5; 3]], ImageContract::SRGB_DISPLAY).unwrap();
        let token = CancellationToken::new();

        let result = pipeline.render_with_context(
            source,
            &RenderContext::with_cancellation(RenderQuality::Preview, token.clone()),
            &mut |update: RenderProgress| {
                if update.completed_stages == 0 {
                    token.cancel();
                }
            },
        );

        assert_eq!(
            result.unwrap_err(),
            PipelineError::Cancelled {
                module: None,
                module_index: None,
            }
        );
    }

    #[test]
    fn cancellation_during_empty_completion_progress_does_not_report_success() {
        let pipeline = Pipeline::from_snapshot(PipelineSnapshot {
            version: PIPELINE_VERSION,
            working_space: WorkingSpace::default(),
            modules: Vec::new(),
        });
        let source = Image::new(1, 1, vec![[0.5; 3]], ImageContract::SRGB_DISPLAY).unwrap();
        let token = CancellationToken::new();

        let result = pipeline.render_with_context(
            source,
            &RenderContext::with_cancellation(RenderQuality::Preview, token.clone()),
            &mut |update: RenderProgress| {
                if (update.fraction - 1.0).abs() < f32::EPSILON {
                    token.cancel();
                }
            },
        );

        assert_eq!(
            result.unwrap_err(),
            PipelineError::Cancelled {
                module: None,
                module_index: None,
            }
        );
    }

    #[test]
    fn cancellation_during_final_progress_does_not_report_success() {
        let pipeline = Pipeline::from_snapshot(PipelineSnapshot {
            version: PIPELINE_VERSION,
            working_space: WorkingSpace::default(),
            modules: vec![enabled(ModuleParameters::OrientationAndCrop)],
        });
        let source = Image::new(1, 1, vec![[0.5; 3]], ImageContract::SRGB_DISPLAY).unwrap();
        let token = CancellationToken::new();

        let result = pipeline.render_with_context(
            source,
            &RenderContext::with_cancellation(RenderQuality::Preview, token.clone()),
            &mut |update: RenderProgress| {
                if update.completed_stages == update.total_stages {
                    token.cancel();
                }
            },
        );

        assert_eq!(
            result.unwrap_err(),
            PipelineError::Cancelled {
                module: Some(ModuleKind::OrientationAndCrop),
                module_index: Some(0),
            }
        );
    }

    #[test]
    fn progress_fraction_preserves_ratios_for_large_stage_counts() {
        assert!((progress_fraction(65_536, 131_072) - 0.5).abs() < 1.0e-6);
    }

    #[test]
    fn cancellation_stops_a_render_before_it_can_report_completion() {
        let source = Image::new(
            2,
            1,
            vec![[0.0, 0.5, 1.0], [0.25, 0.5, 0.75]],
            ImageContract::SRGB_DISPLAY,
        )
        .unwrap();
        let token = CancellationToken::new();
        let context = RenderContext::with_cancellation(RenderQuality::Preview, token.clone());
        let mut updates = Vec::new();

        let error = Pipeline::default()
            .render_with_context(source, &context, &mut |update: RenderProgress| {
                if update.current_module == Some(ModuleKind::InputTransform) {
                    token.cancel();
                }
                updates.push(update);
            })
            .unwrap_err();

        assert_eq!(
            error,
            PipelineError::Cancelled {
                module: Some(ModuleKind::InputTransform),
                module_index: Some(0),
            }
        );
        assert!(updates.iter().all(|update| update.fraction < 1.0));
    }

    #[test]
    fn pipeline_snapshots_round_trip_through_json() {
        let snapshot = Pipeline::default().snapshot();
        let encoded = serde_json::to_string(&snapshot).unwrap();
        let decoded: PipelineSnapshot = serde_json::from_str(&encoded).unwrap();

        assert_eq!(decoded, snapshot);
    }
}
