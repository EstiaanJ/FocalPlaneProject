use std::{
    path::{Path, PathBuf},
    sync::{
        Arc,
        mpsc::{self, Receiver, Sender},
    },
};

use crate::vectorscope::{
    AnalysisRegion, DensityScale, SCOPE_RESOLUTION, ScopeSpace, VectorscopeAnalysis, analyse,
    analyse_region_in_space, render_reverse_highlight,
};

pub struct LoadedImage {
    pub path: PathBuf,
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    pub scope: VectorscopeAnalysis,
    pub cie_scope: VectorscopeAnalysis,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum AnalysisKind {
    Hover,
    Rectangle,
}

enum LoadRequest {
    Open {
        id: u64,
        path: PathBuf,
    },
    Analyse {
        id: u64,
        image: Arc<LoadedImage>,
        region: AnalysisRegion,
        kind: AnalysisKind,
        space: ScopeSpace,
    },
    Highlight {
        id: u64,
        image: Arc<LoadedImage>,
        centre: [f32; 2],
        radius: f32,
        space: ScopeSpace,
        density_scale: DensityScale,
    },
}

pub enum LoadEvent {
    Loaded {
        id: u64,
        image: LoadedImage,
    },
    Analysed {
        id: u64,
        analysis: VectorscopeAnalysis,
        kind: AnalysisKind,
        space: ScopeSpace,
    },
    Highlighted {
        id: u64,
        rgba: Vec<u8>,
        width: u32,
        height: u32,
    },
    Failed {
        id: u64,
        message: String,
    },
}

pub struct ImageLoader {
    requests: Sender<LoadRequest>,
    events: Receiver<LoadEvent>,
    next_id: u64,
}

impl ImageLoader {
    pub fn new() -> Self {
        let (request_sender, request_receiver) = mpsc::channel::<LoadRequest>();
        let (event_sender, event_receiver) = mpsc::channel::<LoadEvent>();

        std::thread::Builder::new()
            .name("vectorscope-image-loader".to_owned())
            .spawn(move || {
                let mut pending = Vec::new();
                loop {
                    if pending.is_empty() {
                        let Ok(request) = request_receiver.recv() else {
                            break;
                        };
                        pending.push(request);
                    }
                    while let Ok(newer) = request_receiver.try_recv() {
                        match newer {
                            LoadRequest::Open { .. } => {
                                pending.clear();
                                pending.push(newer);
                            }
                            LoadRequest::Analyse { kind, .. } => {
                                if let Some(index) = pending.iter().position(|request| {
                                    matches!(request, LoadRequest::Analyse {
                                        kind: pending_kind,
                                        ..
                                    } if *pending_kind == kind)
                                }) {
                                    pending[index] = newer;
                                } else {
                                    pending.push(newer);
                                }
                            }
                            LoadRequest::Highlight { .. } => {
                                if let Some(index) = pending.iter().position(|request| {
                                    matches!(request, LoadRequest::Highlight { .. })
                                }) {
                                    pending[index] = newer;
                                } else {
                                    pending.push(newer);
                                }
                            }
                        }
                    }

                    let open_index = pending
                        .iter()
                        .position(|request| matches!(request, LoadRequest::Open { .. }));
                    let request_index = open_index
                        .or_else(|| {
                            pending.iter().position(|request| {
                                matches!(
                                    request,
                                    LoadRequest::Analyse {
                                        kind: AnalysisKind::Rectangle,
                                        ..
                                    }
                                )
                            })
                        })
                        .unwrap_or(0);
                    let request = pending.swap_remove(request_index);
                    if open_index.is_some() {
                        pending.clear();
                    }

                    if !process_request(request, &event_sender) {
                        break;
                    }
                }
            })
            .expect("spawn vectorscope image loader");

        Self {
            requests: request_sender,
            events: event_receiver,
            next_id: 0,
        }
    }

    pub fn open(&mut self, path: PathBuf) -> u64 {
        self.next_id += 1;
        let id = self.next_id;
        let _ = self.requests.send(LoadRequest::Open { id, path });
        id
    }

    pub fn analyse(
        &mut self,
        image: Arc<LoadedImage>,
        region: AnalysisRegion,
        kind: AnalysisKind,
        space: ScopeSpace,
    ) -> u64 {
        self.next_id += 1;
        let id = self.next_id;
        let _ = self.requests.send(LoadRequest::Analyse {
            id,
            image,
            region,
            kind,
            space,
        });
        id
    }

    pub fn highlight(
        &mut self,
        image: Arc<LoadedImage>,
        centre: [f32; 2],
        radius: f32,
        space: ScopeSpace,
        density_scale: DensityScale,
    ) -> u64 {
        self.next_id += 1;
        let id = self.next_id;
        let _ = self.requests.send(LoadRequest::Highlight {
            id,
            image,
            centre,
            radius,
            space,
            density_scale,
        });
        id
    }

    pub fn poll(&self) -> Vec<LoadEvent> {
        self.events.try_iter().collect()
    }
}

fn process_request(request: LoadRequest, event_sender: &Sender<LoadEvent>) -> bool {
    let event = match request {
        LoadRequest::Open { id, path } => match load_image(&path) {
            Ok(image) => LoadEvent::Loaded { id, image },
            Err(message) => LoadEvent::Failed { id, message },
        },
        LoadRequest::Analyse {
            id,
            image,
            region,
            kind,
            space,
        } => LoadEvent::Analysed {
            id,
            analysis: analyse_region_in_space(
                &image.rgba,
                image.width,
                image.height,
                SCOPE_RESOLUTION,
                Some(region),
                space,
            ),
            kind,
            space,
        },
        LoadRequest::Highlight {
            id,
            image,
            centre,
            radius,
            space,
            density_scale,
        } => LoadEvent::Highlighted {
            id,
            rgba: render_reverse_highlight(
                &image.rgba,
                image.width,
                image.height,
                centre,
                radius,
                space,
                density_scale,
            ),
            width: image.width,
            height: image.height,
        },
    };
    event_sender.send(event).is_ok()
}

fn load_image(path: &Path) -> Result<LoadedImage, String> {
    let decoded = image::ImageReader::open(path)
        .map_err(|error| format!("Could not open {}: {error}", path.display()))?
        .with_guessed_format()
        .map_err(|error| format!("Could not identify {}: {error}", path.display()))?
        .decode()
        .map_err(|error| format!("Could not decode {}: {error}", path.display()))?;
    let rgba = decoded.to_rgba8();
    let (width, height) = rgba.dimensions();
    let rgba = rgba.into_raw();
    let scope = analyse(&rgba, width, height, SCOPE_RESOLUTION);
    let cie_scope = crate::vectorscope::analyse_cie1931(&rgba, width, height, SCOPE_RESOLUTION);

    Ok(LoadedImage {
        path: path.to_owned(),
        width,
        height,
        rgba,
        scope,
        cie_scope,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageBuffer, Rgb, codecs::jpeg::JpegEncoder};
    use std::{thread, time::Duration};

    fn tiny_image() -> Arc<LoadedImage> {
        Arc::new(LoadedImage {
            path: PathBuf::from("test.png"),
            width: 2,
            height: 2,
            rgba: vec![255; 16],
            scope: VectorscopeAnalysis {
                space: ScopeSpace::Ryb,
                resolution: 2,
                density: vec![0.0; 4],
                colours: vec![[0.0; 3]; 4],
                sampled_pixels: 0,
            },
            cie_scope: VectorscopeAnalysis {
                space: ScopeSpace::Cie1931,
                resolution: 2,
                density: vec![0.0; 4],
                colours: vec![[0.0; 3]; 4],
                sampled_pixels: 0,
            },
        })
    }

    #[test]
    fn pending_hover_and_rectangle_requests_are_both_processed() {
        let mut loader = ImageLoader::new();
        let image = tiny_image();
        let rectangle_id = loader.analyse(
            image.clone(),
            AnalysisRegion::Rectangle {
                min: [0.0, 0.0],
                max: [1.0, 1.0],
            },
            AnalysisKind::Rectangle,
            ScopeSpace::Ryb,
        );
        let hover_id = loader.analyse(
            image,
            AnalysisRegion::Circle {
                centre: [0.5, 0.5],
                radius: 0.5,
            },
            AnalysisKind::Hover,
            ScopeSpace::Ryb,
        );

        let mut saw_rectangle = false;
        let mut saw_hover = false;
        for _ in 0..2000 {
            for event in loader.poll() {
                if let LoadEvent::Analysed { id, .. } = event {
                    saw_rectangle |= id == rectangle_id;
                    saw_hover |= id == hover_id;
                }
            }
            if saw_rectangle && saw_hover {
                break;
            }
            thread::sleep(Duration::from_millis(1));
        }

        assert!(saw_rectangle, "rectangle analysis was dropped");
        assert!(saw_hover, "hover analysis was dropped");
    }

    #[test]
    #[ignore = "known bug FP-PLOTS-003: JPEG EXIF orientation is ignored"]
    fn jpeg_exif_orientation_is_applied_before_scope_analysis() {
        let image = ImageBuffer::from_fn(2, 1, |x, _| {
            if x == 0 {
                Rgb([255_u8, 0, 0])
            } else {
                Rgb([0_u8, 0, 255])
            }
        });
        let mut jpeg = Vec::new();
        JpegEncoder::new_with_quality(&mut jpeg, 90)
            .encode_image(&image)
            .expect("encode JPEG fixture");

        let tiff = [
            b'I', b'I', 42, 0, 8, 0, 0, 0, // TIFF header and IFD offset
            1, 0, // one IFD entry
            0x12, 0x01, 3, 0, 1, 0, 0, 0, 6, 0, 0, 0, // rotate 90° clockwise
            0, 0, 0, 0, // next IFD
        ];
        let mut exif = b"Exif\0\0".to_vec();
        exif.extend_from_slice(&tiff);
        let segment_length = u16::try_from(exif.len() + 2).expect("small EXIF fixture");
        let mut with_exif = vec![0xFF, 0xD8, 0xFF, 0xE1];
        with_exif.extend_from_slice(&segment_length.to_be_bytes());
        with_exif.extend_from_slice(&exif);
        with_exif.extend_from_slice(&jpeg[2..]);

        let path = std::env::temp_dir().join(format!(
            "better-plots-orientation-regression-{}.jpg",
            std::process::id()
        ));
        std::fs::write(&path, with_exif).expect("write temporary JPEG fixture");
        let loaded = load_image(&path).expect("oriented JPEG decodes");
        let _ = std::fs::remove_file(path);

        assert_eq!(
            (loaded.width, loaded.height),
            (1, 2),
            "the displayed image and its scope must use the EXIF display orientation"
        );
    }
}
