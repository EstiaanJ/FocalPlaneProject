#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)]

use crate::{ADOBE_RGB_LUMA_COEFFICIENTS, CancellationToken, Image};
const ADOBE_GAMMA: f32 = 2.199_218_8;

pub(crate) fn white_balance(
    image: &mut Image,
    warmth: f32,
    tint: f32,
    cancellation: &CancellationToken,
) -> Result<(), ()> {
    // These opponent gains are deliberately described as warmth and tint:
    // rendered RGB pixels no longer retain a physical colour temperature.
    let warm_gain = 2.0_f32.powf(warmth / 100.0);
    let tint_gain = 2.0_f32.powf(tint / 200.0);
    let mut gains = [
        warm_gain * tint_gain,
        1.0 / tint_gain,
        tint_gain / warm_gain,
    ];
    let neutral_luma = luma(gains);
    for gain in &mut gains {
        *gain /= neutral_luma;
    }

    for pixel in image.pixels_mut() {
        if cancellation.is_cancelled() {
            return Err(());
        }
        for (channel, gain) in pixel.iter_mut().zip(gains) {
            *channel *= gain;
        }
    }
    Ok(())
}

pub(crate) fn local_contrast(
    image: &mut Image,
    amount: f32,
    radius: f32,
    cancellation: &CancellationToken,
) -> Result<(), ()> {
    if amount == 0.0 {
        return Ok(());
    }
    let encoded = encoded_pixels(image);
    let lightness: Vec<f32> = encoded.iter().copied().map(luma).collect();
    let base = gaussian_blur(
        &lightness,
        image.width() as usize,
        image.height() as usize,
        radius,
        cancellation,
    )?;
    let strength = amount / 100.0;

    for (((pixel, encoded_pixel), source_luma), base_luma) in image
        .pixels_mut()
        .iter_mut()
        .zip(encoded)
        .zip(lightness)
        .zip(base)
    {
        if cancellation.is_cancelled() {
            return Err(());
        }
        let target_luma = (source_luma + strength * (source_luma - base_luma)).max(0.0);
        let scale = if source_luma > 1.0e-6 {
            target_luma / source_luma
        } else {
            1.0
        };
        *pixel = encoded_pixel.map(|value| adobe_to_linear((value * scale).max(0.0)));
    }
    Ok(())
}

pub(crate) fn contrast(
    image: &mut Image,
    amount: f32,
    cancellation: &CancellationToken,
) -> Result<(), ()> {
    if amount == 0.0 {
        return Ok(());
    }
    // RawTherapee derives the pivot from the histogram of linear working-space
    // luminance, then expresses that luminance through the sRGB transfer curve
    // because the contrast curve itself is shaped in sRGB gamma.
    let average = (image
        .pixels()
        .iter()
        .copied()
        .map(luma)
        .map(linear_to_srgb)
        .sum::<f32>()
        / image.pixels().len() as f32)
        .clamp(0.01, 0.99);
    let displacement = amount / 250.0;
    let toe_input = average * (0.4 + displacement);
    let toe_output = average * (0.4 - displacement);
    let shoulder_input = average + (1.0 - average) * (0.6 - displacement);
    let shoulder_output = average + (1.0 - average) * (0.6 + displacement);
    let midpoint = (
        (toe_input + shoulder_input) * 0.5,
        (toe_output + shoulder_output) * 0.5,
    );

    for pixel in image.pixels_mut() {
        if cancellation.is_cancelled() {
            return Err(());
        }
        *pixel = pixel.map(|channel| {
            let channel = linear_to_srgb(channel);
            let adjusted = if channel <= midpoint.0 {
                quadratic_bezier_y_for_x((0.0, 0.0), (toe_input, toe_output), midpoint, channel)
            } else {
                quadratic_bezier_y_for_x(
                    midpoint,
                    (shoulder_input, shoulder_output),
                    (1.0, 1.0),
                    channel,
                )
            };
            srgb_to_linear(adjusted)
        });
    }
    Ok(())
}

pub(crate) fn noise_reduction(
    image: &mut Image,
    luminance: f32,
    colour: f32,
    cancellation: &CancellationToken,
) -> Result<(), ()> {
    if luminance == 0.0 && colour == 0.0 {
        return Ok(());
    }
    let width = image.width() as usize;
    let height = image.height() as usize;
    let encoded = encoded_pixels(image);
    let source_luma: Vec<f32> = encoded.iter().copied().map(luma).collect();
    let radius = 1 + ((luminance.max(colour) / 34.0).floor() as usize).min(2);
    let filtered = edge_aware_filter(&encoded, &source_luma, width, height, radius, cancellation)?;
    let luma_mix = luminance / 100.0;
    let colour_mix = colour / 100.0;

    for index in 0..encoded.len() {
        if cancellation.is_cancelled() {
            return Err(());
        }
        let original = encoded[index];
        let smooth = filtered[index];
        let original_luma = source_luma[index];
        let smooth_luma = luma(smooth);
        let output_luma = lerp(original_luma, smooth_luma, luma_mix);
        let mut output = [0.0; 3];
        for channel in 0..3 {
            let original_chroma = original[channel] - original_luma;
            let smooth_chroma = smooth[channel] - smooth_luma;
            output[channel] = output_luma + lerp(original_chroma, smooth_chroma, colour_mix);
        }
        image.pixels_mut()[index] = output.map(|value| adobe_to_linear(value.max(0.0)));
    }
    Ok(())
}

pub(crate) fn saturation(
    image: &mut Image,
    amount: f32,
    cancellation: &CancellationToken,
) -> Result<(), ()> {
    if amount == 0.0 {
        return Ok(());
    }
    let adjustment = amount / 100.0;
    for pixel in image.pixels_mut() {
        if cancellation.is_cancelled() {
            return Err(());
        }
        // RawTherapee's Exposure-panel saturation works directly on linear
        // working RGB after its tone curve, rather than on encoded channels.
        let (hue, mut saturation, value) = rgb_to_hsv(*pixel);
        if adjustment > 0.0 {
            let protected_target = 1.0 - (1.0 - saturation.min(1.0)).powi(4);
            saturation = lerp(saturation, protected_target, adjustment);
        } else {
            saturation *= 1.0 + adjustment;
        }
        *pixel = hsv_to_rgb(hue, saturation, value);
    }
    Ok(())
}

fn encoded_pixels(image: &Image) -> Vec<[f32; 3]> {
    image
        .pixels()
        .iter()
        .map(|pixel| pixel.map(linear_to_adobe))
        .collect()
}

fn luma(pixel: [f32; 3]) -> f32 {
    pixel[0].mul_add(
        ADOBE_RGB_LUMA_COEFFICIENTS[0],
        pixel[1].mul_add(
            ADOBE_RGB_LUMA_COEFFICIENTS[1],
            pixel[2] * ADOBE_RGB_LUMA_COEFFICIENTS[2],
        ),
    )
}

fn linear_to_adobe(value: f32) -> f32 {
    value.max(0.0).powf(1.0 / ADOBE_GAMMA)
}

fn adobe_to_linear(value: f32) -> f32 {
    value.powf(ADOBE_GAMMA)
}

fn linear_to_srgb(value: f32) -> f32 {
    let value = value.max(0.0);
    if value <= 0.003_130_8 {
        value * 12.92
    } else {
        1.055 * value.powf(1.0 / 2.4) - 0.055
    }
}

fn srgb_to_linear(value: f32) -> f32 {
    if value <= 0.040_45 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

fn gaussian_blur(
    source: &[f32],
    width: usize,
    height: usize,
    sigma: f32,
    cancellation: &CancellationToken,
) -> Result<Vec<f32>, ()> {
    // Three equal box blurs closely approximate a Gaussian while making a
    // large photographic radius no more expensive per pixel than a small one.
    let radius = sigma.round().clamp(1.0, 256.0) as usize;
    let mut current = source.to_vec();
    let mut intermediate = vec![0.0; source.len()];
    let mut output = vec![0.0; source.len()];
    for _ in 0..3 {
        box_blur_axis(
            &current,
            &mut intermediate,
            width,
            height,
            radius,
            true,
            cancellation,
        )?;
        box_blur_axis(
            &intermediate,
            &mut output,
            width,
            height,
            radius,
            false,
            cancellation,
        )?;
        std::mem::swap(&mut current, &mut output);
    }
    Ok(current)
}

fn box_blur_axis(
    source: &[f32],
    output: &mut [f32],
    width: usize,
    height: usize,
    radius: usize,
    horizontal: bool,
    cancellation: &CancellationToken,
) -> Result<(), ()> {
    let (line_count, line_length) = if horizontal {
        (height, width)
    } else {
        (width, height)
    };
    let sample = |line: usize, position: usize| {
        if horizontal {
            source[line * width + position]
        } else {
            source[position * width + line]
        }
    };
    let output_index = |line: usize, position: usize| {
        if horizontal {
            line * width + position
        } else {
            position * width + line
        }
    };
    let divisor = (radius * 2 + 1) as f32;

    for line in 0..line_count {
        if cancellation.is_cancelled() {
            return Err(());
        }
        let mut sum = 0.0;
        for offset in 0..=(radius * 2) {
            sum += sample(line, offset.saturating_sub(radius).min(line_length - 1));
        }
        for position in 0..line_length {
            if position % 4096 == 0 && cancellation.is_cancelled() {
                return Err(());
            }
            output[output_index(line, position)] = sum / divisor;
            let leaving = position.saturating_sub(radius);
            let entering = (position + radius + 1).min(line_length - 1);
            sum += sample(line, entering) - sample(line, leaving);
        }
    }
    Ok(())
}

fn edge_aware_filter(
    source: &[[f32; 3]],
    guide: &[f32],
    width: usize,
    height: usize,
    radius: usize,
    cancellation: &CancellationToken,
) -> Result<Vec<[f32; 3]>, ()> {
    let mut output = vec![[0.0; 3]; source.len()];
    let range_sigma = 0.08_f32;
    for y in 0..height {
        if cancellation.is_cancelled() {
            return Err(());
        }
        for x in 0..width {
            if x % 1024 == 0 && cancellation.is_cancelled() {
                return Err(());
            }
            let centre = guide[y * width + x];
            let mut sum = [0.0; 3];
            let mut total_weight = 0.0;
            for sample_y in y.saturating_sub(radius)..=(y + radius).min(height - 1) {
                for sample_x in x.saturating_sub(radius)..=(x + radius).min(width - 1) {
                    let index = sample_y * width + sample_x;
                    let difference = guide[index] - centre;
                    let range_weight =
                        (-0.5 * difference * difference / (range_sigma * range_sigma)).exp();
                    let dx = x.abs_diff(sample_x) as f32;
                    let dy = y.abs_diff(sample_y) as f32;
                    let spatial_weight =
                        (-0.5 * (dx * dx + dy * dy) / (radius as f32).powi(2)).exp();
                    let weight = range_weight * spatial_weight;
                    for channel in 0..3 {
                        sum[channel] += source[index][channel] * weight;
                    }
                    total_weight += weight;
                }
            }
            output[y * width + x] = sum.map(|value| value / total_weight);
        }
    }
    Ok(output)
}

fn lerp(start: f32, end: f32, amount: f32) -> f32 {
    start + amount * (end - start)
}

fn quadratic_bezier_y_for_x(
    start: (f32, f32),
    control: (f32, f32),
    end: (f32, f32),
    x: f32,
) -> f32 {
    let x = x.clamp(start.0, end.0);
    let mut lower = 0.0_f32;
    let mut upper = 1.0_f32;
    for _ in 0..20 {
        let parameter = (lower + upper) * 0.5;
        if quadratic_bezier(start.0, control.0, end.0, parameter) < x {
            lower = parameter;
        } else {
            upper = parameter;
        }
    }
    quadratic_bezier(start.1, control.1, end.1, (lower + upper) * 0.5).clamp(0.0, 1.0)
}

fn quadratic_bezier(start: f32, control: f32, end: f32, parameter: f32) -> f32 {
    let inverse = 1.0 - parameter;
    inverse * inverse * start + 2.0 * inverse * parameter * control + parameter * parameter * end
}

fn rgb_to_hsv(rgb: [f32; 3]) -> (f32, f32, f32) {
    let maximum = rgb.into_iter().fold(f32::NEG_INFINITY, f32::max);
    let minimum = rgb.into_iter().fold(f32::INFINITY, f32::min);
    let chroma = maximum - minimum;
    let saturation = if maximum > 0.0 { chroma / maximum } else { 0.0 };
    let hue = if chroma <= f32::EPSILON {
        0.0
    } else if rgb[0] >= rgb[1] && rgb[0] >= rgb[2] {
        ((rgb[1] - rgb[2]) / chroma).rem_euclid(6.0)
    } else if rgb[1] >= rgb[2] {
        (rgb[2] - rgb[0]) / chroma + 2.0
    } else {
        (rgb[0] - rgb[1]) / chroma + 4.0
    };
    (hue, saturation, maximum)
}

fn hsv_to_rgb(hue: f32, saturation: f32, value: f32) -> [f32; 3] {
    let chroma = value * saturation;
    let intermediate = chroma * (1.0 - (hue.rem_euclid(2.0) - 1.0).abs());
    let base = match hue.floor() as i32 {
        0 => [chroma, intermediate, 0.0],
        1 => [intermediate, chroma, 0.0],
        2 => [0.0, chroma, intermediate],
        3 => [0.0, intermediate, chroma],
        4 => [intermediate, 0.0, chroma],
        _ => [chroma, 0.0, intermediate],
    };
    let minimum = value - chroma;
    base.map(|channel| channel + minimum)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ImageContract;

    fn image(width: u32, height: u32, pixels: Vec<[f32; 3]>) -> Image {
        Image::new(width, height, pixels, ImageContract::LINEAR_ADOBE_RGB).unwrap()
    }

    #[test]
    fn neutral_white_balance_is_identity() {
        let mut value = image(1, 1, vec![[0.2, 0.4, 0.8]]);
        white_balance(&mut value, 0.0, 0.0, &CancellationToken::new()).unwrap();
        assert_eq!(value.pixels(), &[[0.2, 0.4, 0.8]]);
    }

    #[test]
    fn warmth_moves_neutral_pixels_towards_red_and_away_from_blue() {
        let mut value = image(1, 1, vec![[0.5; 3]]);
        white_balance(&mut value, 50.0, 0.0, &CancellationToken::new()).unwrap();
        let pixel = value.pixels()[0];
        assert!(pixel[0] > pixel[1] && pixel[1] > pixel[2]);

        let mut tint = image(1, 1, vec![[0.5; 3]]);
        white_balance(&mut tint, 0.0, 100.0, &CancellationToken::new()).unwrap();
        let pixel = tint.pixels()[0];
        assert!(pixel[0] > pixel[1] && pixel[2] > pixel[1]);
    }

    #[test]
    fn local_contrast_changes_detail_but_not_a_constant_field() {
        let mut constant = image(3, 1, vec![[0.4; 3]; 3]);
        local_contrast(&mut constant, 50.0, 2.0, &CancellationToken::new()).unwrap();
        assert!(
            constant
                .pixels()
                .iter()
                .all(|pixel| (pixel[0] - 0.4).abs() < 1.0e-5)
        );

        let mut detail = image(3, 1, vec![[0.2; 3], [0.8; 3], [0.2; 3]]);
        local_contrast(&mut detail, 50.0, 2.0, &CancellationToken::new()).unwrap();
        assert!(detail.pixels()[1][0] > 0.8);
    }

    #[test]
    fn contrast_is_bounded_and_uses_an_image_adaptive_s_curve() {
        let mut value = image(3, 1, vec![[0.05; 3], [0.4; 3], [0.95; 3]]);
        contrast(&mut value, 20.0, &CancellationToken::new()).unwrap();
        assert!(value.pixels()[0][0] < 0.05);
        assert!(value.pixels()[2][0] > 0.95);
        assert!(
            value
                .pixels()
                .iter()
                .flatten()
                .all(|channel| *channel >= 0.0)
        );

        let mut negative = image(3, 1, vec![[0.05; 3], [0.4; 3], [0.95; 3]]);
        contrast(&mut negative, -20.0, &CancellationToken::new()).unwrap();
        assert!(negative.pixels()[0][0] > 0.05);
        assert!(negative.pixels()[2][0] < 0.95);
    }

    #[test]
    fn srgb_transfer_used_by_contrast_round_trips_linear_values() {
        for value in [0.0, 0.001, 0.18, 0.5, 1.0] {
            assert!((srgb_to_linear(linear_to_srgb(value)) - value).abs() < 1.0e-6);
        }
    }

    #[test]
    fn colour_noise_reduction_smooths_chroma_without_flattening_luma_edges() {
        let mut value = image(
            3,
            1,
            vec![[0.2, 0.2, 0.2], [0.2, 0.5, 0.2], [0.8, 0.8, 0.8]],
        );
        let before = value.pixels()[1][1] - value.pixels()[1][0];
        noise_reduction(&mut value, 0.0, 100.0, &CancellationToken::new()).unwrap();
        let after = value.pixels()[1][1] - value.pixels()[1][0];
        assert!(after < before);
        assert!(value.pixels()[2][0] > 0.7);

        let mut luminance = image(3, 1, vec![[0.2; 3], [0.8; 3], [0.2; 3]]);
        noise_reduction(&mut luminance, 100.0, 0.0, &CancellationToken::new()).unwrap();
        assert!(luminance.pixels()[1][0] < 0.8);
    }

    #[test]
    fn spatial_processing_observes_preexisting_cancellation() {
        let cancellation = CancellationToken::new();
        cancellation.cancel();
        let mut value = image(2, 2, vec![[0.5; 3]; 4]);
        assert!(local_contrast(&mut value, 20.0, 2.0, &cancellation).is_err());
        assert!(noise_reduction(&mut value, 20.0, 20.0, &cancellation).is_err());
    }

    #[test]
    fn saturation_is_an_identity_at_zero_and_reduces_chroma_when_negative() {
        let mut identity = image(1, 1, vec![[0.8, 0.3, 0.2]]);
        let original = identity.clone();
        saturation(&mut identity, 0.0, &CancellationToken::new()).unwrap();
        assert_eq!(identity, original);

        saturation(&mut identity, -100.0, &CancellationToken::new()).unwrap();
        let pixel = identity.pixels()[0];
        assert!((pixel[0] - pixel[1]).abs() < 1.0e-6);
        assert!((pixel[1] - pixel[2]).abs() < 1.0e-6);

        let mut positive = image(1, 1, vec![[0.8, 0.3, 0.2]]);
        saturation(&mut positive, 100.0, &CancellationToken::new()).unwrap();
        let pixel = positive.pixels()[0];
        assert!(pixel[0] - pixel[2] > 0.5);
    }

    #[test]
    fn saturation_operates_on_linear_working_rgb() {
        let mut value = image(1, 1, vec![[0.8, 0.4, 0.2]]);
        saturation(&mut value, -50.0, &CancellationToken::new()).unwrap();
        let pixel = value.pixels()[0];
        for (actual, expected) in pixel.into_iter().zip([0.8, 0.6, 0.5]) {
            assert!((actual - expected).abs() < 1.0e-6);
        }
    }
}
