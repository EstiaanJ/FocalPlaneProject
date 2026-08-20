#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::too_many_lines
)]

use std::{env, error::Error, path::Path};

use focal_core::{Image, ImageContract, ModuleParameters, Pipeline};
use image::{ImageBuffer, ImageReader, Rgb};

#[derive(Clone, Copy)]
struct Edit {
    name: &'static str,
    exposure: f32,
    contrast: f32,
    saturation: f32,
}

fn main() -> Result<(), Box<dyn Error>> {
    let set = Path::new("test-image/RTSet");
    let results = set.join("results");
    std::fs::create_dir_all(&results)?;
    let source_path = env::args()
        .nth(1)
        .map_or_else(|| set.join("original.jpg"), Into::into);
    let prefix = env::args().nth(2).unwrap_or_else(|| "focalcore".to_owned());

    let decoded = ImageReader::open(source_path)?
        .with_guessed_format()?
        .decode()?
        .to_rgb8();
    let (width, height) = decoded.dimensions();
    let pixels = decoded
        .pixels()
        .map(|pixel| pixel.0.map(|channel| f32::from(channel) / 255.0))
        .collect();
    let source = Image::new(width, height, pixels, ImageContract::SRGB_DISPLAY)?;

    let mut edits = vec![
        Edit {
            name: "baseline",
            exposure: 0.0,
            contrast: 0.0,
            saturation: 0.0,
        },
        Edit {
            name: "+0.5ev",
            exposure: 0.5,
            contrast: 0.0,
            saturation: 0.0,
        },
        Edit {
            name: "+1ev",
            exposure: 1.0,
            contrast: 0.0,
            saturation: 0.0,
        },
        Edit {
            name: "-1ev",
            exposure: -1.0,
            contrast: 0.0,
            saturation: 0.0,
        },
        Edit {
            name: "+20contrast",
            exposure: 0.0,
            contrast: 20.0,
            saturation: 0.0,
        },
        Edit {
            name: "+20Sat",
            exposure: 0.0,
            contrast: 0.0,
            saturation: 20.0,
        },
        Edit {
            name: "-20sat",
            exposure: 0.0,
            contrast: 0.0,
            saturation: -20.0,
        },
        Edit {
            name: "+51Cont",
            exposure: 0.0,
            contrast: 51.0,
            saturation: 0.0,
        },
        Edit {
            name: "+100Cont",
            exposure: 0.0,
            contrast: 100.0,
            saturation: 0.0,
        },
        Edit {
            name: "-100Cont",
            exposure: 0.0,
            contrast: -100.0,
            saturation: 0.0,
        },
        Edit {
            name: "+50sat",
            exposure: 0.0,
            contrast: 0.0,
            saturation: 50.0,
        },
        Edit {
            name: "+100sat",
            exposure: 0.0,
            contrast: 0.0,
            saturation: 100.0,
        },
        Edit {
            name: "-50sat",
            exposure: 0.0,
            contrast: 0.0,
            saturation: -50.0,
        },
        Edit {
            name: "-100sat",
            exposure: 0.0,
            contrast: 0.0,
            saturation: -100.0,
        },
        Edit {
            name: "-50Cont+30Sat",
            exposure: 0.0,
            contrast: -50.0,
            saturation: 30.0,
        },
        Edit {
            name: "-50Cont+60Sat",
            exposure: 0.0,
            contrast: -50.0,
            saturation: 60.0,
        },
        Edit {
            name: "original-target",
            exposure: -0.28,
            contrast: 22.0,
            saturation: -31.0,
        },
    ];
    if env::args().any(|argument| argument == "--sweep") {
        for (name, contrast) in [
            ("sweep-contrast-5", 5.0),
            ("sweep-contrast-10", 10.0),
            ("sweep-contrast-15", 15.0),
            ("sweep-contrast-25", 25.0),
            ("sweep-contrast-30", 30.0),
            ("sweep-contrast-40", 40.0),
            ("sweep-contrast-50", 50.0),
            ("sweep-contrast-60", 60.0),
            ("sweep-contrast-70", 70.0),
            ("sweep-contrast-80", 80.0),
            ("sweep-contrast-90", 90.0),
            ("sweep-contrast-minus-20", -20.0),
            ("sweep-contrast-minus-30", -30.0),
            ("sweep-contrast-minus-40", -40.0),
            ("sweep-contrast-minus-50", -50.0),
            ("sweep-contrast-minus-60", -60.0),
            ("sweep-contrast-minus-70", -70.0),
            ("sweep-contrast-minus-80", -80.0),
            ("sweep-contrast-minus-90", -90.0),
        ] {
            edits.push(Edit {
                name,
                exposure: 0.0,
                contrast,
                saturation: 0.0,
            });
        }
        for (name, saturation) in [
            ("sweep-saturation-minus-5", -5.0),
            ("sweep-saturation-minus-10", -10.0),
            ("sweep-saturation-minus-15", -15.0),
            ("sweep-saturation-minus-25", -25.0),
            ("sweep-saturation-minus-30", -30.0),
            ("sweep-saturation-minus-40", -40.0),
            ("sweep-saturation-minus-50", -50.0),
            ("sweep-saturation-minus-60", -60.0),
            ("sweep-saturation-minus-70", -70.0),
            ("sweep-saturation-plus-30", 30.0),
            ("sweep-saturation-plus-40", 40.0),
            ("sweep-saturation-plus-60", 60.0),
            ("sweep-saturation-plus-70", 70.0),
            ("sweep-saturation-plus-80", 80.0),
            ("sweep-saturation-plus-90", 90.0),
        ] {
            edits.push(Edit {
                name,
                exposure: 0.0,
                contrast: 0.0,
                saturation,
            });
        }
    }

    for edit in edits {
        let mut snapshot = Pipeline::default().snapshot();
        for module in &mut snapshot.modules {
            match &mut module.parameters {
                ModuleParameters::Exposure { stops } => *stops = edit.exposure,
                ModuleParameters::Contrast { amount } => *amount = edit.contrast,
                ModuleParameters::Saturation { amount } => *amount = edit.saturation,
                _ => {}
            }
        }
        let (rendered, _) = Pipeline::from_snapshot(snapshot).render(source.clone())?;
        let bytes: Vec<u8> = rendered
            .pixels()
            .iter()
            .flat_map(|pixel| pixel.map(|channel| (channel.clamp(0.0, 1.0) * 255.0).round() as u8))
            .collect();
        let output: ImageBuffer<Rgb<u8>, _> =
            ImageBuffer::from_raw(width, height, bytes).ok_or("invalid rendered dimensions")?;
        output.save(results.join(format!("{prefix}-{}.png", edit.name)))?;
    }

    Ok(())
}
