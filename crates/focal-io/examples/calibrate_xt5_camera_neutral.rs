#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::needless_range_loop
)]

use std::{env, fs, path::Path};

use image::{GenericImageView, RgbImage};

const FEATURE_COUNT: usize = 20;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let developed_path = env::args()
        .nth(1)
        .ok_or("usage: calibrate_xt5_camera_neutral <developed.png> <rectangles.csv>")?;
    let regions_path = env::args()
        .nth(2)
        .ok_or("usage: calibrate_xt5_camera_neutral <developed.png> <rectangles.csv>")?;
    let developed = image::open(Path::new(&developed_path))?.to_rgb8();
    let regions = read_regions(Path::new(&regions_path), &developed)?;
    let fit_all = env::var_os("FOCAL_CALIBRATE_ALL").is_some();
    let training = regions
        .iter()
        .filter(|region| fit_all || region.number % 3 != 0)
        .collect::<Vec<_>>();
    let coefficients = fit(&training)?;

    if fit_all {
        report("training", regions.iter(), &coefficients);
    } else {
        report(
            "training",
            regions.iter().filter(|region| region.number % 3 != 0),
            &coefficients,
        );
        report(
            "held-out",
            regions.iter().filter(|region| region.number % 3 == 0),
            &coefficients,
        );
    }
    report("all", regions.iter(), &coefficients);
    println!("per-region source -> fitted -> target average RGB:");
    for region in &regions {
        let fitted = predict(region.source, &coefficients);
        println!(
            "{:02}: [{:6.1}, {:6.1}, {:6.1}] -> [{:6.1}, {:6.1}, {:6.1}] -> [{:6.1}, {:6.1}, {:6.1}]",
            region.number,
            region.source[0] * 255.0,
            region.source[1] * 255.0,
            region.source[2] * 255.0,
            fitted[0] * 255.0,
            fitted[1] * 255.0,
            fitted[2] * 255.0,
            region.target[0] * 255.0,
            region.target[1] * 255.0,
            region.target[2] * 255.0,
        );
    }
    println!(
        "features: 1, r, g, b, r², g², b², rg, rb, gb, r³, g³, b³, r²g, r²b, g²r, g²b, b²r, b²g, rgb"
    );
    println!("coefficients (one feature row per line, RGB columns):");
    for row in coefficients {
        println!("[{:.9}, {:.9}, {:.9}],", row[0], row[1], row[2]);
    }
    if let Some(output_path) = env::args().nth(3) {
        let mut calibrated = developed;
        for pixel in calibrated.pixels_mut() {
            let source = pixel.0.map(|value| f64::from(value) / 255.0);
            let output = predict(source, &coefficients);
            pixel.0 = output.map(|value| (value * 255.0).round() as u8);
        }
        calibrated.save(output_path)?;
    }
    Ok(())
}

#[derive(Clone, Debug)]
struct Region {
    number: usize,
    source: [f64; 3],
    target: [f64; 3],
}

fn read_regions(
    path: &Path,
    developed: &RgbImage,
) -> Result<Vec<Region>, Box<dyn std::error::Error>> {
    let text = fs::read_to_string(path)?;
    let mut regions = Vec::new();
    for line in text.lines().skip(1) {
        if line.trim().is_empty() {
            break;
        }
        let fields = line.split(',').collect::<Vec<_>>();
        if fields.len() < 16 {
            return Err(format!("invalid rectangle row: {line}").into());
        }
        let number = fields[0].parse::<usize>()?;
        let left = fields[1].parse::<f64>()?.floor() as u32;
        let top = fields[2].parse::<f64>()?.floor() as u32;
        let right = fields[3].parse::<f64>()?.ceil() as u32;
        let bottom = fields[4].parse::<f64>()?.ceil() as u32;
        if right > developed.width()
            || bottom > developed.height()
            || left >= right
            || top >= bottom
        {
            return Err(format!("rectangle {number} is outside the developed image").into());
        }
        let mut sum = [0.0; 3];
        let mut count = 0_u64;
        for pixel in developed
            .view(left, top, right - left, bottom - top)
            .pixels()
        {
            for channel in 0..3 {
                sum[channel] += f64::from(pixel.2[channel]) / 255.0;
            }
            count += 1;
        }
        let source = sum.map(|value| value / count as f64);
        let target = target_from_fields(&fields)?;
        regions.push(Region {
            number,
            source,
            target,
        });
    }
    if regions.len() != 38 {
        return Err(format!("expected 38 rectangle rows, found {}", regions.len()).into());
    }
    Ok(regions)
}

fn target_from_fields(fields: &[&str]) -> Result<[f64; 3], Box<dyn std::error::Error>> {
    Ok([
        fields[9].parse::<f64>()? / 255.0,
        fields[10].parse::<f64>()? / 255.0,
        fields[11].parse::<f64>()? / 255.0,
    ])
}

fn features([r, g, b]: [f64; 3]) -> [f64; FEATURE_COUNT] {
    [
        1.0,
        r,
        g,
        b,
        r * r,
        g * g,
        b * b,
        r * g,
        r * b,
        g * b,
        r * r * r,
        g * g * g,
        b * b * b,
        r * r * g,
        r * r * b,
        g * g * r,
        g * g * b,
        b * b * r,
        b * b * g,
        r * g * b,
    ]
}

fn fit(regions: &[&Region]) -> Result<[[f64; 3]; FEATURE_COUNT], &'static str> {
    let mut normal = [[0.0; FEATURE_COUNT]; FEATURE_COUNT];
    let mut targets = [[0.0; 3]; FEATURE_COUNT];
    for region in regions {
        let x = features(region.source);
        for row in 0..FEATURE_COUNT {
            for column in 0..FEATURE_COUNT {
                normal[row][column] += x[row] * x[column];
            }
            for channel in 0..3 {
                targets[row][channel] += x[row] * region.target[channel];
            }
        }
    }
    for diagonal in 1..FEATURE_COUNT {
        normal[diagonal][diagonal] += 1.0e-5;
    }
    solve(normal, targets)
}

fn solve(
    mut matrix: [[f64; FEATURE_COUNT]; FEATURE_COUNT],
    mut values: [[f64; 3]; FEATURE_COUNT],
) -> Result<[[f64; 3]; FEATURE_COUNT], &'static str> {
    for pivot in 0..FEATURE_COUNT {
        let best = (pivot..FEATURE_COUNT)
            .max_by(|left, right| {
                matrix[*left][pivot]
                    .abs()
                    .total_cmp(&matrix[*right][pivot].abs())
            })
            .ok_or("calibration matrix is empty")?;
        if matrix[best][pivot].abs() < 1.0e-12 {
            return Err("calibration matrix is singular");
        }
        matrix.swap(pivot, best);
        values.swap(pivot, best);
        let divisor = matrix[pivot][pivot];
        for column in pivot..FEATURE_COUNT {
            matrix[pivot][column] /= divisor;
        }
        for channel in 0..3 {
            values[pivot][channel] /= divisor;
        }
        for row in 0..FEATURE_COUNT {
            if row == pivot {
                continue;
            }
            let factor = matrix[row][pivot];
            for column in pivot..FEATURE_COUNT {
                matrix[row][column] -= factor * matrix[pivot][column];
            }
            for channel in 0..3 {
                values[row][channel] -= factor * values[pivot][channel];
            }
        }
    }
    Ok(values)
}

fn report<'a>(
    name: &str,
    regions: impl Iterator<Item = &'a Region>,
    coefficients: &[[f64; 3]; FEATURE_COUNT],
) {
    let mut before = 0.0;
    let mut after = 0.0;
    let mut samples = 0;
    for region in regions {
        let predicted = predict(region.source, coefficients);
        for channel in 0..3 {
            before += (region.source[channel] - region.target[channel]).powi(2);
            after += (predicted[channel] - region.target[channel]).powi(2);
            samples += 1;
        }
    }
    println!(
        "{name}: RGB RMSE {:.3} -> {:.3} code values",
        (before / f64::from(samples)).sqrt() * 255.0,
        (after / f64::from(samples)).sqrt() * 255.0
    );
}

fn predict(source: [f64; 3], coefficients: &[[f64; 3]; FEATURE_COUNT]) -> [f64; 3] {
    let x = features(source);
    let mut output = [0.0; 3];
    for feature in 0..FEATURE_COUNT {
        for channel in 0..3 {
            output[channel] += x[feature] * coefficients[feature][channel];
        }
    }
    output.map(|value| value.clamp(0.0, 1.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_uses_average_rgb_columns_in_csv_order() {
        let fields = [
            "1", "0", "0", "1", "1", "0", "0", "1", "1", "10", "20", "30", "40", "50", "60",
            "#28323C",
        ];
        assert_eq!(
            target_from_fields(&fields).unwrap(),
            [10.0 / 255.0, 20.0 / 255.0, 30.0 / 255.0]
        );
    }
}
