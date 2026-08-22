//! Compare the CPU reference and optional GPU point-wise pipeline.
//!
//! Run from the workspace root with:
//!
//! ```text
//! cargo run --release --features gpu --example benchmark_pipeline -p focal-core -- test-image/radial_mtf.png
//! ```

use std::{env, hint::black_box, path::Path, time::Instant};

use focal_core::{Image, ImageContract, OptimizedPipeline, Pipeline};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = env::args()
        .nth(1)
        .unwrap_or_else(|| "test-image/radial_mtf.png".to_owned());
    let image = decode(Path::new(&path))?;
    let pipeline = Pipeline::default();
    let iterations = 10_u32;

    let reference_inputs: Vec<_> = (0..iterations).map(|_| image.clone()).collect();
    let start = Instant::now();
    for input in reference_inputs {
        black_box(pipeline.render(input)?);
    }
    let cpu = start.elapsed();
    println!(
        "CPU reference: total={cpu:?}, per_render={:?}",
        cpu / iterations
    );

    let optimized_cpu = OptimizedPipeline::cpu_only()?;
    let _ = optimized_cpu.render(&pipeline, image.clone())?;
    let optimized_inputs: Vec<_> = (0..iterations).map(|_| image.clone()).collect();
    let start = Instant::now();
    for input in optimized_inputs {
        black_box(optimized_cpu.render(&pipeline, input)?);
    }
    let optimized_cpu_time = start.elapsed();
    println!(
        "Optimized CPU: total={optimized_cpu_time:?}, per_render={:?}, speedup={:.2}x",
        optimized_cpu_time / iterations,
        cpu.as_secs_f64() / optimized_cpu_time.as_secs_f64()
    );

    #[cfg(feature = "gpu")]
    {
        let gpu = focal_core::gpu::GpuPipeline::new()?;
        let _ = gpu.render(&pipeline, &image)?;
        let start = Instant::now();
        for _ in 0..iterations {
            black_box(gpu.render(&pipeline, &image)?);
        }
        let gpu_time = start.elapsed();
        println!("GPU adapter: {}", gpu.adapter_name());
        println!(
            "GPU point-wise path: total={gpu_time:?}, per_render={:?}, speedup={:.2}x",
            gpu_time / iterations,
            cpu.as_secs_f64() / gpu_time.as_secs_f64()
        );
    }
    #[cfg(not(feature = "gpu"))]
    println!("GPU path is disabled; rerun with --features gpu.");

    Ok(())
}

fn decode(path: &Path) -> Result<Image, Box<dyn std::error::Error>> {
    let decoded = image::open(path)?.to_rgb32f();
    let (width, height) = decoded.dimensions();
    let pixels = decoded
        .pixels()
        .map(|pixel| [pixel[0], pixel[1], pixel[2]])
        .collect();
    Ok(Image::new(
        width,
        height,
        pixels,
        ImageContract::SRGB_DISPLAY,
    )?)
}
