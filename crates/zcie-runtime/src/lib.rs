//! Runtime parts: a bounded two-priority job queue, a worker pool and a shutdown signal.

pub mod error;
pub mod queue;
pub mod shutdown;
pub mod workers;

pub use error::{Error, Result};
pub use queue::{JobQueue, QueueSizes};
pub use shutdown::Shutdown;
pub use workers::{JobHandler, WorkerPool};
