//! Optional `wgpu` compute execution for the point-wise part of `FocalCore`.
//!
//! The CPU pipeline remains the reference implementation. This backend only
//! accepts canonical ordered snapshots whose active work can be expressed as
//! one point-wise compute pass. Neighbourhood operations such as local
//! contrast, noise reduction, and crop are rejected until they have kernels
//! with separately verified boundary semantics.

use std::{
    fmt,
    sync::{Mutex, mpsc},
    time::Duration,
};

use bytemuck::{Pod, Zeroable, bytes_of, cast_slice};

use crate::{
    CurveSet, Image, ImageContract, ModuleKind, ModuleParameters, Pipeline, PipelineError,
    ProgressReporter, RenderContext, RenderProgress, RenderQuality, RenderReport,
    RenderStageReport, RenderStageStatus,
};

const WORKGROUP_SIZE: u32 = 64;
const SHADER: &str = r"
struct Parameters {
    execution: vec4<u32>,
    adjustment: vec4<f32>,
};

@group(0) @binding(0)
var<storage, read> input_pixels: array<vec4<f32>>;

@group(0) @binding(1)
var<storage, read_write> output_pixels: array<vec4<f32>>;

@group(0) @binding(2)
var<storage, read> parameters: Parameters;

fn srgb_to_linear(value: f32) -> f32 {
    if (value <= 0.04045) {
        return value / 12.92;
    }
    return pow((value + 0.055) / 1.055, 2.4);
}

fn linear_to_srgb(value: f32) -> f32 {
    let non_negative = max(value, 0.0);
    if (non_negative <= 0.0031308) {
        return non_negative * 12.92;
    }
    return 1.055 * pow(non_negative, 1.0 / 2.4) - 0.055;
}

fn linear_to_adobe(value: f32) -> f32 {
    return pow(max(value, 0.0), 1.0 / 2.1992188);
}

fn adobe_to_linear(value: f32) -> f32 {
    return pow(clamp(value, 0.0, 1.0), 2.1992188);
}

fn input_srgb_to_adobe(rgb: vec3<f32>) -> vec3<f32> {
    let linear = vec3<f32>(
        srgb_to_linear(rgb.r),
        srgb_to_linear(rgb.g),
        srgb_to_linear(rgb.b),
    );
    return vec3<f32>(
        0.715126 * linear.r + 0.284874 * linear.g,
        linear.g,
        0.041162 * linear.g + 0.958838 * linear.b,
    );
}

fn adobe_to_srgb(rgb: vec3<f32>) -> vec3<f32> {
    let linear_srgb = vec3<f32>(
        1.398355 * rgb.r - 0.398355 * rgb.g,
        rgb.g,
        -0.042929 * rgb.g + 1.042929 * rgb.b,
    );
    return vec3<f32>(
        clamp(linear_to_srgb(linear_srgb.r), 0.0, 1.0),
        clamp(linear_to_srgb(linear_srgb.g), 0.0, 1.0),
        clamp(linear_to_srgb(linear_srgb.b), 0.0, 1.0),
    );
}

@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let index = invocation.x;
    if (index >= parameters.execution.x) {
        return;
    }

    var rgb = input_pixels[index].xyz;
    let has_input = parameters.execution.z != 0u;
    let has_output = parameters.execution.w != 0u;
    if (has_input) {
        if (parameters.execution.y != 0u) {
            rgb = input_srgb_to_adobe(rgb);
        } else {
            rgb = vec3<f32>(
                adobe_to_linear(rgb.r),
                adobe_to_linear(rgb.g),
                adobe_to_linear(rgb.b),
            );
        }
    }

    rgb *= vec3<f32>(
        parameters.adjustment.y,
        parameters.adjustment.z,
        parameters.adjustment.w,
    );
    rgb *= parameters.adjustment.x;

    if (has_output) {
        rgb = adobe_to_srgb(rgb);
    }
    output_pixels[index] = vec4<f32>(rgb, 1.0);
}
";

#[derive(Debug)]
pub struct GpuPipeline {
    device: wgpu::Device,
    queue: wgpu::Queue,
    compute: wgpu::ComputePipeline,
    layout: wgpu::BindGroupLayout,
    adapter_name: String,
    buffers: Mutex<Option<GpuBuffers>>,
}

impl GpuPipeline {
    /// Creates a compute device using the system's default `wgpu` backends.
    ///
    /// # Errors
    ///
    /// Returns an availability error when no compatible adapter or device can
    /// be created. Headless CI and machines without a GPU can therefore keep
    /// the CPU reference and its tests without special configuration.
    pub fn new() -> Result<Self, GpuError> {
        pollster::block_on(Self::new_async())
    }

    async fn new_async() -> Result<Self, GpuError> {
        let instance = wgpu::Instance::default();
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                force_fallback_adapter: false,
                compatible_surface: None,
            })
            .await
            .map_err(|error| GpuError::AdapterUnavailable(error.to_string()))?;
        let adapter_name = adapter.get_info().name;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default())
            .await
            .map_err(|error| GpuError::DeviceUnavailable(error.to_string()))?;

        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("focal-core-gpu-bind-group-layout"),
            entries: &[
                storage_entry(0, wgpu::BufferBindingType::Storage { read_only: true }),
                storage_entry(1, wgpu::BufferBindingType::Storage { read_only: false }),
                storage_entry(2, wgpu::BufferBindingType::Storage { read_only: true }),
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("focal-core-gpu-pipeline-layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("focal-core-gpu-pointwise-shader"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let compute = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("focal-core-gpu-pointwise-pipeline"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: Some("main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });

        Ok(Self {
            device,
            queue,
            compute,
            layout,
            adapter_name,
            buffers: Mutex::new(None),
        })
    }

    /// Returns the adapter selected for this renderer.
    #[must_use]
    pub fn adapter_name(&self) -> &str {
        &self.adapter_name
    }

    /// Renders an image through a GPU-compatible pipeline snapshot.
    ///
    /// The returned pixels are compared against `Pipeline::render` in the
    /// parity tests. Unsupported spatial work is rejected instead of being
    /// approximated or silently sent through a second production pipeline.
    ///
    /// # Errors
    ///
    /// Returns [`GpuError::UnsupportedPipeline`] for snapshots which need a
    /// neighbourhood kernel, or a device/transfer error for GPU failures.
    #[allow(clippy::too_many_lines)]
    pub fn render(
        &self,
        pipeline: &Pipeline,
        image: &Image,
    ) -> Result<(Image, RenderReport), GpuError> {
        let context = RenderContext::new(RenderQuality::Export);
        let mut ignore_progress = |_: RenderProgress| {};
        self.render_with_context(pipeline, image, &context, &mut ignore_progress)
    }

    /// Renders a GPU-compatible snapshot with shared execution controls.
    ///
    /// # Errors
    ///
    /// Returns [`GpuError::UnsupportedPipeline`] for snapshots outside the
    /// verified GPU subset, or a validation, cancellation, device, or transfer
    /// error.
    #[allow(clippy::too_many_lines)]
    pub fn render_with_context<P: ProgressReporter>(
        &self,
        pipeline: &Pipeline,
        image: &Image,
        context: &RenderContext,
        progress: &mut P,
    ) -> Result<(Image, RenderReport), GpuError> {
        let snapshot = pipeline.snapshot();
        if snapshot.version != crate::PIPELINE_VERSION {
            return Err(GpuError::Pipeline(
                PipelineError::UnsupportedPipelineVersion {
                    expected: crate::PIPELINE_VERSION,
                    actual: snapshot.version,
                },
            ));
        }
        pipeline.validate_modules().map_err(GpuError::Pipeline)?;
        let cancellation = context.cancellation_token();
        if cancellation.is_cancelled() {
            return Err(cancelled_gpu_render());
        }
        if context.quality() == RenderQuality::Preview {
            return Err(GpuError::UnsupportedPipeline(
                "GPU preview awaits a parity-verified clipping-report kernel",
            ));
        }
        let parameters = GpuParameters::from_snapshot(&snapshot.modules, image)?;
        let stages: Vec<_> = snapshot
            .modules
            .iter()
            .enumerate()
            .filter(|(_, module)| module.enabled)
            .map(|(module_index, module)| RenderStageReport {
                module_index,
                module: module.kind(),
                status: if module.is_placeholder() {
                    RenderStageStatus::Placeholder
                } else {
                    RenderStageStatus::Processed
                },
            })
            .collect();
        report_gpu_progress(progress, 0, stages.len(), None);
        if cancellation.is_cancelled() {
            return Err(cancelled_gpu_render());
        }
        report_gpu_progress(progress, 0, stages.len(), stages.first());
        if cancellation.is_cancelled() {
            return Err(cancelled_gpu_render());
        }

        let mut input = Vec::with_capacity(image.pixels().len());
        for chunk in image.pixels().chunks(2_048) {
            if cancellation.is_cancelled() {
                return Err(cancelled_gpu_render());
            }
            input.extend(
                chunk
                    .iter()
                    .map(|pixel| [pixel[0], pixel[1], pixel[2], 1.0]),
            );
        }
        let output_size = pixel_buffer_size(input.len())?;
        let mut buffers = self.buffers.lock().map_err(|_| GpuError::ResourceLock)?;
        if buffers
            .as_ref()
            .is_none_or(|buffers| buffers.pixel_capacity < input.len())
        {
            let pixel_capacity = input
                .len()
                .checked_next_power_of_two()
                .ok_or(GpuError::BufferSizeOverflow)?;
            *buffers = Some(GpuBuffers::new(&self.device, &self.layout, pixel_capacity)?);
        }
        let buffers = buffers.as_mut().ok_or(GpuError::ResourceLock)?;
        self.queue
            .write_buffer(&buffers.input, 0, cast_slice(&input));
        self.queue
            .write_buffer(&buffers.parameters, 0, bytes_of(&parameters.block));
        if cancellation.is_cancelled() {
            return Err(cancelled_gpu_render());
        }
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("focal-core-gpu-command-encoder"),
            });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("focal-core-gpu-pointwise-pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.compute);
            pass.set_bind_group(0, &buffers.bind_group, &[]);
            let groups = u32::try_from(input.len())
                .map_err(|_| GpuError::BufferSizeOverflow)?
                .div_ceil(WORKGROUP_SIZE);
            pass.dispatch_workgroups(groups, 1, 1);
        }
        encoder.copy_buffer_to_buffer(&buffers.output, 0, &buffers.readback, 0, output_size);
        let submission = self.queue.submit([encoder.finish()]);

        let output_slice = buffers.readback.slice(..output_size);
        let (sender, receiver) = mpsc::channel();
        output_slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = sender.send(result);
        });
        let mut cancelled = false;
        loop {
            match self.device.poll(wgpu::PollType::Wait {
                submission_index: Some(submission.clone()),
                timeout: Some(Duration::from_millis(10)),
            }) {
                Ok(_) => break,
                Err(wgpu::PollError::Timeout) => {
                    cancelled |= cancellation.is_cancelled();
                }
                Err(error) => return Err(GpuError::DevicePoll(error.to_string())),
            }
        }
        receiver
            .recv()
            .map_err(|error| GpuError::Readback(error.to_string()))?
            .map_err(|error| GpuError::Readback(error.to_string()))?;
        let mapped = output_slice.get_mapped_range();
        let output = cast_slice::<u8, [f32; 4]>(&mapped).to_vec();
        drop(mapped);
        buffers.readback.unmap();
        if cancelled || cancellation.is_cancelled() {
            return Err(cancelled_gpu_render());
        }

        let pixels = output
            .into_iter()
            .map(|pixel| [pixel[0], pixel[1], pixel[2]])
            .collect();
        let result = Image::new(
            image.width(),
            image.height(),
            pixels,
            parameters.output_contract,
        )
        .map_err(GpuError::Image)?;
        for completed in 1..=stages.len() {
            report_gpu_progress(progress, completed, stages.len(), None);
            if cancellation.is_cancelled() {
                return Err(cancelled_gpu_render());
            }
            if let Some(next) = stages.get(completed) {
                report_gpu_progress(progress, completed, stages.len(), Some(next));
                if cancellation.is_cancelled() {
                    return Err(cancelled_gpu_render());
                }
            }
        }
        Ok((
            result,
            RenderReport {
                stages,
                clipping: None,
            },
        ))
    }
}

fn report_gpu_progress<P: ProgressReporter>(
    progress: &mut P,
    completed: usize,
    total: usize,
    current: Option<&RenderStageReport>,
) {
    #[allow(clippy::cast_precision_loss)]
    let fraction = completed as f32 / total.max(1) as f32;
    progress.report(RenderProgress {
        fraction,
        completed_stages: completed,
        total_stages: total,
        current_module: current.map(|stage| stage.module),
        current_module_index: current.map(|stage| stage.module_index),
    });
}

fn cancelled_gpu_render() -> GpuError {
    GpuError::Pipeline(PipelineError::Cancelled {
        module: None,
        module_index: None,
    })
}

#[derive(Debug)]
struct GpuBuffers {
    input: wgpu::Buffer,
    output: wgpu::Buffer,
    readback: wgpu::Buffer,
    parameters: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    pixel_capacity: usize,
}

impl GpuBuffers {
    fn new(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        pixel_capacity: usize,
    ) -> Result<Self, GpuError> {
        let size = pixel_buffer_size(pixel_capacity)?;
        let input = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("focal-core-gpu-input"),
            size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let output = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("focal-core-gpu-output"),
            size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("focal-core-gpu-readback"),
            size,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let parameters = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("focal-core-gpu-parameters"),
            size: u64::try_from(std::mem::size_of::<GpuParameterBlock>())
                .map_err(|_| GpuError::BufferSizeOverflow)?,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("focal-core-gpu-bind-group"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: input.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: output.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: parameters.as_entire_binding(),
                },
            ],
        });
        Ok(Self {
            input,
            output,
            readback,
            parameters,
            bind_group,
            pixel_capacity,
        })
    }
}

fn pixel_buffer_size(pixel_count: usize) -> Result<u64, GpuError> {
    pixel_count
        .checked_mul(std::mem::size_of::<[f32; 4]>())
        .and_then(|bytes| u64::try_from(bytes).ok())
        .ok_or(GpuError::BufferSizeOverflow)
}

fn storage_entry(binding: u32, ty: wgpu::BufferBindingType) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer {
            ty,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

#[derive(Clone, Debug)]
struct GpuParameters {
    block: GpuParameterBlock,
    output_contract: ImageContract,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct GpuParameterBlock {
    execution: [u32; 4],
    adjustment: [f32; 4],
}

impl GpuParameters {
    #[allow(clippy::too_many_lines)]
    fn from_snapshot(modules: &[crate::Module], image: &Image) -> Result<Self, GpuError> {
        let mut last_rank = None;
        let mut contract = image.contract();
        let mut has_input = false;
        let mut has_output = false;
        let mut exposure_gain = 1.0;
        let mut white_balance = [1.0; 3];
        for (module_index, module) in modules
            .iter()
            .enumerate()
            .filter(|(_, module)| module.enabled)
        {
            let rank = module_rank(module.kind());
            if last_rank.is_some_and(|last| rank <= last) {
                return Err(GpuError::UnsupportedPipeline(
                    "GPU execution requires the documented ordered pipeline",
                ));
            }
            last_rank = Some(rank);
            if let Some(expected) = module.required_contract(crate::WorkingSpace::LinearAdobeRgb)
                && contract != expected
            {
                return Err(GpuError::Pipeline(PipelineError::ContractMismatch {
                    module: module.kind(),
                    module_index,
                    expected,
                    actual: contract,
                }));
            }
            match &module.parameters {
                ModuleParameters::InputTransform => {
                    if !matches!(
                        contract,
                        ImageContract::SRGB_DISPLAY | ImageContract::ADOBE_RGB_CURVE
                    ) {
                        return Err(GpuError::UnsupportedPipeline(
                            "GPU input transform requires encoded sRGB or Adobe RGB",
                        ));
                    }
                    has_input = true;
                    contract = ImageContract::LINEAR_ADOBE_RGB;
                }
                ModuleParameters::WhiteBalance { warmth, tint } => {
                    let gains = white_balance_gains(*warmth, *tint);
                    for (current, gain) in white_balance.iter_mut().zip(gains) {
                        *current *= gain;
                    }
                }
                ModuleParameters::Exposure { stops } if stops.abs() <= 100.0 => {
                    exposure_gain *= stops.exp2();
                }
                ModuleParameters::Exposure { .. } => {
                    return Err(GpuError::UnsupportedPipeline(
                        "exposure outside the stage-safe GPU range requires optimized CPU execution",
                    ));
                }
                ModuleParameters::Contrast { amount } if *amount == 0.0 => {}
                ModuleParameters::Contrast { .. } => {
                    return Err(GpuError::UnsupportedPipeline(
                        "non-zero contrast needs a histogram reduction kernel",
                    ));
                }
                ModuleParameters::TonalCurve { curves, .. } if curves == &CurveSet::default() => {}
                ModuleParameters::TonalCurve { .. } => {
                    return Err(GpuError::UnsupportedPipeline(
                        "non-identity tonal curves need a GPU lookup-table kernel",
                    ));
                }
                ModuleParameters::LocalContrast { amount, .. } if *amount == 0.0 => {}
                ModuleParameters::NoiseReduction { luminance, colour }
                    if *luminance == 0.0 && *colour == 0.0 => {}
                ModuleParameters::Saturation { amount } if *amount == 0.0 => {}
                ModuleParameters::OrientationAndCrop { crop: None }
                | ModuleParameters::HighlightsAndShadows
                | ModuleParameters::CreativeColour
                | ModuleParameters::Sharpening
                | ModuleParameters::Resize
                | ModuleParameters::QuantisationAndDither => {}
                ModuleParameters::OutputTransform => {
                    has_output = true;
                    contract = ImageContract::SRGB_DISPLAY;
                }
                ModuleParameters::OrientationAndCrop { crop: Some(_) }
                | ModuleParameters::LocalContrast { .. }
                | ModuleParameters::NoiseReduction { .. }
                | ModuleParameters::Saturation { .. } => {
                    return Err(GpuError::UnsupportedPipeline(
                        "GPU execution does not yet cover non-zero spatial or chroma stages",
                    ));
                }
            }
        }
        if !has_input {
            return Err(GpuError::UnsupportedPipeline(
                "GPU stage fusion requires an encoded input transform",
            ));
        }
        let source_is_srgb = image.contract() == ImageContract::SRGB_DISPLAY;
        let pixel_count =
            u32::try_from(image.pixels().len()).map_err(|_| GpuError::BufferSizeOverflow)?;
        Ok(Self {
            block: GpuParameterBlock {
                execution: [
                    pixel_count,
                    u32::from(source_is_srgb),
                    u32::from(has_input),
                    u32::from(has_output),
                ],
                adjustment: [
                    exposure_gain,
                    white_balance[0],
                    white_balance[1],
                    white_balance[2],
                ],
            },
            output_contract: contract,
        })
    }
}

fn white_balance_gains(warmth: f32, tint: f32) -> [f32; 3] {
    let warm_gain = 2.0_f32.powf(warmth / 100.0);
    let tint_gain = 2.0_f32.powf(tint / 200.0);
    let mut gains = [
        warm_gain * tint_gain,
        1.0 / tint_gain,
        tint_gain / warm_gain,
    ];
    let neutral_luma = gains[0].mul_add(
        crate::ADOBE_RGB_LUMA_COEFFICIENTS[0],
        gains[1].mul_add(
            crate::ADOBE_RGB_LUMA_COEFFICIENTS[1],
            gains[2] * crate::ADOBE_RGB_LUMA_COEFFICIENTS[2],
        ),
    );
    for gain in &mut gains {
        *gain /= neutral_luma;
    }
    gains
}

const fn module_rank(kind: ModuleKind) -> u8 {
    match kind {
        ModuleKind::InputTransform => 0,
        ModuleKind::OrientationAndCrop => 1,
        ModuleKind::WhiteBalance => 2,
        ModuleKind::Exposure => 3,
        ModuleKind::HighlightsAndShadows => 4,
        ModuleKind::NoiseReduction => 5,
        ModuleKind::Contrast => 6,
        ModuleKind::TonalCurve => 7,
        ModuleKind::LocalContrast => 8,
        ModuleKind::Saturation => 9,
        ModuleKind::CreativeColour => 10,
        ModuleKind::Sharpening => 11,
        ModuleKind::Resize => 12,
        ModuleKind::OutputTransform => 13,
        ModuleKind::QuantisationAndDither => 14,
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GpuError {
    AdapterUnavailable(String),
    DeviceUnavailable(String),
    DevicePoll(String),
    Readback(String),
    ResourceLock,
    BufferSizeOverflow,
    Image(crate::ImageError),
    Pipeline(PipelineError),
    UnsupportedPipeline(&'static str),
}

impl fmt::Display for GpuError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AdapterUnavailable(error) => {
                write!(formatter, "GPU adapter unavailable: {error}")
            }
            Self::DeviceUnavailable(error) => write!(formatter, "GPU device unavailable: {error}"),
            Self::DevicePoll(error) => write!(formatter, "GPU device poll failed: {error}"),
            Self::Readback(error) => write!(formatter, "GPU readback failed: {error}"),
            Self::ResourceLock => write!(formatter, "GPU resource lock is unavailable"),
            Self::BufferSizeOverflow => {
                write!(formatter, "GPU buffer size overflows addressable memory")
            }
            Self::Image(error) => write!(formatter, "GPU output image is invalid: {error}"),
            Self::Pipeline(error) => write!(formatter, "pipeline cannot run on GPU: {error}"),
            Self::UnsupportedPipeline(reason) => {
                write!(formatter, "GPU pipeline unsupported: {reason}")
            }
        }
    }
}

impl std::error::Error for GpuError {}

impl From<PipelineError> for GpuError {
    fn from(error: PipelineError) -> Self {
        Self::Pipeline(error)
    }
}
