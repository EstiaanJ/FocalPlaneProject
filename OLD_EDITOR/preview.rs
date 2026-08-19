/// Identifies one immutable preview request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PreviewGeneration(u64);

/// Tracks which asynchronous preview request is current.
///
/// Rendering will be added later. Keeping this policy independent of the GUI
/// makes stale-result rejection deterministic and testable.
#[derive(Debug, Default)]
pub struct PreviewTracker {
    latest_generation: u64,
    pending: bool,
}

impl PreviewTracker {
    #[must_use]
    pub fn begin_request(&mut self) -> PreviewGeneration {
        self.latest_generation = self.latest_generation.saturating_add(1);
        self.pending = true;
        PreviewGeneration(self.latest_generation)
    }

    #[must_use]
    pub fn accepts(&self, generation: PreviewGeneration) -> bool {
        self.pending && generation.0 == self.latest_generation
    }

    pub fn complete(&mut self, generation: PreviewGeneration) -> bool {
        if !self.accepts(generation) {
            return false;
        }

        self.pending = false;
        true
    }

    /// Invalidates any pending request without starting another one.
    pub fn invalidate(&mut self) {
        self.latest_generation = self.latest_generation.saturating_add(1);
        self.pending = false;
    }

    #[must_use]
    pub const fn is_pending(&self) -> bool {
        self.pending
    }
}

/// Owns the cancellation lifetime of the currently active preview.
#[derive(Debug, Default)]
pub struct PreviewCancellation {
    active: Option<CancellationToken>,
}

impl PreviewCancellation {
    #[must_use]
    pub fn begin(&mut self) -> CancellationToken {
        if let Some(active) = self.active.take() {
            active.cancel();
        }
        let next = CancellationToken::new();
        self.active = Some(next.clone());
        next
    }

    /// Cancels any active preview without creating a replacement request.
    pub fn cancel(&mut self) {
        if let Some(active) = self.active.take() {
            active.cancel();
        }
    }
}
use focal_engine::CancellationToken;
