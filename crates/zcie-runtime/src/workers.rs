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
    /// Starts `count` workers that take jobs from `queue` and call `handler`.
    pub fn start(count: usize, queue: Arc<JobQueue>, shutdown: Shutdown, handler: JobHandler) -> Result<WorkerPool> {
        let mut handles = Vec::with_capacity(count);
        for index in 0..count.max(1) {
            let queue = Arc::clone(&queue);
            let shutdown = shutdown.clone();
            let handler = Arc::clone(&handler);
            let handle = std::thread::Builder::new()
                .name(format!("zcie-worker-{index}"))
                .spawn(move || worker_loop(&queue, &shutdown, &handler))
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

fn worker_loop(queue: &JobQueue, shutdown: &Shutdown, handler: &JobHandler) {
    while !shutdown.is_triggered() {
        if let Some(job) = queue.pop(POLL_INTERVAL) {
            handler(job);
        }
    }
}
