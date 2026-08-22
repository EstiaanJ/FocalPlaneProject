#![cfg(feature = "gpu")]

use std::path::Path;
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant};

use focal_core::{
    CancellationToken, Image, ImageContract, Module, ModuleParameters, PIPELINE_VERSION, Pipeline,
    PipelineError, PipelineSnapshot, RenderContext, RenderProgress, RenderQuality, WorkingSpace,
};
use image::DynamicImage;

static GPU_TEST_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

fn fixture(name: &str) -> Option<Image> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../test-image")
        .join(name);
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) => {
            assert!(
                !gpu_tests_required(),
                "required GPU fixture {} cannot be read: {error}",
                path.display()
            );
            return None;
        }
    };
    let decoded = match image::load_from_memory(&bytes) {
        Ok(decoded) => decoded,
        Err(error) => panic!("GPU fixture {} cannot be decoded: {error}", path.display()),
    };
    Some(to_focal_image(&decoded))
}

fn gpu_tests_required() -> bool {
    std::env::var("FOCAL_REQUIRE_GPU_TESTS").is_ok_and(|value| value == "1")
}

fn to_focal_image(decoded: &DynamicImage) -> Image {
    let rgb = decoded.to_rgb32f();
    let (width, height) = rgb.dimensions();
    let pixels = rgb
        .pixels()
        .map(|pixel| [pixel[0], pixel[1], pixel[2]])
        .collect();
    Image::new(width, height, pixels, ImageContract::SRGB_DISPLAY).unwrap()
}

fn available_gpu() -> Option<(MutexGuard<'static, ()>, focal_core::gpu::GpuPipeline)> {
    let guard = GPU_TEST_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    match focal_core::gpu::GpuPipeline::new() {
        Ok(gpu) => Some((guard, gpu)),
        Err(error) => {
            assert!(
                !gpu_tests_required(),
                "GPU tests are required but no adapter could be created: {error}"
            );
            eprintln!("skipping GPU test: {error}");
            None
        }
    }
}

#[test]
fn gpu_matches_cpu_reference_on_top_level_fixtures() {
    let Some((_guard, gpu)) = available_gpu() else {
        return;
    };
    let pipeline = Pipeline::default();
    let mut fixture_count = 0;
    for name in [
        "color_patches.png",
        "neutral_gray.png",
        "gradients.png",
        "pure_chroma_16.png",
        "frequency_sweep_mtf.png",
        "slanted_edge_mtf.png",
        "radial_mtf.png",
    ] {
        let Some(source) = fixture(name) else {
            continue;
        };
        fixture_count += 1;
        let (cpu, cpu_report) = pipeline.render(source.clone()).unwrap();
        let (gpu, gpu_report) = gpu.render(&pipeline, &source).unwrap();
        assert_eq!(gpu.width(), cpu.width(), "fixture={name}");
        assert_eq!(gpu.height(), cpu.height(), "fixture={name}");
        assert_eq!(gpu.contract(), cpu.contract(), "fixture={name}");
        assert_eq!(gpu_report.stages, cpu_report.stages, "fixture={name}");
        assert_eq!(gpu.pixels().len(), cpu.pixels().len(), "fixture={name}");

        let max_error = gpu
            .pixels()
            .iter()
            .zip(cpu.pixels())
            .flat_map(|(gpu, cpu)| gpu.iter().zip(cpu))
            .map(|(gpu, cpu)| (gpu - cpu).abs())
            .fold(0.0_f32, f32::max);
        assert!(max_error <= 2.0e-5, "fixture={name}, max_error={max_error}");
        assert!(
            gpu.pixels()
                .iter()
                .flatten()
                .all(|value| value.is_finite() && (0.0..=1.0).contains(value))
        );
    }
    if fixture_count == 0 {
        assert!(
            !gpu_tests_required(),
            "no required GPU fixtures were exercised"
        );
        eprintln!("skipping GPU parity test: top-level test-image fixtures are missing");
    }
}

#[test]
fn gpu_observes_preexisting_and_progress_callback_cancellation() {
    let Some(source) = fixture("gradients.png") else {
        return;
    };
    let Some((_guard, gpu)) = available_gpu() else {
        return;
    };
    let pipeline = Pipeline::default();
    let token = CancellationToken::new();
    token.cancel();
    let error = gpu
        .render_with_context(
            &pipeline,
            &source,
            &RenderContext::with_cancellation(RenderQuality::Export, token),
            &mut |_: RenderProgress| {},
        )
        .unwrap_err();
    assert!(matches!(
        error,
        focal_core::gpu::GpuError::Pipeline(PipelineError::Cancelled { .. })
    ));

    let token = CancellationToken::new();
    let context = RenderContext::with_cancellation(RenderQuality::Export, token.clone());
    let error = gpu
        .render_with_context(&pipeline, &source, &context, &mut |_: RenderProgress| {
            token.cancel();
        })
        .unwrap_err();
    assert!(matches!(
        error,
        focal_core::gpu::GpuError::Pipeline(PipelineError::Cancelled { .. })
    ));
}

#[test]
fn optimized_preview_uses_cpu_until_gpu_clipping_reports_have_parity() {
    let Some(source) = fixture("gradients.png") else {
        return;
    };
    let _guard = GPU_TEST_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let executor = focal_core::OptimizedPipeline::new().unwrap();
    if executor.gpu_initialization_error().is_some() {
        assert!(
            !gpu_tests_required(),
            "required GPU executor was unavailable"
        );
        return;
    }
    let (rendered, report, backend) = executor
        .render_with_context(
            &Pipeline::default(),
            source.clone(),
            &RenderContext::new(RenderQuality::Preview),
            &mut |_: RenderProgress| {},
        )
        .unwrap();
    let (reference, reference_report) = Pipeline::default()
        .render_with_context(
            source,
            &RenderContext::new(RenderQuality::Preview),
            &mut |_: RenderProgress| {},
        )
        .unwrap();
    assert_eq!(backend, focal_core::OptimizedBackend::Cpu);
    assert_eq!(rendered, reference);
    assert_eq!(report, reference_report);
    assert!(report.clipping.is_some());
}

#[test]
fn gpu_rejects_exposure_which_cannot_preserve_stage_boundary_validation() {
    let Some(source) = fixture("gradients.png") else {
        return;
    };
    let Some((_guard, gpu)) = available_gpu() else {
        return;
    };
    let pipeline = Pipeline::from_snapshot(PipelineSnapshot {
        version: PIPELINE_VERSION,
        working_space: WorkingSpace::LinearAdobeRgb,
        modules: vec![
            Module {
                enabled: true,
                parameters: ModuleParameters::InputTransform,
            },
            Module {
                enabled: true,
                parameters: ModuleParameters::Exposure { stops: 128.0 },
            },
            Module {
                enabled: true,
                parameters: ModuleParameters::OutputTransform,
            },
        ],
    });
    assert!(matches!(
        gpu.render(&pipeline, &source),
        Err(focal_core::gpu::GpuError::UnsupportedPipeline(_))
    ));
}

#[test]
fn gpu_performance_smoke_has_no_catastrophic_regression() {
    let Some(source) = fixture("radial_mtf.png") else {
        return;
    };
    let Some((_guard, gpu)) = available_gpu() else {
        return;
    };
    let pipeline = Pipeline::default();
    let _ = gpu.render(&pipeline, &source).unwrap();

    let cpu_start = Instant::now();
    for _ in 0..3 {
        let _ = pipeline.render(source.clone()).unwrap();
    }
    let cpu_elapsed = cpu_start.elapsed();

    let gpu_start = Instant::now();
    for _ in 0..3 {
        let _ = gpu.render(&pipeline, &source).unwrap();
    }
    let gpu_elapsed = gpu_start.elapsed();

    // Transfer and dispatch overhead makes small fixtures unsuitable for a
    // speed-up gate. This catches hangs and gross regressions while the
    // benchmark example records the real ratio on a target machine.
    let max_slowdown = std::env::var("FOCAL_GPU_MAX_SLOWDOWN")
        .ok()
        .and_then(|value| value.parse::<f64>().ok())
        .unwrap_or(100.0);
    let allowance = cpu_elapsed.mul_f64(max_slowdown);
    assert!(
        gpu_elapsed <= allowance.saturating_add(Duration::from_secs(2)),
        "GPU smoke regression: CPU={cpu_elapsed:?}, GPU={gpu_elapsed:?}, allowed={allowance:?}"
    );
}

#[test]
fn gpu_matches_cpu_for_exposure_and_white_balance() {
    let Some(source) = fixture("color_patches.png") else {
        return;
    };
    let Some((_guard, gpu)) = available_gpu() else {
        return;
    };
    let enabled = |parameters| Module {
        enabled: true,
        parameters,
    };
    let pipeline = Pipeline::from_snapshot(PipelineSnapshot {
        version: PIPELINE_VERSION,
        working_space: WorkingSpace::LinearAdobeRgb,
        modules: vec![
            enabled(ModuleParameters::InputTransform),
            enabled(ModuleParameters::WhiteBalance {
                warmth: 37.0,
                tint: -18.0,
            }),
            enabled(ModuleParameters::Exposure { stops: 0.65 }),
            enabled(ModuleParameters::OutputTransform),
        ],
    });
    let (cpu, _) = pipeline.render(source.clone()).unwrap();
    let (gpu, _) = gpu.render(&pipeline, &source).unwrap();
    let max_error = gpu
        .pixels()
        .iter()
        .zip(cpu.pixels())
        .flat_map(|(gpu, cpu)| gpu.iter().zip(cpu))
        .map(|(gpu, cpu)| (gpu - cpu).abs())
        .fold(0.0_f32, f32::max);
    assert!(max_error <= 2.0e-5, "max_error={max_error}");
}

#[test]
fn gpu_export_reports_the_same_stage_progress_as_reference() {
    let Some(source) = fixture("gradients.png") else {
        return;
    };
    let Some((_guard, gpu)) = available_gpu() else {
        return;
    };
    let pipeline = Pipeline::default();
    let context = RenderContext::new(RenderQuality::Export);
    let mut reference_progress = Vec::new();
    pipeline
        .render_with_context(source.clone(), &context, &mut |update| {
            reference_progress.push(update);
        })
        .unwrap();
    let mut gpu_progress = Vec::new();
    gpu.render_with_context(&pipeline, &source, &context, &mut |update| {
        gpu_progress.push(update);
    })
    .unwrap();
    assert_eq!(gpu_progress, reference_progress);
}

#[test]
fn gpu_rejects_unsupported_spatial_work_instead_of_falling_back_silently() {
    let Some(source) = fixture("gradients.png") else {
        return;
    };
    let Some((_guard, gpu)) = available_gpu() else {
        return;
    };
    let pipeline = Pipeline::from_snapshot(PipelineSnapshot {
        version: PIPELINE_VERSION,
        working_space: WorkingSpace::LinearAdobeRgb,
        modules: vec![
            Module {
                enabled: true,
                parameters: ModuleParameters::InputTransform,
            },
            Module {
                enabled: true,
                parameters: ModuleParameters::LocalContrast {
                    amount: 20.0,
                    radius: 4.0,
                },
            },
        ],
    });
    let error = gpu.render(&pipeline, &source).unwrap_err();
    assert!(matches!(
        error,
        focal_core::gpu::GpuError::UnsupportedPipeline(_)
    ));
}
