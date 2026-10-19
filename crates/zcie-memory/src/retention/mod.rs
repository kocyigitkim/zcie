//! Retention helpers for `forget` (`docs/11-security.md` section 7).

use zcie_store::{Batch, Store};
use zcie_types::EventId;

use crate::conflict::{only_from, retract};
use crate::error::Result;
use crate::records::memories_of_event;

/// Retracts the memory records that come only from forgotten events. Returns their refs.
pub fn retract_records_of_events(store: &Store, batch: &mut Batch, tenant: &str, forgotten: &[EventId]) -> Result<Vec<String>> {
    let mut refs = Vec::new();
    for event_id in forgotten {
        for record in memories_of_event(store, tenant, event_id.as_str())? {
            let memory_ref = format!("mem:{}", record.memory_id);
            if only_from(&record, forgotten) && !refs.contains(&memory_ref) && retract(store, batch, tenant, record.memory_id.as_str())? {
                refs.push(memory_ref);
            }
        }
    }
    Ok(refs)
}

/// Deletes the tasks that a forgotten event made (`created_from`), with their external ref lookup.
/// Returns the deleted task refs, so that the caller removes their edges and search documents.
pub fn delete_tasks_of_events(store: &Store, batch: &mut Batch, tenant: &str, forgotten: &[EventId]) -> Result<Vec<String>> {
    let mut refs = Vec::new();
    let prefix = zcie_store::layout::entity_prefix(tenant, "task");
    zcie_store::recovery::for_each_page::<zcie_types::TaskRecord>(store, zcie_store::Cf::Entities, &prefix, 1_000, &mut |page| {
        for (key, task) in page.into_iter().filter(|(_, t)| forgotten.contains(&t.created_from)) {
            batch.delete(zcie_store::Cf::Entities, key);
            if let Some(external) = &task.external_ref {
                batch.delete(zcie_store::Cf::Lookup, zcie_store::layout::task_external(tenant, &task.scope.workspace_id, external));
            }
            refs.push(format!("task:{}", task.task_id));
        }
        Ok(())
    })?;
    Ok(refs)
}
