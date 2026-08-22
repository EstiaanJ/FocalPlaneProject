//! Coordinated accelerated execution for the ordered `FocalCore` pipeline.
//!
//! This is an implementation of the same processing contract as [`Pipeline`],
//! not a second processing architecture. The scalar CPU renderer remains the
//! correctness reference.

use std::{
    fmt,
    sync::{Arc, OnceLock},
};

use crate::{
    Image, Pipeline, PipelineError, ProgressReporter, RenderContext, RenderProgress, RenderQuality,
    RenderReport,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OptimizedBackend {
    Cpu,
    #[cfg(feature = "gpu")]
    Gpu,
}

#[derive(Debug)]
pub struct OptimizedPipeline {
    cpu_pool: Arc<rayon::ThreadPool>,
    #[cfg(feature = "gpu")]
    gpu: Option<crate::gpu::GpuPipeline>,
    #[cfg(feature = "gpu")]
    gpu_initialization_error: Option<String>,
}

impl OptimizedPipeline {
    /// Construct an optimized executor which uses multithreaded CPU work.
    ///
    /// # Errors
    ///
    /// Returns an initialization error if the bounded CPU worker pool cannot
    /// be created.
    pub fn cpu_only() -> Result<Self, OptimizedPipelineError> {
        Ok(Self {
            cpu_pool: create_cpu_pool()?,
            #[cfg(feature = "gpu")]
            gpu: None,
            #[cfg(feature = "gpu")]
            gpu_initialization_error: None,
        })
    }

    /// Construct an optimized executor which prefers a compatible GPU and
    /// otherwise uses multithreaded CPU work.
    ///
    /// # Errors
    ///
    /// Returns an initialization error if the bounded CPU worker pool cannot
    /// be created. GPU unavailability remains inspectable through
    /// [`Self::gpu_initialization_error`] and does not prevent CPU execution.
    pub fn new() -> Result<Self, OptimizedPipelineError> {
        #[cfg(feature = "gpu")]
        let (gpu, gpu_initialization_error) = match crate::gpu::GpuPipeline::new() {
            Ok(gpu) => (Some(gpu), None),
            Err(error) => (None, Some(error.to_string())),
        };
        Ok(Self {
            cpu_pool: create_cpu_pool()?,
            #[cfg(feature = "gpu")]
            gpu,
            #[cfg(feature = "gpu")]
            gpu_initialization_error,
        })
    }

    /// Explains why GPU initialization failed, when CPU fallback was selected
    /// during construction. `None` also covers explicit CPU-only execution.
    #[cfg(feature = "gpu")]
    #[must_use]
    pub fn gpu_initialization_error(&self) -> Option<&str> {
        self.gpu_initialization_error.as_deref()
    }

    /// Render using the fastest available complete implementation.
    ///
    /// A GPU-incompatible snapshot runs on the multithreaded optimized CPU
    /// implementation. Device failures are returned instead of being hidden
    /// by an unreported backend change.
    ///
    /// # Errors
    ///
    /// Returns a pipeline validation, cancellation, image-processing, GPU
    /// device, or transfer error without silently changing backend.
    pub fn render(
        &self,
        pipeline: &Pipeline,
        image: Image,
    ) -> Result<(Image, RenderReport, OptimizedBackend), OptimizedPipelineError> {
        let context = RenderContext::new(RenderQuality::Export);
        let mut ignore_progress = |_: RenderProgress| {};
        self.render_with_context(pipeline, image, &context, &mut ignore_progress)
    }

    /// Render with the shared cancellation, progress, and quality contract.
    ///
    /// # Errors
    ///
    /// Returns a pipeline validation, cancellation, image-processing, GPU
    /// device, or transfer error without silently changing backend.
    pub fn render_with_context<P: ProgressReporter + Send>(
        &self,
        pipeline: &Pipeline,
        image: Image,
        context: &RenderContext,
        progress: &mut P,
    ) -> Result<(Image, RenderReport, OptimizedBackend), OptimizedPipelineError> {
        #[cfg(feature = "gpu")]
        if let Some(gpu) = &self.gpu {
            match gpu.render_with_context(pipeline, &image, context, progress) {
                Ok((image, report)) => return Ok((image, report, OptimizedBackend::Gpu)),
                Err(crate::gpu::GpuError::UnsupportedPipeline(_)) => {}
                Err(error) => return Err(OptimizedPipelineError::Gpu(error)),
            }
        }

        let result = self
            .cpu_pool
            .install(|| pipeline.render_optimized_with_context(image, context, progress));
        let (image, report) = result.map_err(OptimizedPipelineError::Pipeline)?;
        Ok((image, report, OptimizedBackend::Cpu))
    }
}

fn create_cpu_pool() -> Result<Arc<rayon::ThreadPool>, OptimizedPipelineError> {
    static CPU_POOL: OnceLock<Arc<rayon::ThreadPool>> = OnceLock::new();
    if let Some(pool) = CPU_POOL.get() {
        return Ok(Arc::clone(pool));
    }
    let available = std::thread::available_parallelism().map_or(1, usize::from);
    let worker_count = available.saturating_sub(1).max(1);
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(worker_count)
        .thread_name(|index| format!("focal-optimized-{index}"))
        .build()
        .map(Arc::new)
        .map_err(|error| OptimizedPipelineError::Initialization(error.to_string()))?;
    let _ = CPU_POOL.set(Arc::clone(&pool));
    Ok(CPU_POOL.get().map_or(pool, Arc::clone))
}

#[derive(Debug)]
pub enum OptimizedPipelineError {
    Initialization(String),
    Pipeline(PipelineError),
    #[cfg(feature = "gpu")]
    Gpu(crate::gpu::GpuError),
}

impl fmt::Display for OptimizedPipelineError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Initialization(error) => {
                write!(
                    formatter,
                    "optimized executor initialization failed: {error}"
                )
            }
            Self::Pipeline(error) => write!(formatter, "optimized CPU render failed: {error}"),
            #[cfg(feature = "gpu")]
            Self::Gpu(error) => write!(formatter, "optimized GPU render failed: {error}"),
        }
    }
}

impl std::error::Error for OptimizedPipelineError {}
