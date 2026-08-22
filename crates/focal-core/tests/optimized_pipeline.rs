use focal_core::{
    CancellationToken, CurveMode, CurvePoint, CurveSet, Image, ImageContract, Module,
    ModuleParameters, OptimizedBackend, OptimizedPipeline, PIPELINE_VERSION, Pipeline,
    PipelineError, PipelineSnapshot, RenderContext, RenderProgress, RenderQuality, SmoothCurve,
    WorkingSpace,
};
use std::time::{Duration, Instant};

#[allow(clippy::cast_precision_loss)]
fn image(width: u32, height: u32) -> Image {
    let pixels = (0..width * height)
        .map(|index| {
            let value = index as f32 / (width * height - 1).max(1) as f32;
            [value, (value * 0.73).min(1.0), (1.0 - value) * 0.81]
        })
        .collect();
    Image::new(width, height, pixels, ImageContract::SRGB_DISPLAY).unwrap()
}

fn enabled(parameters: ModuleParameters) -> Module {
    Module {
        enabled: true,
        parameters,
    }
}

#[test]
fn optimized_cpu_matches_reference_for_pointwise_and_spatial_stages() {
    let pipeline = Pipeline::from_snapshot(PipelineSnapshot {
        version: PIPELINE_VERSION,
        working_space: WorkingSpace::LinearAdobeRgb,
        modules: vec![
            enabled(ModuleParameters::InputTransform),
            enabled(ModuleParameters::WhiteBalance {
                warmth: 27.0,
                tint: -11.0,
            }),
            enabled(ModuleParameters::Exposure { stops: 0.35 }),
            enabled(ModuleParameters::NoiseReduction {
                luminance: 20.0,
                colour: 35.0,
            }),
            enabled(ModuleParameters::Contrast { amount: 18.0 }),
            enabled(ModuleParameters::LocalContrast {
                amount: 24.0,
                radius: 3.0,
            }),
            enabled(ModuleParameters::Saturation { amount: 15.0 }),
            enabled(ModuleParameters::OutputTransform),
        ],
    });
    let source = image(96, 64);
    let (reference, reference_report) = pipeline.render(source.clone()).unwrap();
    let (optimized, optimized_report, backend) = OptimizedPipeline::cpu_only()
        .unwrap()
        .render(&pipeline, source)
        .unwrap();

    assert_eq!(backend, OptimizedBackend::Cpu);
    assert_eq!(optimized.contract(), reference.contract());
    assert_eq!(optimized_report, reference_report);
    assert_eq!(optimized.pixels(), reference.pixels());
}

#[test]
fn optimized_executor_reports_its_selected_backend() {
    let pipeline = Pipeline::default();
    let (_, _, backend) = OptimizedPipeline::cpu_only()
        .unwrap()
        .render(&pipeline, image(8, 8))
        .unwrap();
    assert_eq!(backend, OptimizedBackend::Cpu);
}

#[test]
fn optimized_cpu_preserves_context_progress_and_preview_clipping() {
    let pipeline = Pipeline::default();
    let source = image(32, 16);
    let context = RenderContext::new(RenderQuality::Preview);
    let mut reference_progress = Vec::new();
    let (reference, reference_report) = pipeline
        .render_with_context(source.clone(), &context, &mut |update| {
            reference_progress.push(update);
        })
        .unwrap();
    let mut optimized_progress = Vec::new();
    let (optimized, optimized_report, _) = OptimizedPipeline::cpu_only()
        .unwrap()
        .render_with_context(&pipeline, source, &context, &mut |update| {
            optimized_progress.push(update);
        })
        .unwrap();

    assert_eq!(optimized, reference);
    assert_eq!(optimized_report, reference_report);
    assert_eq!(optimized_progress, reference_progress);
    assert!(optimized_report.clipping.is_some());
}

#[test]
fn optimized_cpu_observes_progress_callback_cancellation() {
    let token = CancellationToken::new();
    let context = RenderContext::with_cancellation(RenderQuality::Preview, token.clone());
    let error = OptimizedPipeline::cpu_only()
        .unwrap()
        .render_with_context(
            &Pipeline::default(),
            image(128, 64),
            &context,
            &mut |update: RenderProgress| {
                if update.completed_stages == 1 {
                    token.cancel();
                }
            },
        )
        .unwrap_err();

    assert!(matches!(
        error,
        focal_core::OptimizedPipelineError::Pipeline(PipelineError::Cancelled { .. })
    ));
}

#[test]
fn optimized_cpu_matches_reference_for_crop_and_each_curve_mode() {
    let curve = SmoothCurve::from_points(vec![
        CurvePoint { x: 0.0, y: 0.0 },
        CurvePoint { x: 0.4, y: 0.3 },
        CurvePoint { x: 1.0, y: 1.0 },
    ])
    .unwrap();
    for mode in [
        CurveMode::LinkedRgb,
        CurveMode::Luma,
        CurveMode::PerChannelRgb,
    ] {
        let curves = CurveSet {
            linked: curve.clone(),
            luma: curve.clone(),
            red: curve.clone(),
            green: curve.clone(),
            blue: curve.clone(),
        };
        let pipeline = Pipeline::from_snapshot(PipelineSnapshot {
            version: PIPELINE_VERSION,
            working_space: WorkingSpace::LinearAdobeRgb,
            modules: vec![
                enabled(ModuleParameters::InputTransform),
                enabled(ModuleParameters::OrientationAndCrop {
                    crop: Some(focal_core::CropSettings {
                        left: 0.125,
                        top: 0.125,
                        right: 0.875,
                        bottom: 0.875,
                        rotation_degrees: 0.0,
                    }),
                }),
                enabled(ModuleParameters::TonalCurve { curves, mode }),
                enabled(ModuleParameters::OutputTransform),
            ],
        });
        let source = image(17, 1);
        let reference = pipeline.render(source.clone()).unwrap();
        let optimized = OptimizedPipeline::cpu_only()
            .unwrap()
            .render(&pipeline, source)
            .unwrap();
        assert_eq!(optimized.0, reference.0, "mode={mode:?}");
        assert_eq!(optimized.1, reference.1, "mode={mode:?}");
    }
}

#[test]
fn optimized_cpu_rejects_the_same_invalid_version_as_reference() {
    let pipeline = Pipeline::from_snapshot(PipelineSnapshot {
        version: PIPELINE_VERSION + 1,
        ..Pipeline::default().snapshot()
    });
    let source = image(1, 1);
    let reference = pipeline.render(source.clone()).unwrap_err();
    let optimized = OptimizedPipeline::cpu_only()
        .unwrap()
        .render(&pipeline, source)
        .unwrap_err();
    assert_eq!(
        optimized.to_string(),
        format!("optimized CPU render failed: {reference}")
    );
}

#[test]
fn optimized_cpu_performance_has_no_catastrophic_regression() {
    let pipeline = Pipeline::default();
    let source = image(1_024, 512);
    let optimized = OptimizedPipeline::cpu_only().unwrap();
    let _ = optimized.render(&pipeline, source.clone()).unwrap();

    let reference_start = Instant::now();
    for _ in 0..2 {
        let _ = pipeline.render(source.clone()).unwrap();
    }
    let reference_elapsed = reference_start.elapsed();
    let optimized_start = Instant::now();
    for _ in 0..2 {
        let _ = optimized.render(&pipeline, source.clone()).unwrap();
    }
    let optimized_elapsed = optimized_start.elapsed();
    let max_slowdown = std::env::var("FOCAL_OPTIMIZED_CPU_MAX_SLOWDOWN")
        .ok()
        .and_then(|value| value.parse::<f64>().ok())
        .unwrap_or(3.0);
    let allowance = reference_elapsed
        .mul_f64(max_slowdown)
        .saturating_add(Duration::from_millis(100));
    assert!(
        optimized_elapsed <= allowance,
        "optimized CPU smoke regression: reference={reference_elapsed:?}, optimized={optimized_elapsed:?}, allowed={allowance:?}"
    );
}

#[test]
fn optimized_cpu_stops_started_parallel_work_within_the_latency_budget() {
    let curve = SmoothCurve::from_points(vec![
        CurvePoint { x: 0.0, y: 0.0 },
        CurvePoint { x: 0.45, y: 0.35 },
        CurvePoint { x: 1.0, y: 1.0 },
    ])
    .unwrap();
    let curves = CurveSet {
        linked: curve,
        ..CurveSet::default()
    };
    let pipeline = Pipeline::from_snapshot(PipelineSnapshot {
        version: PIPELINE_VERSION,
        working_space: WorkingSpace::LinearAdobeRgb,
        modules: vec![
            enabled(ModuleParameters::InputTransform),
            enabled(ModuleParameters::TonalCurve {
                curves,
                mode: CurveMode::LinkedRgb,
            }),
            enabled(ModuleParameters::OutputTransform),
        ],
    });
    let source = image(2_048, 1_024);
    let executor = OptimizedPipeline::cpu_only().unwrap();
    let token = CancellationToken::new();
    let context = RenderContext::with_cancellation(RenderQuality::Preview, token.clone());
    let (started_sender, started_receiver) = std::sync::mpsc::sync_channel(1);

    std::thread::scope(|scope| {
        let worker = scope.spawn(|| {
            let mut started_sender = Some(started_sender);
            executor.render_with_context(
                &pipeline,
                source,
                &context,
                &mut |update: RenderProgress| {
                    if update.current_module == Some(focal_core::ModuleKind::TonalCurve)
                        && let Some(sender) = started_sender.take()
                    {
                        sender.send(()).unwrap();
                    }
                },
            )
        });
        started_receiver.recv().unwrap();
        let cancellation_start = Instant::now();
        token.cancel();
        let result = worker.join().unwrap();
        let cancellation_latency = cancellation_start.elapsed();
        assert!(matches!(
            result,
            Err(focal_core::OptimizedPipelineError::Pipeline(
                PipelineError::Cancelled { .. }
            ))
        ));
        assert!(
            cancellation_latency <= Duration::from_millis(150),
            "optimized CPU cancellation took {cancellation_latency:?}"
        );
    });
}
