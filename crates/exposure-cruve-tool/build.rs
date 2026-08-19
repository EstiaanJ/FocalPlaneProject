use std::{borrow::Cow, env, fs::File, path::PathBuf};

const WIDTH: u32 = 320;
const HEIGHT: u32 = 192;

fn main() {
    println!("cargo:rerun-if-changed=assets/adobe_rgb_1998.icc");

    let output_dir = PathBuf::from(env::var_os("OUT_DIR").expect("Cargo sets OUT_DIR"));
    let output_path = output_dir.join("controlled_adobe_rgb.png");
    let profile = include_bytes!("assets/adobe_rgb_1998.icc");

    let mut info = png::Info::with_size(WIDTH, HEIGHT);
    info.icc_profile = Some(Cow::Borrowed(profile));

    let file = File::create(output_path).expect("create controlled PNG fixture");
    let mut encoder = png::Encoder::with_info(file, info).expect("create PNG encoder");
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Sixteen);
    let mut writer = encoder.write_header().expect("write PNG header");

    let mut pixels = Vec::with_capacity((WIDTH * HEIGHT * 6) as usize);
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let xf = f32::from(u16::try_from(x).expect("fixture width fits u16"))
                / f32::from(u16::try_from(WIDTH - 1).expect("fixture width fits u16"));
            let yf = f32::from(u16::try_from(y).expect("fixture height fits u16"))
                / f32::from(u16::try_from(HEIGHT - 1).expect("fixture height fits u16"));
            let patch = (x / 40) % 8;
            let rgb = match patch {
                0 => [xf, xf, xf],
                1 => [xf, 0.12 + 0.55 * yf, 0.08],
                2 => [0.08, xf, 0.18 + 0.55 * yf],
                3 => [0.18 + 0.55 * yf, 0.08, xf],
                4 => [0.92 * xf, 0.84 * xf, 0.12],
                5 => [0.08, 0.78 * xf, 0.72 * xf],
                6 => [0.78 * xf, 0.10, 0.70 * xf],
                _ => [0.10 + 0.80 * xf, 0.10 + 0.80 * yf, 0.10 + 0.80 * (1.0 - xf)],
            };

            for channel in rgb {
                pixels.extend_from_slice(&fixture_channel(channel));
            }
        }
    }

    writer
        .write_image_data(&pixels)
        .expect("write controlled PNG fixture");
}

fn fixture_channel(channel: f32) -> [u8; 2] {
    let bounded = (channel.clamp(0.0, 1.0) * f32::from(u16::MAX)).round();
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let value = bounded as u16;
    value.to_be_bytes()
}
