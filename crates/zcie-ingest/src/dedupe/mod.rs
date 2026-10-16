//! Idempotency check (`docs/07-ingest-and-indexing.md` section 2, step 5).

use zcie_store::Store;
use zcie_types::api::CaptureReceipt;

use crate::error::Result;

/// Returns the stored receipt of an idempotency key with `duplicate: true`.
pub fn find_existing(store: &Store, tenant: &str, idempotency_key: &str, canonical_seq: u64) -> Result<Option<CaptureReceipt>> {
    let Some(value) = zcie_store::events::get_receipt(store, tenant, idempotency_key)? else { return Ok(None) };
    Ok(Some(CaptureReceipt {
        event_id: value.event_id,
        seq: value.seq,
        durable: true,
        indexed: false,
        duplicate: true,
        canonical_seq,
    }))
}
