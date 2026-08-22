use std::{env, path::Path};

use focal_io::decode_xt5_raf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = env::args().nth(1).ok_or("usage: inspect_xt5 <image.RAF>")?;
    let image = decode_xt5_raf(Path::new(&path))?;
    println!("sensor={}x{}", image.width, image.height);
    println!("cfa={}x{}", image.cfa_width, image.cfa_height);
    println!("active_area={:?}", image.active_area);
    println!("crop_area={:?}", image.crop_area);
    println!("white_balance={:?}", image.white_balance);
    println!("xyz_to_camera={:?}", image.xyz_to_camera);
    let (minimum, maximum, sum) = image.samples.iter().fold(
        (f32::INFINITY, f32::NEG_INFINITY, 0.0_f64),
        |(minimum, maximum, sum), sample| {
            (
                minimum.min(*sample),
                maximum.max(*sample),
                sum + f64::from(*sample),
            )
        },
    );
    #[allow(clippy::cast_precision_loss)]
    let mean = sum / image.samples.len() as f64;
    println!("samples: min={minimum:.6}, mean={mean:.6}, max={maximum:.6}");
    Ok(())
}
