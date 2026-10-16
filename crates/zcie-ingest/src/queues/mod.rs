//! Job runner: dispatch, commit, retry, dead-letter and graph watermark (`docs/07-ingest-and-indexing.md` section 3).

use zcie_store::layout;
use zcie_store::recovery::{lowest_pending_seq, move_to_dead_letter};
use zcie_store::watermarks::{GRAPH, raise_watermark};
use zcie_store::Cf;
use zcie_types::{JobId, JobKind, JobPriority, JobRecord, Scope, now_utc};
use zcie_types::job::file_target;

use crate::error::Result;
use crate::ingest::Ingest;
use crate::processors::Output;

/// Number of failed attempts before a job goes to the dead-letter list.
pub const MAX_ATTEMPTS: u32 = 3;
/// Number of stored jobs that a worker loads when the queue is empty.
const REFILL_ON_EMPTY: usize = 64;

impl Ingest {
    /// Runs one job. Logs errors and applies the retry rule. Never panics.
    pub fn handle_job(&self, job: JobRecord) {
        if !self.mark_running(&job) {
            return;
        }
        let result = self.run_job(&job);
        self.unmark_running(&job);
        match result {
            Ok(()) => {
                zcie_telemetry::Counters::inc(&self.counters.jobs_done);
                self.refill_when_empty();
            }
            Err(error) => {
                zcie_telemetry::Counters::inc(&self.counters.jobs_failed);
                self.fail_job(job, &error.to_string());
            }
        }
    }

    /// Loads stored pending jobs when the in-memory queue is empty, so a small queue drains fast.
    fn refill_when_empty(&self) {
        let sizes = self.queue.sizes();
        if sizes.interactive + sizes.background == 0
            && let Err(error) = self.refill_queue(REFILL_ON_EMPTY)
        {
            tracing::warn!(%error, "queue refill failed");
        }
    }

    fn mark_running(&self, job: &JobRecord) -> bool {
        let mut running = self.running.lock().unwrap_or_else(|e| e.into_inner());
        running.insert(job.job_id.0.clone())
    }

    fn unmark_running(&self, job: &JobRecord) {
        let mut running = self.running.lock().unwrap_or_else(|e| e.into_inner());
        running.remove(job.job_id.as_str());
    }

    /// Runs the handler of a job and commits its output. A job with no pending key is skipped.
    pub fn run_job(&self, job: &JobRecord) -> Result<()> {
        let pending = layout::pending_job(self.tenant(), job.source_seq, job.job_id.as_str());
        if !self.store.exists(Cf::Jobs, &pending)? {
            return Ok(());
        }
        match job.kind {
            JobKind::IndexFile => {
                let prepared = self.prepare_file(job)?;
                let _guard = self.lock_write();
                let output = match prepared {
                    Some(prepared) => self.index_file(job, prepared)?,
                    None => self.delete_file(job)?,
                };
                self.commit_output(job, output)
            }
            JobKind::UpdateSearch => {
                let output = self.rebuild_search_job(job)?;
                let _guard = self.lock_write();
                self.commit_output(job, output)
            }
            _ => {
                let _guard = self.lock_write();
                let output = self.dispatch(job)?;
                self.commit_output(job, output)
            }
        }
    }

    fn dispatch(&self, job: &JobRecord) -> Result<Output> {
        match job.kind {
            JobKind::ExtractMemory => self.process_extract(job),
            JobKind::DeleteFile => self.delete_file(job),
            JobKind::ScanRepository => self.scan_worktree(job),
            JobKind::IndexCommit => self.index_commit(job),
            JobKind::IndexFile | JobKind::UpdateSearch => Ok(Output::default()),
        }
    }

    /// Writes the output, deletes the pending key, sends search operations and raises `graph_seq`.
    fn commit_output(&self, job: &JobRecord, mut output: Output) -> Result<()> {
        let t = self.tenant().to_string();
        output.batch.delete(Cf::Jobs, layout::pending_job(&t, job.source_seq, job.job_id.as_str()));
        for new_job in &output.new_jobs {
            output.batch.put(Cf::Jobs, layout::pending_job(&t, new_job.source_seq, new_job.job_id.as_str()), new_job)?;
        }
        let held = output.search_ops.len() as u64;
        self.search.hold(held);
        if let Err(error) = self.store.write_batch(output.batch, false) {
            self.search.release(held);
            return Err(error.into());
        }
        for op in output.search_ops {
            if self.search.send(op).is_err() {
                tracing::error!("search writer is stopped");
            }
        }
        self.search.release(held);
        for (worktree_id, path) in &output.done_paths {
            self.overlay.remove(worktree_id, path);
        }
        for new_job in output.new_jobs {
            self.push_job(new_job);
        }
        self.advance_graph_watermark()
    }

    /// Raises `graph_seq` to (lowest pending seq − 1), or to the canonical seq if no job is pending.
    pub fn advance_graph_watermark(&self) -> Result<()> {
        let canonical = self.event_seq();
        let target = match lowest_pending_seq(&self.store, self.tenant())? {
            Some(seq) => seq.saturating_sub(1).min(canonical),
            None => canonical,
        };
        raise_watermark(&self.store, self.tenant(), GRAPH, target)?;
        Ok(())
    }

    fn fail_job(&self, mut job: JobRecord, error: &str) {
        job.attempts += 1;
        job.last_error = Some(error.to_string());
        job.updated_utc = now_utc();
        tracing::warn!(job = %job.job_id, kind = ?job.kind, attempts = job.attempts, error, "job failed");
        let t = self.tenant().to_string();
        if job.attempts >= MAX_ATTEMPTS {
            if let Err(e) = move_to_dead_letter(&self.store, &t, &job, error, &self.config.dead_letter_dir) {
                tracing::error!(error = %e, "dead-letter write failed");
            }
            let _ = self.advance_graph_watermark();
            return;
        }
        let key = layout::pending_job(&t, job.source_seq, job.job_id.as_str());
        if let Err(e) = self.store.put_sync(Cf::Jobs, &key, &job) {
            tracing::error!(error = %e, "job retry write failed");
            return;
        }
        self.push_job(job);
    }

    fn push_job(&self, job: JobRecord) {
        if self.queue.push(job).is_err() {
            tracing::debug!("job queue is full; the job stays in the jobs column family");
        }
    }

    /// Stores a job durably and puts it in the queue.
    pub fn enqueue_job(&self, job: JobRecord) -> Result<JobId> {
        let key = layout::pending_job(self.tenant(), job.source_seq, job.job_id.as_str());
        self.store.put_sync(Cf::Jobs, &key, &job)?;
        let id = job.job_id.clone();
        self.push_job(job);
        Ok(id)
    }

    /// Stores an `IndexFile` or `DeleteFile` job for a watcher change.
    pub fn enqueue_file_job(&self, scope: &Scope, worktree_id: &str, rel_path: &str, kind: JobKind) -> Result<JobId> {
        let target = file_target(worktree_id, rel_path);
        let job = JobRecord::new(kind, scope.clone(), JobPriority::Background, self.event_seq(), target);
        self.overlay.add(worktree_id, rel_path);
        self.enqueue_job(job)
    }

    /// Puts stored pending jobs into the queue while it has room. Returns the number of jobs.
    pub fn refill_queue(&self, max: usize) -> Result<usize> {
        let mut count = 0;
        let prefix = layout::pending_prefix(self.tenant());
        let scan_limit = max.saturating_mul(2).max(1);
        for (_, job) in self.store.prefix_scan::<JobRecord>(Cf::Jobs, &prefix, scan_limit)? {
            if count >= max || !self.queue.has_room(1) {
                break;
            }
            let running = self.running.lock().unwrap_or_else(|e| e.into_inner()).contains(job.job_id.as_str());
            if running {
                continue;
            }
            if matches!(job.kind, JobKind::IndexFile | JobKind::DeleteFile)
                && let Some((worktree_id, path)) = zcie_types::job::split_file_target(&job.target)
            {
                self.overlay.add(worktree_id, path);
            }
            if self.queue.push(job).is_ok() {
                count += 1;
            }
        }
        Ok(count)
    }
}
