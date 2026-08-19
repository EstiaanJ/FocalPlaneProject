use std::sync::mpsc::{self, Receiver, Sender};

use focal_core::{Image, ModuleParameters, Pipeline, PipelineError};

#[derive(Debug)]
pub struct PreviewRequest {
    pub generation: u64,
    pub image: Image,
    pub exposure_stops: f32,
    pub contrast: f32,
}

#[derive(Debug)]
pub struct PreviewResult {
    pub generation: u64,
    pub image: Result<Image, PipelineError>,
}

/// Starts the latest-request-wins preview worker.
pub fn spawn() -> (Sender<PreviewRequest>, Receiver<PreviewResult>) {
    let (request_sender, request_receiver) = mpsc::channel::<PreviewRequest>();
    let (result_sender, result_receiver) = mpsc::channel();

    std::thread::Builder::new()
        .name("focal-editor-preview".to_owned())
        .spawn(move || {
            while let Ok(mut request) = request_receiver.recv() {
                request = newest_queued(request, &request_receiver);

                let result = render(request);
                if result_sender.send(result).is_err() {
                    break;
                }
            }
        })
        .expect("preview worker thread should start");

    (request_sender, result_receiver)
}

fn newest_queued(
    mut request: PreviewRequest,
    request_receiver: &Receiver<PreviewRequest>,
) -> PreviewRequest {
    while let Ok(newer) = request_receiver.try_recv() {
        request = newer;
    }
    request
}

fn render(request: PreviewRequest) -> PreviewResult {
    let mut snapshot = Pipeline::default().snapshot();
    for module in &mut snapshot.modules {
        match &mut module.parameters {
            ModuleParameters::Exposure { stops } => *stops = request.exposure_stops,
            ModuleParameters::Contrast { amount } => *amount = request.contrast,
            _ => {}
        }
    }

    let pipeline = Pipeline::from_snapshot(snapshot);
    let image = pipeline.render(request.image).map(|(image, _report)| image);
    PreviewResult {
        generation: request.generation,
        image,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use focal_core::ImageContract;

    #[test]
    fn worker_returns_the_requested_generation() {
        let (sender, receiver) = spawn();
        sender
            .send(PreviewRequest {
                generation: 7,
                image: Image::new(1, 1, vec![[0.5; 3]], ImageContract::SRGB_DISPLAY).unwrap(),
                exposure_stops: 0.0,
                contrast: 0.0,
            })
            .unwrap();

        let result = receiver.recv().unwrap();
        assert_eq!(result.generation, 7);
        assert!(result.image.is_ok());
    }

    #[test]
    fn newest_queued_request_is_preferred() {
        let (sender, receiver) = mpsc::channel();
        for generation in [1, 2, 3] {
            sender
                .send(PreviewRequest {
                    generation,
                    image: Image::new(1, 1, vec![[0.5; 3]], ImageContract::SRGB_DISPLAY).unwrap(),
                    exposure_stops: 0.0,
                    contrast: 0.0,
                })
                .unwrap();
        }

        let first = receiver.recv().unwrap();
        assert_eq!(newest_queued(first, &receiver).generation, 3);
    }
}
