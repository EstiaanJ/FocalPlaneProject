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
}

impl Drop for PreviewWorker {
    fn drop(&mut self) {
        if let Some(cancelled) = &self.active_cancel {
            cancelled.store(true, Ordering::Relaxed);
        }
    }
}
