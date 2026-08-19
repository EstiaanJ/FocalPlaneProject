use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use crate::module::ModuleKind;

/// Selects the intended use of a render without changing its processing
/// meaning. Preview approximations can be added only after they are defined
/// and tested explicitly.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RenderQuality {
    #[default]
    Preview,
    Export,
}

/// Cooperative cancellation shared by a render request and its caller.
#[derive(Clone, Debug, Default)]
pub struct CancellationToken {
    cancelled: Arc<AtomicBool>,
}

impl CancellationToken {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Marks the associated render obsolete.
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
}

/// Immutable execution inputs shared by all stages of one render.
#[derive(Clone, Debug)]
pub struct RenderContext {
    cancellation: CancellationToken,
    quality: RenderQuality,
}

impl RenderContext {
    #[must_use]
    pub fn new(quality: RenderQuality) -> Self {
        Self::with_cancellation(quality, CancellationToken::new())
    }

    #[must_use]
    pub fn with_cancellation(quality: RenderQuality, cancellation: CancellationToken) -> Self {
        Self {
            cancellation,
            quality,
        }
    }

    #[must_use]
    pub const fn quality(&self) -> RenderQuality {
        self.quality
    }

    #[must_use]
    pub fn cancellation_token(&self) -> CancellationToken {
        self.cancellation.clone()
    }

    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.cancellation.is_cancelled()
    }
}

impl Default for RenderContext {
    fn default() -> Self {
        Self::new(RenderQuality::default())
    }
}

/// Progress for one immutable render request.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RenderProgress {
    pub fraction: f32,
    pub completed_stages: usize,
    pub total_stages: usize,
    pub current_module: Option<ModuleKind>,
    pub current_module_index: Option<usize>,
}

/// Receives progress without coupling `FocalCore` to a GUI or event system.
pub trait ProgressReporter {
    fn report(&mut self, progress: RenderProgress);
}

impl<F> ProgressReporter for F
where
    F: FnMut(RenderProgress),
{
    fn report(&mut self, progress: RenderProgress) {
        self(progress);
    }
}
