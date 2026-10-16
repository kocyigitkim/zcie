//! Shutdown signal shared by all workers.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// A shared stop flag.
#[derive(Debug, Clone, Default)]
pub struct Shutdown {
    stopped: Arc<AtomicBool>,
}

impl Shutdown {
    /// Makes a new signal in the running state.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the signal to stopped.
    pub fn trigger(&self) {
        self.stopped.store(true, Ordering::SeqCst);
    }

    /// Returns true after `trigger`.
    pub fn is_triggered(&self) -> bool {
        self.stopped.load(Ordering::SeqCst)
    }
}
