use focal_core::{
    Image, ImageContract, Module, ModuleKind, ModuleParameters, PIPELINE_VERSION, Pipeline,
    PipelineError, PipelineSnapshot, RenderContext, RenderQuality, WorkingSpace,
};

fn enabled(parameters: ModuleParameters) -> Module {
    Module {
        enabled: true,
        parameters,
    }
}

fn linear_source() -> Image {
    Image::new(1, 1, vec![[0.5; 3]], ImageContract::LINEAR_ADOBE_RGB).unwrap()
}

fn unit_interval(index: u32, last: u32) -> f32 {
    f32::from(u16::try_from(index).expect("test fixture index fits u16"))
        / f32::from(u16::try_from(last).expect("test fixture bound fits u16"))
}

#[test]
fn invalid_module_state_matrix_is_rejected_at_the_snapshot_boundary() {
    let cases = [
        (
            "non-finite exposure",
            ModuleParameters::Exposure { stops: f32::NAN },
            ModuleKind::Exposure,
            "exposure stops must be finite",
        ),
        (
            "contrast above range",
            ModuleParameters::Contrast { amount: 100.1 },
            ModuleKind::Contrast,
            "contrast amount must be between -100 and 100",
        ),
        (
            "white balance below range",
            ModuleParameters::WhiteBalance {
                warmth: -100.1,
                tint: 0.0,
            },
            ModuleKind::WhiteBalance,
            "white-balance warmth must be between -100 and 100",
        ),
        (
            "local contrast non-finite amount",
            ModuleParameters::LocalContrast {
                amount: f32::INFINITY,
                radius: 80.0,
            },
            ModuleKind::LocalContrast,
            "adjustment value must be finite",
        ),
        (
            "local contrast zero radius",
            ModuleParameters::LocalContrast {
                amount: 0.0,
                radius: 0.0,
            },
            ModuleKind::LocalContrast,
            "local-contrast radius must be between 1 and 256 pixels",
        ),
        (
            "negative luminance noise strength",
            ModuleParameters::NoiseReduction {
                luminance: -0.1,
                colour: 0.0,
            },
            ModuleKind::NoiseReduction,
            "noise-reduction luminance must be between 0 and 100",
        ),
        (
            "non-finite saturation",
            ModuleParameters::Saturation {
                amount: f32::NEG_INFINITY,
            },
            ModuleKind::Saturation,
            "adjustment value must be finite",
        ),
        (
            "non-finite crop",
            ModuleParameters::OrientationAndCrop {
                crop: Some(focal_core::CropSettings {
                    left: f32::NAN,
                    top: 0.0,
                    right: 1.0,
                    bottom: 1.0,
                    rotation_degrees: 0.0,
                }),
            },
            ModuleKind::OrientationAndCrop,
            "crop values must be finite",
        ),
    ];

    for (label, parameters, module, reason) in cases {
        let pipeline = Pipeline::from_snapshot(PipelineSnapshot {
            version: PIPELINE_VERSION,
            working_space: WorkingSpace::default(),
            modules: vec![enabled(parameters)],
        });

        assert_eq!(
            pipeline.render(linear_source()).unwrap_err(),
            PipelineError::InvalidParameters {
                module,
                module_index: 0,
                reason,
            },
            "invalid state case: {label}"
        );
    }
}

#[test]
fn default_pipeline_preserves_adversarial_image_shapes_and_display_contract() {
    let shapes = [(1, 1), (1, 17), (257, 1), (3, 2)];

    for (width, height) in shapes {
        let pixels = (0..height)
            .flat_map(|y| {
                (0..width).map(move |x| {
                    let x = if width > 1 {
                        unit_interval(x, width - 1)
                    } else {
                        0.0
                    };
                    let y = if height > 1 {
                        unit_interval(y, height - 1)
                    } else {
                        0.0
                    };
                    [x, y, (x + y) * 0.5]
                })
            })
            .collect();
        let source = Image::new(width, height, pixels, ImageContract::SRGB_DISPLAY).unwrap();

        let (rendered, report) = Pipeline::default().render(source).unwrap();

        assert_eq!([rendered.width(), rendered.height()], [width, height]);
        assert_eq!(rendered.pixels().len(), (width * height) as usize);
        assert_eq!(rendered.contract(), ImageContract::SRGB_DISPLAY);
        assert!(
            rendered
                .pixels()
                .iter()
                .flatten()
                .all(|value| value.is_finite() && (0.0..=1.0).contains(value))
        );
        assert_eq!(report.stages.len(), 15);
    }
}

#[test]
fn preview_and_export_match_for_a_multi_pixel_gradient() {
    let width = 17;
    let height = 3;
    let pixels = (0..height)
        .flat_map(|y| {
            (0..width).map(move |x| {
                let red = unit_interval(x, width - 1);
                let green = unit_interval(y, height - 1);
                [red, green, (red * 0.25 + green * 0.75)]
            })
        })
        .collect();
    let source = Image::new(width, height, pixels, ImageContract::SRGB_DISPLAY).unwrap();
    let pipeline = Pipeline::default();

    let preview = pipeline
        .render_with_context(
            source.clone(),
            &RenderContext::new(RenderQuality::Preview),
            &mut |_progress| {},
        )
        .unwrap()
        .0;
    let export = pipeline
        .render_with_context(
            source,
            &RenderContext::new(RenderQuality::Export),
            &mut |_progress| {},
        )
        .unwrap()
        .0;

    assert_eq!(preview, export);
}
