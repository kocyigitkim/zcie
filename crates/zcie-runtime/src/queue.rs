//! Bounded two-priority job queue (`docs/07-ingest-and-indexing.md` sections 3.3 and 3.4).

use std::collections::{HashSet, VecDeque};
use std::sync::{Condvar, Mutex};
use std::time::Duration;

use zcie_types::{JobPriority, JobRecord};

use crate::error::{Error, Result};

#[derive(Default)]
struct Queues {
    interactive: VecDeque<JobRecord>,
    background: VecDeque<JobRecord>,
    queued_ids: HashSet<String>,
    closed: bool,
}

/// A bounded in-memory job queue. Interactive jobs come out before background jobs.
pub struct JobQueue {
    queues: Mutex<Queues>,
    ready: Condvar,
    capacity: usize,
}

/// Sizes of the two queues.
#[derive(Debug, Clone, Copy, Default)]
pub struct QueueSizes {
    /// Interactive jobs waiting.
    pub interactive: usize,
    /// Background jobs waiting.
    pub background: usize,
}

impl JobQueue {
    /// Makes a queue with a total capacity.
    pub fn new(capacity: usize) -> Self {
        Self { queues: Mutex::new(Queues::default()), ready: Condvar::new(), capacity: capacity.max(1) }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Queues> {
        self.queues.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Adds a job. A job ID that is already queued is ignored. Returns `Busy` when full.
    pub fn push(&self, job: JobRecord) -> Result<()> {
        let mut queues = self.lock();
        if queues.closed {
            return Err(Error::ShuttingDown);
        }
        if queues.queued_ids.contains(job.job_id.as_str()) {
            return Ok(());
        }
        if queues.interactive.len() + queues.background.len() >= self.capacity {
            return Err(Error::Busy);
        }
        queues.queued_ids.insert(job.job_id.0.clone());
        match job.priority {
            JobPriority::Interactive => queues.interactive.push_back(job),
            JobPriority::Background => queues.background.push_back(job),
        }
        self.ready.notify_one();
        Ok(())
    }

    /// Takes the next job. Waits at most `timeout`. Returns `None` on timeout or close.
    pub fn pop(&self, timeout: Duration) -> Option<JobRecord> {
        let mut queues = self.lock();
        if queues.interactive.is_empty() && queues.background.is_empty() && !queues.closed {
            queues = self.ready.wait_timeout(queues, timeout).map(|r| r.0).unwrap_or_else(|e| e.into_inner().0);
        }
        let job = queues.interactive.pop_front().or_else(|| queues.background.pop_front())?;
        queues.queued_ids.remove(job.job_id.as_str());
        Some(job)
    }

    /// Returns true if the queue has room for `count` more jobs.
    pub fn has_room(&self, count: usize) -> bool {
        let queues = self.lock();
        queues.interactive.len() + queues.background.len() + count <= self.capacity
    }

    /// Returns the current sizes.
    pub fn sizes(&self) -> QueueSizes {
        let queues = self.lock();
        QueueSizes { interactive: queues.interactive.len(), background: queues.background.len() }
    }

    /// Closes the queue. Waiting workers wake up.
    pub fn close(&self) {
        self.lock().closed = true;
        self.ready.notify_all();
    }
}
