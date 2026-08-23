use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc::{self, Receiver, Sender},
};

use crate::pipeline::{PipelineSnapshot, PreparedImage, RenderedPreview, render};

struct RenderRequest {
    id: u64,
    snapshot: PipelineSnapshot,
    cancelled: Arc<AtomicBool>,
}

pub enum RenderEvent {
    Progress { id: u64, fraction: f32 },
    Finished { id: u64, preview: RenderedPreview },
}

pub struct PreviewWorker {
    requests: Sender<RenderRequest>,
    events: Receiver<RenderEvent>,
    next_id: u64,
    latest_id: u64,
    active_cancel: Option<Arc<AtomicBool>>,
}

impl PreviewWorker {
    pub fn new(prepared: Arc<PreparedImage>) -> Self {
        let (request_sender, request_receiver) = mpsc::channel::<RenderRequest>();
        let (event_sender, event_receiver) = mpsc::channel::<RenderEvent>();

        std::thread::Builder::new()
            .name("curve-preview".to_owned())
            .spawn(move || {
                while let Ok(mut request) = request_receiver.recv() {
                    // Collapse queued stale snapshots before starting another
                    // render. The active render is cancelled by the sender.
                    while let Ok(newer) = request_receiver.try_recv() {
                        request = newer;
                    }
                    let id = request.id;
                    let cancelled = request.cancelled.clone();
                    let event_sender = &event_sender;
                    let result = render(&prepared, &request.snapshot, &cancelled, |fraction| {
                        let _ = event_sender.send(RenderEvent::Progress { id, fraction });
                    });
                    if let Some(preview) = result {
                        let _ = event_sender.send(RenderEvent::Finished { id, preview });
                    }
                }
            })
            .expect("spawn preview worker");

        Self {
            requests: request_sender,
            events: event_receiver,
            next_id: 0,
            latest_id: 0,
            active_cancel: None,
        }
    }

    pub fn request(&mut self, snapshot: PipelineSnapshot) -> u64 {
        if let Some(cancelled) = &self.active_cancel {
            cancelled.store(true, Ordering::Relaxed);
        }
        self.next_id += 1;
        self.latest_id = self.next_id;
        let cancelled = Arc::new(AtomicBool::new(false));
        self.active_cancel = Some(cancelled.clone());
        let request = RenderRequest {
            id: self.latest_id,
            snapshot,
            cancelled,
        };
        let _ = self.requests.send(request);
        self.latest_id
    }

    pub fn poll(&self) -> Vec<RenderEvent> {
        self.events.try_iter().collect()
    }

    pub fn cancel_active(&mut self) -> bool {
        let Some(cancelled) = self.active_cancel.take() else {
            return false;
        };
        cancelled.store(true, Ordering::Relaxed);
        true
    }
}

impl Drop for PreviewWorker {
    fn drop(&mut self) {
        self.cancel_active();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::curve::{CurveInterpolation, CurveMode, CurveSet, LuminanceDefinition};
    use crate::pipeline::{
        EmbeddedProfile, HistogramCalculation, InputColourSpace, InputFormat, PipelineSnapshot,
        PreparedImage,
    };

    fn prepared() -> Arc<PreparedImage> {
        Arc::new(PreparedImage {
            width: 1,
            height: 1,
            curve_domain: vec![[0.5; 3]],
            before_rgba: vec![128, 128, 128, 255],
            source_pixels: Arc::new(vec![[0.5; 3]]),
            profile: EmbeddedProfile {
                label: "test".to_owned(),
                byte_length: 0,
                detected_colour_space: Some(InputColourSpace::AdobeRgb),
                detection_source: "test".to_owned(),
            },
            format: InputFormat::Png,
            bit_depth: 8,
            input_colour_space: InputColourSpace::AdobeRgb,
        })
    }

    #[test]
    fn invalidating_a_preview_signals_its_active_cancellation_token() {
        let mut worker = PreviewWorker::new(prepared());
        worker.request(PipelineSnapshot {
            mode: CurveMode::LinkedRgb,
            curves: CurveSet::default(),
            luminance: LuminanceDefinition::AdobeRgb,
            interpolation: CurveInterpolation::Smooth,
            histogram_calculation: HistogramCalculation::FullResolution,
        });
        let token = worker.active_cancel.as_ref().unwrap().clone();
        assert!(worker.cancel_active());
        assert!(token.load(Ordering::Relaxed));
        assert!(!worker.cancel_active());
    }
}
