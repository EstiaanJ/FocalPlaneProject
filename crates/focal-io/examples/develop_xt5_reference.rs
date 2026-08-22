use std::{env, path::Path};

use focal_core::CancellationToken;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let source = env::args()
        .nth(1)
        .ok_or("usage: develop_xt5_reference <image.RAF> <output.png>")?;
    let destination = env::args()
        .nth(2)
        .ok_or("usage: develop_xt5_reference <image.RAF> <output.png>")?;
    let rendered =
        focal_io::decode_xt5_camera_neutral(Path::new(&source), &CancellationToken::new())?;
    let image = image::RgbaImage::from_raw(rendered.width, rendered.height, rendered.rgba)
        .ok_or("developed image dimensions are invalid")?;
    image.save(Path::new(&destination))?;
    Ok(())
}
