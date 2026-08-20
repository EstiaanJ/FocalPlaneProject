use std::{
    path::PathBuf,
    sync::Arc,
    sync::mpsc::{self, Receiver, Sender},
};

use crate::pipeline::{
    AlphaPolicy, InputColourSpace, PipelineError, PreparedImage, SourceImage,
    decode_image_file_with_alpha, prepare, reprepare,
};

enum ImageRequest {
    Open {
        id: u64,
        path: PathBuf,
        alpha_policy: AlphaPolicy,
    },
    Reprepare {
        id: u64,
        current: Arc<PreparedImage>,
        colour_space: InputColourSpace,
    },
    #[cfg(test)]
    Block {
        started: Sender<()>,
        release: Receiver<()>,
    },
}

pub enum ImageEvent {
    Opened {
        id: u64,
        path: PathBuf,
        source: Arc<SourceImage>,
        prepared: PreparedImage,
    },
    Reprepared {
        id: u64,
        prepared: PreparedImage,
    },
    Failed {
        id: u64,
        message: String,
    },
    TransparencyConfirmationRequired {
        id: u64,
        path: PathBuf,
    },
}

pub struct ImageLoader {
    requests: Sender<ImageRequest>,
    events: Receiver<ImageEvent>,
    next_id: u64,
}

impl ImageLoader {
    pub fn new() -> Self {
        let (request_sender, request_receiver) = mpsc::channel::<ImageRequest>();
        let (event_sender, event_receiver) = mpsc::channel::<ImageEvent>();

        std::thread::Builder::new()
            .name("curve-image-loader".to_owned())
            .spawn(move || {
                while let Ok(request) = request_receiver.recv() {
                    let events = event_sender.clone();
                    std::thread::Builder::new()
                        .name("curve-image-request".to_owned())
                        .spawn(move || process_request(request, &events))
                        .expect("spawn independent image request");
                }
            })
            .expect("spawn image loader");

        Self {
            requests: request_sender,
            events: event_receiver,
            next_id: 0,
        }
    }

    pub fn open(&mut self, path: PathBuf) -> u64 {
        self.open_with_alpha_policy(path, AlphaPolicy::RejectTransparency)
    }

    pub fn open_confirmed_flatten(&mut self, path: PathBuf) -> u64 {
        self.open_with_alpha_policy(path, AlphaPolicy::FlattenOver([1.0; 3]))
    }

    fn open_with_alpha_policy(&mut self, path: PathBuf, alpha_policy: AlphaPolicy) -> u64 {
        self.next_id += 1;
        let id = self.next_id;
        let _ = self.requests.send(ImageRequest::Open {
            id,
            path,
            alpha_policy,
        });
        id
    }

    pub fn reprepare(
        &mut self,
        current: Arc<PreparedImage>,
        colour_space: InputColourSpace,
    ) -> u64 {
        self.next_id += 1;
        let id = self.next_id;
        let _ = self.requests.send(ImageRequest::Reprepare {
            id,
            current,
            colour_space,
        });
        id
    }

    pub fn poll(&self) -> Vec<ImageEvent> {
        self.events.try_iter().collect()
    }
}

fn process_request(request: ImageRequest, event_sender: &Sender<ImageEvent>) {
    match request {
        ImageRequest::Open {
            id,
            path,
            alpha_policy,
        } => match decode_image_file_with_alpha(&path, alpha_policy) {
            Ok(source) => {
                let source = Arc::new(source);
                let colour_space = source
                    .profile
                    .detected_colour_space
                    .unwrap_or(InputColourSpace::Srgb);
                let event = match prepare(&source, colour_space) {
                    Ok(prepared) => ImageEvent::Opened {
                        id,
                        path,
                        source,
                        prepared,
                    },
                    Err(error) => ImageEvent::Failed {
                        id,
                        message: error.to_string(),
                    },
                };
                let _ = event_sender.send(event);
            }
            Err(PipelineError::TransparencyNeedsConfirmation) => {
                let _ =
                    event_sender.send(ImageEvent::TransparencyConfirmationRequired { id, path });
            }
            Err(error) => {
                let _ = event_sender.send(ImageEvent::Failed {
                    id,
                    message: error.to_string(),
                });
            }
        },
        ImageRequest::Reprepare {
            id,
            current,
            colour_space,
        } => {
            let event = match reprepare(&current, colour_space) {
                Ok(prepared) => ImageEvent::Reprepared { id, prepared },
                Err(error) => ImageEvent::Failed {
                    id,
                    message: error.to_string(),
                },
            };
            let _ = event_sender.send(event);
        }
        #[cfg(test)]
        ImageRequest::Block { started, release } => {
            let _ = started.send(());
            let _ = release.recv();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::{EmbeddedProfile, InputFormat};
    use std::time::Duration;

    #[test]
    fn an_active_obsolete_request_does_not_block_a_new_request() {
        let mut loader = ImageLoader::new();
        let (started_sender, started_receiver) = mpsc::channel();
        let (release_sender, release_receiver) = mpsc::channel();
        loader
            .requests
            .send(ImageRequest::Block {
                started: started_sender,
                release: release_receiver,
            })
            .unwrap();
        started_receiver
            .recv_timeout(Duration::from_secs(1))
            .expect("blocking request started");

        let prepared = Arc::new(PreparedImage {
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
        });
        let id = loader.reprepare(prepared, InputColourSpace::Srgb);
        let event = loader
            .events
            .recv_timeout(Duration::from_secs(1))
            .expect("new request completes independently");
        assert!(matches!(event, ImageEvent::Reprepared { id: event_id, .. } if event_id == id));
        let _ = release_sender.send(());
    }
}
