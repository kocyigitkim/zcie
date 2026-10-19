//! Worker pool. Each worker is one OS thread that runs blocking job handlers.

use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

use zcie_types::JobRecord;

use crate::error::{Error, Result};
use crate::queue::JobQueue;
use crate::shutdown::Shutdown;

/// Time a worker waits for a job before it checks the shutdown signal.
const POLL_INTERVAL: Duration = Duration::from_millis(250);

/// A job handler. It runs on a worker thread.
pub type JobHandler = Arc<dyn Fn(JobRecord) + Send + Sync + 'static>;

/// A fixed set of worker threads.
pub struct WorkerPool {
    handles: Vec<JoinHandle<()>>,
}

impl WorkerPool {
    /// Starts `count` workers that take jobs from `queue` and call `handler`. With two or more
    /// workers, worker 0 takes interactive jobs only: a captured message is processed at once even
    /// while the other workers index a large repository (slow background jobs).
    pub fn start(count: usize, queue: Arc<JobQueue>, shutdown: Shutdown, handler: JobHandler) -> Result<WorkerPool> {
        let mut handles = Vec::with_capacity(count);
        let count = count.max(1);
        for index in 0..count {
            let queue = Arc::clone(&queue);
            let shutdown = shutdown.clone();
            let handler = Arc::clone(&handler);
            let interactive_only = index == 0 && count > 1;
            let handle = std::thread::Builder::new()
                .name(format!("zcie-worker-{index}"))
                .spawn(move || worker_loop(&queue, &shutdown, &handler, interactive_only))
                .map_err(|e| Error::Spawn(e.to_string()))?;
            handles.push(handle);
        }
        Ok(WorkerPool { handles })
    }

    /// Waits until all workers stop. Call it after the shutdown signal.
    pub fn join(self) {
        for handle in self.handles {
            if handle.join().is_err() {
                tracing::error!("a worker thread panicked");
            }
        }
    }
}

fn worker_loop(queue: &JobQueue, shutdown: &Shutdown, handler: &JobHandler, interactive_only: bool) {
    while !shutdown.is_triggered() {
        let job = if interactive_only { queue.pop_interactive(POLL_INTERVAL) } else { queue.pop(POLL_INTERVAL) };
        if let Some(job) = job {
            handler(job);
        }
    }
}
