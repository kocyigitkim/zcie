//! Storage-level recovery helpers (`docs/06-storage.md` section 8).

use std::fs;
use std::path::Path;

use zcie_types::{EventEnvelope, JobRecord, JobState, now_utc};

use crate::codec::NumberValue;
use crate::error::Result;
use crate::keys::seq_part;
use crate::layout;
use crate::rocksdb::{Batch, Cf, Store};

/// Highest number of pending jobs that recovery loads at one time.
pub const MAX_PENDING_LOAD: usize = 1_000_000;

/// Reads the last assigned event sequence (recovery step 5).
pub fn read_event_seq(store: &Store, tenant: &str) -> Result<u64> {
    let value: Option<NumberValue> = store.get(Cf::Registry, &layout::event_seq_counter(tenant))?;
    Ok(value.map(|v| v.value).unwrap_or(0))
}

/// Returns the first `limit` pending jobs in key order (recovery step 7). The queue refill loads
/// the rest later, so that a large backlog does not fill the memory at the start.
pub fn first_pending_jobs(store: &Store, tenant: &str, limit: usize) -> Result<Vec<JobRecord>> {
    let items = store.prefix_scan::<JobRecord>(Cf::Jobs, &layout::pending_prefix(tenant), limit)?;
    Ok(items.into_iter().map(|(_, job)| job).collect())
}

/// Calls `f` with each page of `page` items under `prefix` (no full load in memory).
pub fn for_each_page<T: serde::de::DeserializeOwned>(store: &Store, cf: Cf, prefix: &[u8], page: usize, f: &mut dyn FnMut(Vec<(Vec<u8>, T)>) -> Result<()>) -> Result<()> {
    let mut start = prefix.to_vec();
    loop {
        let items = store.scan_from::<T>(cf, prefix, &start, page)?;
        let Some((last, _)) = items.last() else { return Ok(()) };
        start = last.clone();
        start.push(0);
        let full = items.len() == page;
        f(items)?;
        if !full {
            return Ok(());
        }
    }
}

/// Returns all pending jobs in key order (tests and tools; the daemon uses `first_pending_jobs`).
pub fn pending_jobs(store: &Store, tenant: &str) -> Result<Vec<JobRecord>> {
    let items = store.prefix_scan::<JobRecord>(Cf::Jobs, &layout::pending_prefix(tenant), MAX_PENDING_LOAD)?;
    Ok(items.into_iter().map(|(_, job)| job).collect())
}

/// Returns the lowest source sequence of all pending jobs.
pub fn lowest_pending_seq(store: &Store, tenant: &str) -> Result<Option<u64>> {
    let first = store.prefix_scan::<JobRecord>(Cf::Jobs, &layout::pending_prefix(tenant), 1)?;
    Ok(first.into_iter().next().map(|(_, job)| job.source_seq))
}

/// Moves a failed job to the dead-letter list and writes a JSON copy to `dead_letter_dir`.
pub fn move_to_dead_letter(store: &Store, tenant: &str, job: &JobRecord, error: &str, dead_letter_dir: &Path) -> Result<()> {
    let mut dead = job.clone();
    dead.state = JobState::DeadLetter;
    dead.last_error = Some(error.to_string());
    dead.updated_utc = now_utc();
    let mut batch = Batch::new();
    batch.delete(Cf::Jobs, layout::pending_job(tenant, job.source_seq, job.job_id.as_str()));
    batch.put(Cf::Jobs, layout::dead_job(tenant, job.job_id.as_str()), &dead)?;
    store.write_batch(batch, true)?;
    fs::create_dir_all(dead_letter_dir)?;
    let file = dead_letter_dir.join(format!("{}.json", job.job_id));
    fs::write(file, serde_json::to_vec_pretty(&dead)?)?;
    Ok(())
}

/// Returns the number of dead-letter jobs.
pub fn dead_letter_count(store: &Store, tenant: &str) -> Result<u64> {
    Ok(store.prefix_keys(Cf::Jobs, &layout::dead_prefix(tenant), usize::MAX)?.len() as u64)
}

/// Jobs that one page of the job sweep reads.
const SWEEP_PAGE: usize = 50_000;

/// Removes the dead-letter jobs whose workspace `keep` rejects (for example a deleted project).
/// Returns the number of removed jobs.
pub fn drop_dead_letters(store: &Store, tenant: &str, keep: impl Fn(&str) -> bool) -> Result<usize> {
    drop_jobs_under(store, &layout::dead_prefix(tenant), &|job: &JobRecord| keep(&job.scope.workspace_id))
}

/// Removes the pending jobs whose workspace `keep` rejects. They can never do work.
pub fn drop_pending_jobs(store: &Store, tenant: &str, keep: impl Fn(&str) -> bool) -> Result<usize> {
    drop_jobs_under(store, &layout::pending_prefix(tenant), &|job: &JobRecord| keep(&job.scope.workspace_id))
}

/// Deletes the pending jobs that `keep` rejects. Returns the number of deleted jobs.
pub fn drop_pending_where(store: &Store, tenant: &str, keep: impl Fn(&JobRecord) -> bool) -> Result<usize> {
    drop_jobs_under(store, &layout::pending_prefix(tenant), &keep)
}

/// Reads the jobs under `prefix` page by page (no full load in memory) and deletes the rejected ones.
fn drop_jobs_under(store: &Store, prefix: &[u8], keep: &dyn Fn(&JobRecord) -> bool) -> Result<usize> {
    let mut start = prefix.to_vec();
    let mut removed = 0;
    loop {
        let page = store.scan_from::<JobRecord>(Cf::Jobs, prefix, &start, SWEEP_PAGE)?;
        let Some((last, _)) = page.last() else { return Ok(removed) };
        start = last.clone();
        start.push(0);
        let mut batch = Batch::new();
        for (key, job) in &page {
            if !keep(job) {
                batch.delete(Cf::Jobs, key.clone());
                removed += 1;
            }
        }
        if !batch.is_empty() {
            store.write_batch(batch, true)?;
        }
        if page.len() < SWEEP_PAGE {
            return Ok(removed);
        }
    }
}

/// Returns events with `from_exclusive < seq <= to_inclusive` (recovery step 9).
pub fn events_in_range(store: &Store, tenant: &str, from_exclusive: u64, to_inclusive: u64) -> Result<Vec<EventEnvelope>> {
    if to_inclusive <= from_exclusive {
        return Ok(Vec::new());
    }
    let start = crate::keys::key(&[tenant, &seq_part(from_exclusive + 1)]);
    let limit = (to_inclusive - from_exclusive) as usize;
    let items = store.scan_from::<EventEnvelope>(Cf::Events, &layout::events_prefix(tenant), &start, limit)?;
    Ok(items.into_iter().map(|(_, event)| event).collect())
}
