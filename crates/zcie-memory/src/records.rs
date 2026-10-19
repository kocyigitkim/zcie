//! Memory record helpers: make, store, read.

use zcie_store::values::Marker;
use zcie_store::{Batch, Cf, Store, layout};
use zcie_types::{
    EdgeStatus, EventEnvelope, EventId, Evidence, EvidenceKind, MemoryId, MemoryKind, MemoryRecord, MemoryStatus,
    Relation, Scope, truncate_chars,
};

use crate::error::Result;
use crate::provenance::check_sources;

/// Maximum characters of a memory text.
pub const MAX_MEMORY_TEXT: usize = 500;
/// Extractor name of Tier A.
pub const TIER_A: &str = "tier_a.v1";

/// Makes a new active memory record from one source event.
pub fn new_memory(scope: &Scope, kind: MemoryKind, text: &str, confidence: f32, source: &EventId, valid_from: &str) -> MemoryRecord {
    MemoryRecord {
        v: 1,
        memory_id: MemoryId::new(),
        scope: scope.clone(),
        kind,
        text: truncate_chars(text, MAX_MEMORY_TEXT),
        status: MemoryStatus::Active,
        confidence,
        source_event_ids: vec![source.clone()],
        valid_from: valid_from.to_string(),
        valid_until: None,
        last_verified_revision: None,
        superseded_by: None,
        extractor: TIER_A.to_string(),
        generation: 1,
        context: None,
    }
}

/// Returns true for the kinds that the decision index holds.
pub fn is_decision_kind(kind: MemoryKind) -> bool {
    matches!(kind, MemoryKind::DesignDecision | MemoryKind::Preference | MemoryKind::Constraint | MemoryKind::Correction)
}

/// Adds a memory record and its source lookups to the batch.
pub fn put_memory(batch: &mut Batch, record: &MemoryRecord) -> Result<()> {
    check_sources(record)?;
    let tenant = &record.scope.tenant_id;
    let id = record.memory_id.as_str();
    batch.put(Cf::Entities, layout::entity(tenant, "mem", id), record)?;
    if is_decision_kind(record.kind) {
        batch.put(Cf::Lookup, layout::decision(tenant, &record.scope.workspace_id, id), &Marker::new())?;
    }
    for source in &record.source_event_ids {
        batch.put(Cf::Lookup, layout::memory_of_event(tenant, source.as_str(), id), &Marker::new())?;
    }
    Ok(())
}

/// Reads a memory record, from the batch first.
pub fn get_memory(store: &Store, batch: &Batch, tenant: &str, memory_id: &str) -> Result<Option<MemoryRecord>> {
    let key = layout::entity(tenant, "mem", memory_id);
    if let Some(record) = batch.pending::<MemoryRecord>(Cf::Entities, &key) {
        return Ok(Some(record));
    }
    Ok(store.get(Cf::Entities, &key)?)
}

/// Returns the memory records made from one event.
pub fn memories_of_event(store: &Store, tenant: &str, event_id: &str) -> Result<Vec<MemoryRecord>> {
    let keys = store.prefix_keys(Cf::Lookup, &layout::memory_of_event_prefix(tenant, event_id), 1000)?;
    let mut out = Vec::new();
    for key in keys {
        let id = zcie_store::keys::last_part(&key);
        if let Some(record) = store.get::<MemoryRecord>(Cf::Entities, &layout::entity(tenant, "mem", &id))? {
            out.push(record);
        }
    }
    Ok(out)
}

/// Maximum decision records that one read returns.
pub const MAX_DECISIONS: usize = 10_000;

/// Returns the active and verified decision records of a workspace.
pub fn decisions_of_workspace(store: &Store, tenant: &str, workspace_id: &str) -> Result<Vec<MemoryRecord>> {
    let keys = store.prefix_keys(Cf::Lookup, &layout::decision_prefix(tenant, workspace_id), MAX_DECISIONS)?;
    let entity_keys: Vec<Vec<u8>> = keys.iter().map(|k| layout::entity(tenant, "mem", &zcie_store::keys::last_part(k))).collect();
    Ok(store
        .multi_get::<MemoryRecord>(Cf::Entities, &entity_keys)?
        .into_iter()
        .flatten()
        .filter(|r| matches!(r.status, zcie_types::MemoryStatus::Active | zcie_types::MemoryStatus::Verified))
        .collect())
}

/// Writes the decision index for records stored before the index existed. Runs once per store.
pub fn backfill_decision_index(store: &Store, tenant: &str) -> Result<u64> {
    let done = layout::migration(tenant, "decision_index_v1");
    if store.exists(Cf::Registry, &done)? {
        return Ok(0);
    }
    let mut batch = Batch::new();
    let mut count = 0;
    for (_, record) in store.prefix_scan::<MemoryRecord>(Cf::Entities, &layout::entity_prefix(tenant, "mem"), usize::MAX)? {
        if is_decision_kind(record.kind) {
            batch.put(Cf::Lookup, layout::decision(tenant, &record.scope.workspace_id, record.memory_id.as_str()), &Marker::new())?;
            count += 1;
        }
    }
    batch.put(Cf::Registry, done, &Marker::new())?;
    store.write_batch(batch, true)?;
    Ok(count)
}

/// Adds the edge `mem -DerivedFrom-> ev` (Verified, Derivation).
pub fn put_derived_from(store: &Store, batch: &mut Batch, record: &MemoryRecord, event: &EventEnvelope) -> Result<()> {
    let evidence = Evidence {
        kind: EvidenceKind::Derivation,
        source_event_ids: vec![event.event_id.clone()],
        revision: None,
        note: None,
    };
    let from = format!("mem:{}", record.memory_id);
    let to = format!("ev:{}", event.event_id);
    let edge = zcie_graph::new_edge(&record.scope, &from, Relation::DerivedFrom, &to, EdgeStatus::Verified, evidence);
    zcie_graph::put_edge(store, batch, edge)?;
    Ok(())
}

/// Makes evidence from one event and one kind.
pub fn evidence(kind: EvidenceKind, events: &[&EventId], note: Option<&str>) -> Evidence {
    Evidence {
        kind,
        source_event_ids: events.iter().map(|e| (*e).clone()).collect(),
        revision: None,
        note: note.map(|n| truncate_chars(n, 200)),
    }
}
