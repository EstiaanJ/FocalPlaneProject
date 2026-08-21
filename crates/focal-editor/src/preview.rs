use std::sync::{
    Arc, Mutex,
    mpsc::{self, Receiver, Sender},
};

use focal_core::{
    CancellationToken, ClippingWarnings, CropSettings, Image, ModuleParameters, Pipeline,
    PipelineError, PipelineSnapshot, RenderContext, RenderProgress, RenderQuality,
};

#[derive(Debug)]
pub struct PreviewRequest {
    pub generation: u64,
    pub source: Arc<Image>,
    pub sampling: PreviewSampling,
    pub snapshot: PipelineSnapshot,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PreviewSampling {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub width: u32,
    pub height: u32,
}

impl PreviewSampling {
    pub const fn full(width: u32, height: u32) -> Self {
        Self {
            left: 0.0,
            top: 0.0,
            right: 1.0,
            bottom: 1.0,
            width,
            height,
        }
    }
}

#[derive(Debug)]
pub struct PreviewFrame {
    pub before: Image,
    pub after: Image,
    pub clipping: Option<ClippingWarnings>,
    pub sampling: PreviewSampling,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Adjustments {
    pub warmth: f32,
    pub tint: f32,
    pub exposure_stops: f32,
    pub contrast: f32,
    pub local_contrast_amount: f32,
    pub local_contrast_radius: f32,
    pub saturation: f32,
    pub noise_luminance: f32,
    pub noise_colour: f32,
    pub crop: Option<CropSettings>,
}

#[derive(Debug)]
struct WorkItem {
    request: PreviewRequest,
    cancellation: CancellationToken,
}

#[derive(Debug)]
pub enum PreviewEvent {
    Progress {
        generation: u64,
        progress: RenderProgress,
    },
    Complete {
        generation: u64,
        image: Result<PreviewFrame, PipelineError>,
    },
}

/// Request handle which immediately cancels the render it supersedes.
#[derive(Clone)]
pub struct PreviewWorker {
    sender: Sender<WorkItem>,
    active: Arc<Mutex<Option<CancellationToken>>>,
}

impl PreviewWorker {
    pub fn cancel(&self) {
        if let Some(active) = self
            .active
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
        {
            active.cancel();
        }
    }

    pub fn submit(&self, request: PreviewRequest) -> Result<(), PreviewRequest> {
        let cancellation = CancellationToken::new();
        let mut active = self
            .active
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(previous) = active.replace(cancellation.clone()) {
            previous.cancel();
        }
        drop(active);

        self.sender
            .send(WorkItem {
                request,
                cancellation,
            })
            .map_err(|error| error.0.request)
    }
}

/// Starts the latest-request-wins preview worker.
pub fn spawn() -> (PreviewWorker, Receiver<PreviewEvent>) {
    let (request_sender, request_receiver) = mpsc::channel::<WorkItem>();
    let (event_sender, event_receiver) = mpsc::channel();
    let active = Arc::new(Mutex::new(None));

    std::thread::Builder::new()
        .name("focal-editor-preview".to_owned())
        .spawn(move || {
            while let Ok(mut work) = request_receiver.recv() {
                work = newest_queued(work, &request_receiver);
                render(work, &event_sender);
            }
        })
        .expect("preview worker thread should start");

    (
        PreviewWorker {
            sender: request_sender,
            active,
        },
        event_receiver,
    )
}

fn newest_queued(mut work: WorkItem, request_receiver: &Receiver<WorkItem>) -> WorkItem {
    while let Ok(newer) = request_receiver.try_recv() {
        work.cancellation.cancel();
        work = newer;
    }
    work
}

fn render(work: WorkItem, event_sender: &Sender<PreviewEvent>) {
    let generation = work.request.generation;
    let mut snapshot = work.request.snapshot;
    let crop = snapshot.modules.iter_mut().find_map(|module| {
        if let ModuleParameters::OrientationAndCrop { crop } = &mut module.parameters {
            crop.take()
        } else {
            None
        }
    });
    let pipeline = Pipeline::from_snapshot(snapshot);
    let before_pipeline = Pipeline::default();
    let context = RenderContext::with_cancellation(RenderQuality::Preview, work.cancellation);
    let mut report_progress = |progress| {
        let _ = event_sender.send(PreviewEvent::Progress {
            generation,
            progress,
        });
    };
    let image = sample_image(&work.request.source, work.request.sampling, crop, &context).and_then(
        |sampled| {
            let (before, _) =
                before_pipeline.render_with_context(sampled.clone(), &context, &mut |_| {})?;
            pipeline
                .render_with_context(sampled, &context, &mut report_progress)
                .map(|(after, report)| PreviewFrame {
                    before,
                    after,
                    clipping: report.clipping,
                    sampling: work.request.sampling,
                })
        },
    );
    let _ = event_sender.send(PreviewEvent::Complete { generation, image });
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)]
fn sample_image(
    source: &Image,
    sampling: PreviewSampling,
    crop: Option<CropSettings>,
    context: &RenderContext,
) -> Result<Image, PipelineError> {
    let cancellation = context.cancellation_token();
    let width = sampling.width.max(1);
    let height = sampling.height.max(1);
    let source_width = source.width() as usize;
    let source_height = source.height() as usize;
    let mut pixels = Vec::with_capacity(width as usize * height as usize);
    for y in 0..height {
        if cancellation.is_cancelled() {
            return Err(PipelineError::Cancelled {
                module: None,
                module_index: None,
            });
        }
        let v = (y as f32 + 0.5) / height as f32;
        let output_y = sampling.top + v * (sampling.bottom - sampling.top);
        for x in 0..width {
            let u = (x as f32 + 0.5) / width as f32;
            let output_x = sampling.left + u * (sampling.right - sampling.left);
            let [normalised_x, normalised_y] = crop.map_or([output_x, output_y], |crop| {
                crop_source_position(
                    [output_x, output_y],
                    crop,
                    source.width() as f32 / source.height() as f32,
                )
            });
            let source_x = normalised_x * source.width() as f32 - 0.5;
            let source_y = normalised_y * source.height() as f32 - 0.5;
            let y0 = source_y.floor().clamp(0.0, (source_height - 1) as f32) as usize;
            let y1 = (y0 + 1).min(source_height - 1);
            let fy = (source_y - y0 as f32).clamp(0.0, 1.0);
            let x0 = source_x.floor().clamp(0.0, (source_width - 1) as f32) as usize;
            let x1 = (x0 + 1).min(source_width - 1);
            let fx = (source_x - x0 as f32).clamp(0.0, 1.0);
            let top = lerp(
                source.pixels()[y0 * source_width + x0],
                source.pixels()[y0 * source_width + x1],
                fx,
            );
            let bottom = lerp(
                source.pixels()[y1 * source_width + x0],
                source.pixels()[y1 * source_width + x1],
                fx,
            );
            pixels.push(lerp(top, bottom, fy));
        }
    }
    Image::new(width, height, pixels, source.contract()).map_err(|_| {
        PipelineError::NonFiniteOutput {
            module: focal_core::ModuleKind::InputTransform,
            module_index: 0,
        }
    })
}

fn crop_source_position(point: [f32; 2], crop: CropSettings, aspect: f32) -> [f32; 2] {
    let centre = [
        (crop.left + crop.right) * 0.5,
        (crop.top + crop.bottom) * 0.5,
    ];
    let crop_point = [
        crop.left + point[0] * (crop.right - crop.left),
        crop.top + point[1] * (crop.bottom - crop.top),
    ];
    let angle = -crop.rotation_degrees.to_radians();
    let (sin, cos) = angle.sin_cos();
    let x = (crop_point[0] - centre[0]) * aspect;
    let y = crop_point[1] - centre[1];
    [
        (cos * x - sin * y) / aspect + centre[0],
        sin * x + cos * y + centre[1],
    ]
}

fn lerp(a: [f32; 3], b: [f32; 3], amount: f32) -> [f32; 3] {
    std::array::from_fn(|channel| a[channel] + (b[channel] - a[channel]) * amount)
}

pub fn snapshot_with_adjustments(adjustments: Adjustments) -> PipelineSnapshot {
    let mut snapshot = Pipeline::default().snapshot();
    for module in &mut snapshot.modules {
        match &mut module.parameters {
            ModuleParameters::WhiteBalance { warmth, tint } => {
                *warmth = adjustments.warmth;
                *tint = adjustments.tint;
            }
            ModuleParameters::Exposure { stops } => *stops = adjustments.exposure_stops,
            ModuleParameters::Contrast { amount } => *amount = adjustments.contrast,
            ModuleParameters::LocalContrast { amount, radius } => {
                *amount = adjustments.local_contrast_amount;
                *radius = adjustments.local_contrast_radius;
            }
            ModuleParameters::Saturation { amount } => *amount = adjustments.saturation,
            ModuleParameters::NoiseReduction { luminance, colour } => {
                *luminance = adjustments.noise_luminance;
                *colour = adjustments.noise_colour;
            }
            ModuleParameters::OrientationAndCrop { crop } => *crop = adjustments.crop,
            _ => {}
        }
    }
    snapshot
}

#[cfg(test)]
mod tests {
    use super::*;
    use focal_core::ImageContract;

    fn request(generation: u64) -> PreviewRequest {
        PreviewRequest {
            generation,
            source: Arc::new(
                Image::new(1, 1, vec![[0.5; 3]], ImageContract::SRGB_DISPLAY).unwrap(),
            ),
            sampling: PreviewSampling::full(1, 1),
            snapshot: snapshot_with_adjustments(Adjustments {
                local_contrast_radius: 80.0,
                ..Adjustments::default()
            }),
        }
    }

    #[test]
    fn worker_returns_progress_and_completion_for_the_requested_generation() {
        let (worker, receiver) = spawn();
        worker.submit(request(7)).unwrap();

        let mut saw_progress = false;
        loop {
            match receiver.recv().unwrap() {
                PreviewEvent::Progress { generation, .. } => {
                    assert_eq!(generation, 7);
                    saw_progress = true;
                }
                PreviewEvent::Complete { generation, image } => {
                    assert_eq!(generation, 7);
                    assert!(image.is_ok());
                    break;
                }
            }
        }
        assert!(saw_progress);
    }

    #[test]
    fn submitting_a_new_request_cancels_the_previous_token() {
        let (sender, _receiver) = mpsc::channel();
        let active = Arc::new(Mutex::new(None));
        let worker = PreviewWorker {
            sender,
            active: Arc::clone(&active),
        };
        worker.submit(request(1)).unwrap();
        let first = active
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .as_ref()
            .unwrap()
            .clone();

        worker.submit(request(2)).unwrap();

        assert!(first.is_cancelled());
    }

    #[test]
    fn snapshot_carries_every_approved_adjustment_into_focal_core() {
        let adjustments = Adjustments {
            warmth: 12.0,
            tint: -4.0,
            exposure_stops: 1.5,
            contrast: 8.0,
            local_contrast_amount: 20.0,
            local_contrast_radius: 64.0,
            saturation: 14.0,
            noise_luminance: 10.0,
            noise_colour: 25.0,
            crop: Some(CropSettings {
                left: 0.25,
                top: 0.25,
                right: 0.75,
                bottom: 0.75,
                rotation_degrees: 0.0,
            }),
        };
        let snapshot = snapshot_with_adjustments(adjustments);

        assert!(snapshot.modules.iter().any(|module| {
            module.parameters
                == ModuleParameters::WhiteBalance {
                    warmth: 12.0,
                    tint: -4.0,
                }
        }));
        assert!(snapshot.modules.iter().any(|module| {
            matches!(
                module.parameters,
                ModuleParameters::OrientationAndCrop { crop: Some(_) }
            )
        }));
        assert!(
            snapshot.modules.iter().any(|module| {
                module.parameters == ModuleParameters::Saturation { amount: 14.0 }
            })
        );
        assert!(snapshot.modules.iter().any(|module| {
            module.parameters
                == ModuleParameters::LocalContrast {
                    amount: 20.0,
                    radius: 64.0,
                }
        }));
        assert!(snapshot.modules.iter().any(|module| {
            module.parameters
                == ModuleParameters::NoiseReduction {
                    luminance: 10.0,
                    colour: 25.0,
                }
        }));
    }

    #[test]
    fn preview_sampling_extracts_only_the_requested_source_region() {
        let source = Image::new(
            4,
            1,
            vec![[0.0; 3], [0.25; 3], [0.75; 3], [1.0; 3]],
            ImageContract::SRGB_DISPLAY,
        )
        .unwrap();
        let context = RenderContext::new(RenderQuality::Preview);
        let sampled = sample_image(
            &source,
            PreviewSampling {
                left: 0.5,
                top: 0.0,
                right: 1.0,
                bottom: 1.0,
                width: 2,
                height: 1,
            },
            None,
            &context,
        )
        .unwrap();

        assert_eq!(sampled.width(), 2);
        assert!(sampled.pixels()[0][0] > 0.6);
        assert!(sampled.pixels()[1][0] > sampled.pixels()[0][0]);
    }

    #[test]
    fn applied_crop_sampling_maps_the_visible_crop_region_back_to_the_source() {
        let source = Image::new(
            4,
            2,
            vec![
                [0.0; 3], [0.25; 3], [0.75; 3], [1.0; 3], [0.0; 3], [0.25; 3], [0.75; 3], [1.0; 3],
            ],
            ImageContract::SRGB_DISPLAY,
        )
        .unwrap();
        let context = RenderContext::new(RenderQuality::Preview);
        let sampled = sample_image(
            &source,
            PreviewSampling::full(2, 2),
            Some(CropSettings {
                left: 0.5,
                top: 0.0,
                right: 1.0,
                bottom: 1.0,
                rotation_degrees: 0.0,
            }),
            &context,
        )
        .unwrap();
        assert!(sampled.pixels().iter().all(|pixel| pixel[0] > 0.6));
    }

    #[test]
    fn explicit_cancel_stops_active_work_before_opening_another_image() {
        let (sender, _receiver) = mpsc::channel();
        let active = Arc::new(Mutex::new(None));
        let worker = PreviewWorker {
            sender,
            active: Arc::clone(&active),
        };
        worker.submit(request(1)).unwrap();
        let token = active
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .as_ref()
            .unwrap()
            .clone();

        worker.cancel();

        assert!(token.is_cancelled());
        assert!(
            active
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .is_none()
        );
    }

    #[test]
    fn newest_queued_request_is_preferred() {
        let (sender, receiver) = mpsc::channel();
        for generation in [1, 2, 3] {
            sender
                .send(WorkItem {
                    request: request(generation),
                    cancellation: CancellationToken::new(),
                })
                .unwrap();
        }

        let first = receiver.recv().unwrap();
        assert_eq!(newest_queued(first, &receiver).request.generation, 3);
    }
}
