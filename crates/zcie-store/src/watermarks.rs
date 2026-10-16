//! Watermarks (`docs/06-storage.md` section 7).

use zcie_types::Watermarks;

use crate::codec::NumberValue;
use crate::error::Result;
use crate::layout;
use crate::rocksdb::{Batch, Cf, Store};

/// Name of the canonical watermark.
pub const CANONICAL: &str = "canonical_seq";
/// Name of the graph watermark.
pub const GRAPH: &str = "graph_seq";
/// Name of the search watermark.
pub const SEARCH: &str = "search_seq";

/// Reads one watermark. A missing watermark is 0.
pub fn read_watermark(store: &Store, tenant: &str, name: &str) -> Result<u64> {
    let value: Option<NumberValue> = store.get(Cf::Revisions, &layout::watermark(tenant, name))?;
    Ok(value.map(|v| v.value).unwrap_or(0))
}

/// Reads all three watermarks.
pub fn read_watermarks(store: &Store, tenant: &str) -> Result<Watermarks> {
    Ok(Watermarks {
        canonical_seq: read_watermark(store, tenant, CANONICAL)?,
        graph_seq: read_watermark(store, tenant, GRAPH)?,
        search_seq: read_watermark(store, tenant, SEARCH)?,
    })
}

/// Adds a watermark put to a batch. The caller makes sure the value does not decrease.
pub fn put_watermark(batch: &mut Batch, tenant: &str, name: &str, value: u64) -> Result<()> {
    batch.put(Cf::Revisions, layout::watermark(tenant, name), &NumberValue::new(value))
}

/// Raises a watermark to `value`. A lower value is ignored. Returns the stored value.
pub fn raise_watermark(store: &Store, tenant: &str, name: &str, value: u64) -> Result<u64> {
    let _guard = store.watermark_lock.lock().unwrap_or_else(|e| e.into_inner());
    let current = read_watermark(store, tenant, name)?;
    if value <= current {
        return Ok(current);
    }
    store.put_sync(Cf::Revisions, &layout::watermark(tenant, name), &NumberValue::new(value))?;
    Ok(value)
}

/// Sets a watermark without the increase check. Only the search rebuild uses it.
pub fn reset_watermark(store: &Store, tenant: &str, name: &str, value: u64) -> Result<()> {
    let _guard = store.watermark_lock.lock().unwrap_or_else(|e| e.into_inner());
    store.put_sync(Cf::Revisions, &layout::watermark(tenant, name), &NumberValue::new(value))
}
