//! Corrections, contradictions and retractions (`docs/08-memory-and-graph.md` section 6).

use zcie_store::{Batch, Store};
use zcie_types::{EdgeStatus, EventEnvelope, EventId, EvidenceKind, MemoryRecord, MemoryStatus, Relation};

use crate::error::{Error, Result};
use crate::records::{evidence, get_memory, new_memory, put_memory};

/// Makes a corrected copy of a memory record. The old record becomes `Superseded`.
pub fn correct_memory(store: &Store, batch: &mut Batch, tenant: &str, memory_id: &str, text: &str, source: &EventEnvelope) -> Result<MemoryRecord> {
    let mut old = get_memory(store, batch, tenant, memory_id)?.ok_or_else(|| Error::NotFound(format!("mem:{memory_id}")))?;
    if old.status == MemoryStatus::Retracted {
        return Err(Error::Invalid(format!("mem:{memory_id} is retracted")));
    }
    let mut new = new_memory(&old.scope, old.kind, text, 1.0, &source.event_id, &source.timestamp_utc);
    new.extractor = "manual".to_string();
    old.status = MemoryStatus::Superseded;
    old.superseded_by = Some(new.memory_id.clone());
    old.generation += 1;
    put_memory(batch, &old)?;
    put_memory(batch, &new)?;
    let evidence = evidence(EvidenceKind::UserConfirmation, &[&source.event_id], Some("correction"));
    let from = format!("mem:{}", new.memory_id);
    let to = format!("mem:{}", old.memory_id);
    let edge = zcie_graph::new_edge(&new.scope, &from, Relation::Supersedes, &to, EdgeStatus::Verified, evidence);
    zcie_graph::put_edge(store, batch, edge)?;
    Ok(new)
}

/// Marks two memory records `Disputed` and adds a `Contradicts` edge.
pub fn mark_contradiction(store: &Store, batch: &mut Batch, event: &EventEnvelope, first_ref: &str, second_ref: &str) -> Result<()> {
    let tenant = &event.scope.tenant_id;
    for entity_ref in [first_ref, second_ref] {
        let Some(id) = entity_ref.strip_prefix("mem:") else { continue };
        if let Some(mut record) = get_memory(store, batch, tenant, id)? {
            record.status = MemoryStatus::Disputed;
            record.generation += 1;
            put_memory(batch, &record)?;
        }
    }
    let evidence = evidence(EvidenceKind::UserConfirmation, &[&event.event_id], None);
    let edge = zcie_graph::new_edge(&event.scope, first_ref, Relation::Contradicts, second_ref, EdgeStatus::Verified, evidence);
    zcie_graph::put_edge(store, batch, edge)?;
    Ok(())
}

/// Retracts a memory record: status `Retracted`, empty text.
pub fn retract(store: &Store, batch: &mut Batch, tenant: &str, memory_id: &str) -> Result<bool> {
    let Some(mut record) = get_memory(store, batch, tenant, memory_id)? else { return Ok(false) };
    if record.status == MemoryStatus::Retracted {
        return Ok(false);
    }
    record.status = MemoryStatus::Retracted;
    record.text = String::new();
    record.generation += 1;
    put_memory(batch, &record)?;
    Ok(true)
}

/// Returns true if all source events of the record are in `forgotten`.
pub fn only_from(record: &MemoryRecord, forgotten: &[EventId]) -> bool {
    record.source_event_ids.iter().all(|id| forgotten.contains(id))
}
