//! Event read helpers and lookup values of the capture path.

use serde::{Deserialize, Serialize};
use zcie_types::{CaptureReceiptValue, EventEnvelope};

use crate::error::Result;
use crate::layout;
use crate::objects::ObjectStore;
use crate::rocksdb::{Cf, Store};

/// Lookup value `t/"event_id"/<id>` → sequence.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct SeqValue {
    /// Value schema version.
    pub v: u32,
    /// The event sequence.
    pub seq: u64,
}

/// Lookup value of a blob: size and number of events that refer to it.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct BlobValue {
    /// Value schema version.
    pub v: u32,
    /// Uncompressed size in bytes.
    pub size: u64,
    /// Number of references.
    pub ref_count: u64,
}

/// Reads an event by sequence.
pub fn get_event(store: &Store, tenant: &str, seq: u64) -> Result<Option<EventEnvelope>> {
    store.get(Cf::Events, &layout::event(tenant, seq))
}

/// Reads an event by ID.
pub fn get_event_by_id(store: &Store, tenant: &str, event_id: &str) -> Result<Option<EventEnvelope>> {
    match store.get::<SeqValue>(Cf::Lookup, &layout::event_id(tenant, event_id))? {
        Some(value) => get_event(store, tenant, value.seq),
        None => Ok(None),
    }
}

/// Reads the stored receipt of an idempotency key.
pub fn get_receipt(store: &Store, tenant: &str, idempotency_key: &str) -> Result<Option<CaptureReceiptValue>> {
    store.get(Cf::Lookup, &layout::idem(tenant, idempotency_key))
}

/// Reads the payload text of an event. Returns `None` if the event has no payload.
pub fn payload_text(objects: &ObjectStore, event: &EventEnvelope) -> Result<Option<String>> {
    match &event.payload_ref {
        Some(hash) if !event.forgotten => Ok(Some(objects.get_text(hash)?)),
        _ => Ok(None),
    }
}
