//! Context of follow-up messages for records that were stored before the context existed
//! (ADR-013). Runs once per store.

use zcie_store::values::Marker;
use zcie_store::recovery::for_each_page;
use zcie_store::{Batch, Cf, ObjectStore, Store, layout};
use zcie_types::{MemoryKind, MemoryRecord, MemoryStatus};

use crate::capture::previous_user_message;
use crate::error::Result;

use super::{MAX_FOLLOW_UP_CHARS, is_shell_input};

/// Memory records that one page of the migration reads.
const PAGE: usize = 1_000;

/// Retracts the request records of shell commands typed with "!" (stored before
/// `is_shell_input` existed). Returns the number of changed records.
pub fn retract_shell_inputs(store: &Store, tenant: &str) -> Result<u64> {
    let done = layout::migration(tenant, "shell_input_v1");
    if store.exists(Cf::Registry, &done)? {
        return Ok(0);
    }
    let mut count = 0;
    for_each_page::<MemoryRecord>(store, Cf::Entities, &layout::entity_prefix(tenant, "mem"), PAGE, &mut |page| {
        let mut batch = Batch::new();
        for (key, mut record) in page {
            if record.kind == MemoryKind::UserRequirement && record.status == MemoryStatus::Active && is_shell_input(&record.text) {
                record.status = MemoryStatus::Retracted;
                record.generation += 1;
                batch.put(Cf::Entities, key, &record)?;
                count += 1;
            }
        }
        store.write_batch(batch, false)
    })?;
    store.put_sync(Cf::Registry, &done, &Marker::new())?;
    Ok(count)
}

/// Sets `context` on the request records of short messages. Returns the number of changed
/// records; the caller rebuilds the search index when it is not zero.
pub fn backfill_context(store: &Store, objects: &ObjectStore, tenant: &str) -> Result<u64> {
    // v2: a shell command typed with "!" is not the context (v1 used it).
    let done = layout::migration(tenant, "follow_up_context_v2");
    if store.exists(Cf::Registry, &done)? {
        return Ok(0);
    }
    let mut candidates: Vec<MemoryRecord> = Vec::new();
    for_each_page::<MemoryRecord>(store, Cf::Entities, &layout::entity_prefix(tenant, "mem"), PAGE, &mut |page| {
        let short = |r: &MemoryRecord| r.text.chars().count() <= MAX_FOLLOW_UP_CHARS;
        let open = |r: &MemoryRecord| r.context.as_deref().is_none_or(is_shell_input);
        candidates.extend(page.into_iter().map(|(_, r)| r).filter(|r| r.kind == MemoryKind::UserRequirement && open(r) && short(r)));
        Ok(())
    })?;
    let mut count = 0;
    for chunk in candidates.chunks_mut(PAGE) {
        let mut batch = Batch::new();
        for record in chunk {
            let Some(source) = record.source_event_ids.first() else { continue };
            let Some(event) = zcie_store::events::get_event_by_id(store, tenant, source.as_str())? else { continue };
            let context = previous_user_message(store, objects, &event)?;
            if context == record.context {
                continue;
            }
            record.context = context;
            record.generation += 1;
            batch.put(Cf::Entities, layout::entity(tenant, "mem", record.memory_id.as_str()), &*record)?;
            count += 1;
        }
        store.write_batch(batch, false)?;
    }
    store.put_sync(Cf::Registry, &done, &Marker::new())?;
    Ok(count)
}
