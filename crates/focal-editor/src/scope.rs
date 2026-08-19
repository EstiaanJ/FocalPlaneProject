use std::sync::mpsc::{self, Receiver, Sender};

use focal_core::Image;
use focal_plot::vectorscope::{
    SCOPE_RESOLUTION, ScopeSpace, VectorscopeAnalysis, analyse_region_in_space,
};

pub struct ScopeRequest {
    pub generation: u64,
    pub image: Image,
}

pub struct ScopeResult {
    pub generation: u64,
    pub cie1931: VectorscopeAnalysis,
    pub ryb: VectorscopeAnalysis,
}

pub fn spawn() -> (Sender<ScopeRequest>, Receiver<ScopeResult>) {
    let (request_sender, request_receiver) = mpsc::channel::<ScopeRequest>();
    let (result_sender, result_receiver) = mpsc::channel();
    std::thread::Builder::new()
        .name("focal-editor-scopes".to_owned())
        .spawn(move || {
            while let Ok(mut request) = request_receiver.recv() {
                while let Ok(newer) = request_receiver.try_recv() {
                    request = newer;
                }
                let rgba = display_rgba(&request.image);
                let width = request.image.width();
                let height = request.image.height();
                let cie1931 = analyse_region_in_space(
                    &rgba,
                    width,
                    height,
                    SCOPE_RESOLUTION,
                    None,
                    ScopeSpace::Cie1931,
                );
                let ryb = analyse_region_in_space(
                    &rgba,
                    width,
                    height,
                    SCOPE_RESOLUTION,
                    None,
                    ScopeSpace::Ryb,
                );
                if result_sender
                    .send(ScopeResult {
                        generation: request.generation,
                        cie1931,
                        ryb,
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .expect("scope worker thread should start");
    (request_sender, result_receiver)
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
}
