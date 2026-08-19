use std::sync::{
    Arc, Mutex,
    mpsc::{self, Receiver, Sender},
};

use focal_core::{
    CancellationToken, Image, ModuleParameters, Pipeline, PipelineError, PipelineSnapshot,
    RenderContext, RenderProgress, RenderQuality,
};

#[derive(Debug)]
pub struct PreviewRequest {
    pub generation: u64,
    pub source: Arc<Image>,
    pub snapshot: PipelineSnapshot,
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
        image: Result<Image, PipelineError>,
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
    let pipeline = Pipeline::from_snapshot(work.request.snapshot);
    let context = RenderContext::with_cancellation(RenderQuality::Preview, work.cancellation);
    let mut report_progress = |progress| {
        let _ = event_sender.send(PreviewEvent::Progress {
            generation,
            progress,
        });
    };
    let image = pipeline
        .render_with_context(
            (*work.request.source).clone(),
            &context,
            &mut report_progress,
        )
        .map(|(image, _report)| image);
    let _ = event_sender.send(PreviewEvent::Complete { generation, image });
}

pub fn snapshot_with_adjustments(exposure_stops: f32, contrast: f32) -> PipelineSnapshot {
    let mut snapshot = Pipeline::default().snapshot();
    for module in &mut snapshot.modules {
        match &mut module.parameters {
            ModuleParameters::Exposure { stops } => *stops = exposure_stops,
            ModuleParameters::Contrast { amount } => *amount = contrast,
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
            snapshot: snapshot_with_adjustments(0.0, 0.0),
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
