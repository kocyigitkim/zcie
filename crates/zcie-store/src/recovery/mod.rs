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

/// Returns all pending jobs in key order (recovery step 7).
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
