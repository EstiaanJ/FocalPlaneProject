use std::sync::{
    Arc, Mutex,
    mpsc::{self, Receiver, Sender},
};

use focal_core::{
    CancellationToken, Image,
    scope::{
        SCOPE_RESOLUTION, ScopeInputContract, ScopeSpace, VectorscopeAnalysis,
        try_analyse_region_in_space,
    },
};

#[derive(Debug)]
pub struct ScopeRequest {
    pub generation: u64,
    pub image: Image,
}

#[derive(Clone)]
pub struct ScopeResult {
    pub generation: u64,
    pub cie1931: VectorscopeAnalysis,
    pub ryb: VectorscopeAnalysis,
}

struct WorkItem {
    request: ScopeRequest,
    cancellation: CancellationToken,
}

#[derive(Clone)]
pub struct ScopeWorker {
    sender: Sender<WorkItem>,
    active: Arc<Mutex<Option<CancellationToken>>>,
}

impl ScopeWorker {
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

    pub fn submit(&self, request: ScopeRequest) -> Result<(), ScopeRequest> {
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

pub fn spawn() -> (ScopeWorker, Receiver<ScopeResult>) {
    let (request_sender, request_receiver) = mpsc::channel::<WorkItem>();
    let (result_sender, result_receiver) = mpsc::channel();
    let active = Arc::new(Mutex::new(None));
    std::thread::Builder::new()
        .name("focal-editor-scopes".to_owned())
        .spawn(move || {
            while let Ok(mut work) = request_receiver.recv() {
                while let Ok(newer) = request_receiver.try_recv() {
                    work.cancellation.cancel();
                    work = newer;
                }
                if let Some(result) = analyse(&work.request, &work.cancellation)
                    && result_sender.send(result).is_err()
                {
                    break;
                }
            }
        })
        .expect("scope worker thread should start");
    (
        ScopeWorker {
            sender: request_sender,
            active,
        },
        result_receiver,
    )
}

fn analyse(request: &ScopeRequest, cancellation: &CancellationToken) -> Option<ScopeResult> {
    let rgba = display_rgba(&request.image);
    let analyse_space = |space| {
        try_analyse_region_in_space(
            &rgba,
            request.image.width(),
            request.image.height(),
            SCOPE_RESOLUTION,
            None,
            space,
            ScopeInputContract::EncodedSrgb8,
            cancellation,
        )
        .ok()
    };
    let cie1931 = analyse_space(ScopeSpace::Cie1931)?;
    let ryb = analyse_space(ScopeSpace::Ryb)?;
    Some(ScopeResult {
        generation: request.generation,
        cie1931,
        ryb,
    })
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn display_rgba(image: &Image) -> Vec<u8> {
    image
        .pixels()
        .iter()
        .flat_map(|pixel| {
            pixel
                .iter()
                .map(|channel| (channel.clamp(0.0, 1.0) * 255.0).round() as u8)
                .chain(std::iter::once(u8::MAX))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use focal_core::ImageContract;

    #[test]
    fn scope_adapter_preserves_display_rgb_and_adds_opaque_alpha() {
        let image = Image::new(1, 1, vec![[0.0, 0.5, 1.0]], ImageContract::SRGB_DISPLAY).unwrap();
        assert_eq!(display_rgba(&image), [0, 128, 255, 255]);
    }

    #[test]
    fn submitting_a_new_scope_cancels_the_active_scan() {
        let (sender, _receiver) = mpsc::channel();
        let active = Arc::new(Mutex::new(None));
        let worker = ScopeWorker {
            sender,
            active: Arc::clone(&active),
        };
        let image = || Image::new(1, 1, vec![[0.5; 3]], ImageContract::SRGB_DISPLAY).unwrap();
        worker
            .submit(ScopeRequest {
                generation: 1,
                image: image(),
            })
            .unwrap();
        let first = active.lock().unwrap().as_ref().unwrap().clone();
        worker
            .submit(ScopeRequest {
                generation: 2,
                image: image(),
            })
            .unwrap();
        assert!(first.is_cancelled());
    }
}
