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
