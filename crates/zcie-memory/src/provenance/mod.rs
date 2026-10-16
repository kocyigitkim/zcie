//! Provenance rules: each memory record keeps its source events.

use zcie_types::MemoryRecord;

use crate::error::{Error, Result};

/// Returns an error if the record has no source event.
pub fn check_sources(record: &MemoryRecord) -> Result<()> {
    if record.source_event_ids.is_empty() {
        return Err(Error::Invalid(format!("memory {} has no source event", record.memory_id)));
    }
    Ok(())
}
