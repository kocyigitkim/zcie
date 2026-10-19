//! Simple timers and counters.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::activity::ActivityLog;
use std::time::Instant;

/// Measures the duration of an operation. Logs it at `debug` level on drop.
#[derive(Debug)]
pub struct Timer {
    name: &'static str,
    start: Instant,
}

impl Timer {
    /// Starts a timer with a name.
    pub fn start(name: &'static str) -> Timer {
        Timer { name, start: Instant::now() }
    }

    /// Returns the time since the start in milliseconds.
    pub fn elapsed_ms(&self) -> f64 {
        self.start.elapsed().as_secs_f64() * 1000.0
    }
}

impl Drop for Timer {
    fn drop(&mut self) {
        tracing::debug!(name = self.name, elapsed_ms = self.elapsed_ms(), "timer");
    }
}

/// Process-wide counters of the daemon.
#[derive(Debug, Default)]
pub struct Counters {
    /// Captured events.
    pub events_captured: AtomicU64,
    /// Completed jobs.
    pub jobs_done: AtomicU64,
    /// Failed job attempts.
    pub jobs_failed: AtomicU64,
    /// Queries served.
    pub queries: AtomicU64,
    /// L0 cache hits.
    pub cache_hits: AtomicU64,
    /// The activity log (ADR-023); set at startup, none in tests.
    pub activity: OnceLock<ActivityLog>,
}

impl Counters {
    /// Adds one to a counter.
    pub fn inc(counter: &AtomicU64) {
        counter.fetch_add(1, Ordering::Relaxed);
    }

    /// Appends one record to the activity log, if there is one.
    pub fn record(&self, value: serde_json::Value) {
        if let Some(log) = self.activity.get() {
            log.record(value);
        }
    }

    /// Reads a counter.
    pub fn get(counter: &AtomicU64) -> u64 {
        counter.load(Ordering::Relaxed)
    }
}
