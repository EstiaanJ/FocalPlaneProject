#![allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]

use std::{env, fs, path::Path};

use focal_core::CancellationToken;
use image::{GenericImageView, ImageBuffer, Rgb};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let raw_path = env::args()
        .nth(1)
        .ok_or("usage: crop_xt5_regions <image.RAF> <rectangles.csv> <output-directory>")?;
    let rectangles_path = env::args()
        .nth(2)
        .ok_or("usage: crop_xt5_regions <image.RAF> <rectangles.csv> <output-directory>")?;
    let output_path = env::args()
        .nth(3)
        .ok_or("usage: crop_xt5_regions <image.RAF> <rectangles.csv> <output-directory>")?;

    let rendered =
        focal_io::decode_xt5_camera_neutral(Path::new(&raw_path), &CancellationToken::new())?;
    let samples = rendered
        .pixels
        .iter()
        .flat_map(|pixel| {
            pixel.map(|value| (value.clamp(0.0, 1.0) * f32::from(u16::MAX)).round() as u16)
        })
        .collect();
    let developed =
        ImageBuffer::<Rgb<u16>, Vec<u16>>::from_raw(rendered.width, rendered.height, samples)
            .ok_or("developed image dimensions are invalid")?;
    if developed.width() != 7728 || developed.height() != 5152 {
        return Err(format!(
            "expected the X-T5 JPEG coordinate grid 7728x5152, found {}x{}",
            developed.width(),
            developed.height()
        )
        .into());
    }

    let output = Path::new(&output_path);
    fs::create_dir_all(output)?;
    let rectangles = fs::read_to_string(rectangles_path)?;
    let mut written = 0;
    for line in rectangles.lines().skip(1) {
        if line.trim().is_empty() {
            break;
        }
        let fields = line.split(',').collect::<Vec<_>>();
        if fields.len() < 5 {
            return Err(format!("invalid rectangle row: {line}").into());
        }
        let number = fields[0].parse::<usize>()?;
        let left = fields[1].parse::<f64>()?.floor() as u32;
        let top = fields[2].parse::<f64>()?.floor() as u32;
        let right = fields[3].parse::<f64>()?.ceil() as u32;
        let bottom = fields[4].parse::<f64>()?.ceil() as u32;
        if left >= right
            || top >= bottom
            || right > developed.width()
            || bottom > developed.height()
        {
            return Err(format!("rectangle {number} is outside the developed RAW").into());
        }
        let crop = developed
            .view(left, top, right - left, bottom - top)
            .to_image();
        crop.save(output.join(format!("camera-neutral-source-{number}.png")))?;
        written += 1;
    }
    if written != 38 {
        return Err(format!("expected 38 rectangles, wrote {written}").into());
    }
    println!(
        "wrote {written} developed RAW crops to {}",
        output.display()
    );
    Ok(())
}
