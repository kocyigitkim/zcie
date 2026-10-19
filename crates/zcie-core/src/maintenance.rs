//! Background maintenance: queue refill and the orphan blob sweep (`docs/06-storage.md` section 11).

use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use zcie_api::Engine;
use zcie_runtime::Shutdown;
use zcie_types::LOCAL_TENANT;

/// Interval of the queue refill.
const REFILL_INTERVAL: Duration = Duration::from_secs(2);
/// Number of jobs that one refill loads.
const REFILL_BATCH: usize = 1000;
/// Interval of the orphan blob sweep.
const SWEEP_INTERVAL: Duration = Duration::from_secs(3600);
/// First sweep after startup.
const FIRST_SWEEP: Duration = Duration::from_secs(30);
/// Minimum age of a blob that the sweep can delete.
const SWEEP_MIN_AGE: Duration = Duration::from_secs(600);
/// Sleep step of the loop.
const STEP: Duration = Duration::from_millis(250);

/// The maintenance thread.
pub struct Maintenance {
    handle: Option<JoinHandle<()>>,
}

impl Maintenance {
    /// Starts the maintenance thread.
    pub fn start(engine: Arc<Engine>, shutdown: Shutdown) -> Maintenance {
        let handle = std::thread::Builder::new()
            .name("zcie-maintenance".into())
            .spawn(move || run(&engine, &shutdown))
            .map_err(|error| tracing::error!(%error, "maintenance thread did not start"))
            .ok();
        Maintenance { handle }
    }

    /// Waits for the thread after the shutdown signal.
    pub fn join(mut self) {
        if let Some(handle) = self.handle.take()
            && handle.join().is_err()
        {
            tracing::error!("maintenance thread panicked");
        }
    }
}

fn run(engine: &Engine, shutdown: &Shutdown) {
    let mut last_refill = Instant::now();
    let mut next_sweep = Instant::now() + FIRST_SWEEP;
    while !shutdown.is_triggered() {
        std::thread::sleep(STEP);
        if last_refill.elapsed() >= REFILL_INTERVAL {
            last_refill = Instant::now();
            if let Err(error) = engine.ingest.refill_queue(REFILL_BATCH) {
                tracing::warn!(%error, "queue refill failed");
            }
            let _ = engine.ingest.advance_graph_watermark();
        }
        if Instant::now() >= next_sweep {
            next_sweep = Instant::now() + SWEEP_INTERVAL;
            match engine.objects.sweep_orphans(&engine.store, LOCAL_TENANT, SWEEP_MIN_AGE) {
                Ok(removed) => tracing::info!(removed, "orphan blob sweep completed"),
                Err(error) => tracing::warn!(%error, "orphan blob sweep failed"),
            }
            match engine.drop_orphan_jobs() {
                Ok(removed) => tracing::info!(removed, "jobs of deleted projects removed"),
                Err(error) => tracing::warn!(%error, "dead letter sweep failed"),
            }
        }
    }
}
