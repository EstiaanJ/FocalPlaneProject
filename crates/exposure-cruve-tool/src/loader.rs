use std::{
    path::PathBuf,
    sync::Arc,
    sync::mpsc::{self, Receiver, Sender},
};

use crate::pipeline::{
    InputColourSpace, PreparedImage, SourceImage, decode_image_file, prepare, reprepare,
};

enum ImageRequest {
    Open {
        id: u64,
        path: PathBuf,
    },
    Reprepare {
        id: u64,
        current: Arc<PreparedImage>,
        colour_space: InputColourSpace,
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
                while let Ok(mut request) = request_receiver.recv() {
                    // If the user picked another file while the first was
                    // waiting, only the newest complete request matters.
                    while let Ok(newer) = request_receiver.try_recv() {
                        request = newer;
                    }

                    match request {
                        ImageRequest::Open { id, path } => match decode_image_file(&path) {
                            Ok(source) => {
                                let source = Arc::new(source);
                                let colour_space = source
                                    .profile
                                    .detected_colour_space
                                    .unwrap_or(InputColourSpace::Srgb);
                                let prepared = prepare(&source, colour_space);
                                let _ = event_sender.send(ImageEvent::Opened {
                                    id,
                                    path,
                                    source,
                                    prepared,
                                });
                            }
                            Err(error) => {
                                let _ = event_sender.send(ImageEvent::Failed {
                                    id,
                                    message: format!("{error}"),
                                });
                            }
                        },
                        ImageRequest::Reprepare {
                            id,
                            current,
                            colour_space,
                        } => {
                            let prepared = reprepare(&current, colour_space);
                            let _ = event_sender.send(ImageEvent::Reprepared { id, prepared });
                        }
                    }
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
        self.next_id += 1;
        let id = self.next_id;
        let _ = self.requests.send(ImageRequest::Open { id, path });
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
