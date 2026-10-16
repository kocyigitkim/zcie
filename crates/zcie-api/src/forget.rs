//! Forget (`docs/11-security.md` section 7, `docs/06-storage.md` section 10).

use zcie_search::SearchOp;
use zcie_security::{AuthContext, Permission};
use zcie_store::events::BlobValue;
use zcie_store::values::TombstoneValue;
use zcie_store::{Batch, Cf, layout};
use zcie_types::api::{ForgetReceipt, ForgetRequest, ForgetSelector};
use zcie_types::scope::parse_utc_seconds;
use zcie_types::{EventEnvelope, EventId, Ref, now_utc};

use crate::engine::Engine;
use crate::error::{ApiError, ApiResult};

/// Maximum events that one forget call removes.
const MAX_FORGET_EVENTS: usize = 1_000_000;

impl Engine {
    /// Removes events and derived memory. Needs `Admin`. Returns an audit receipt.
    pub fn forget(&self, auth: &AuthContext, request: ForgetRequest) -> ApiResult<ForgetReceipt> {
        self.require(auth, Permission::Admin)?;
        let workspace = self.resolve_workspace(auth, &request.workspace, false)?;
        let ws = workspace.workspace_id.clone();
        let reason = if request.reason.is_empty() { "user request".to_string() } else { request.reason.clone() };
        let mut receipt = ForgetReceipt { utc: now_utc(), ..ForgetReceipt::default() };
        let mut batch = Batch::new();
        let mut search_deletes = Vec::new();
        let events = self.select_events(&request.selector, &ws)?;
        if let Some(value) = &request.selector.entity_ref
            && !value.starts_with("ev:")
        {
            self.forget_entity(&mut batch, value, &ws, &reason, &mut receipt, &mut search_deletes)?;
        }
        let blobs = self.forget_events(&mut batch, &events, &reason, &mut receipt, &mut search_deletes)?;
        self.store.write_batch(batch, true)?;
        self.cache.invalidate_workspace(&ws);
        for hash in blobs {
            if let Err(error) = self.objects.delete(&hash) {
                tracing::warn!(%error, "blob delete failed; the orphan sweep removes it later");
            }
        }
        receipt.pending_jobs = search_deletes.len() as u64;
        for entity_ref in search_deletes {
            let _ = self.ingest.search.send(SearchOp::Delete(entity_ref));
        }
        tracing::info!(events = receipt.events_removed, records = receipt.records_retracted, reason, "forget completed");
        Ok(receipt)
    }

    fn select_events(&self, selector: &ForgetSelector, workspace_id: &str) -> ApiResult<Vec<EventEnvelope>> {
        let t = self.tenant().to_string();
        let mut out = Vec::new();
        if let Some(value) = &selector.entity_ref
            && let Some(id) = value.strip_prefix("ev:")
        {
            out.extend(zcie_store::events::get_event_by_id(&self.store, &t, id)?);
        }
        if let Some(session) = &selector.session_id {
            for key in self.store.prefix_keys(Cf::Lookup, &layout::session_prefix(&t, session), MAX_FORGET_EVENTS)? {
                let id = zcie_store::keys::last_part(&key);
                out.extend(zcie_store::events::get_event_by_id(&self.store, &t, &id)?);
            }
        }
        if let Some(before) = &selector.before_utc {
            let limit = parse_utc_seconds(before).ok_or_else(|| ApiError::InvalidRequest("before_utc is not RFC 3339".into()))?;
            for (_, event) in self.store.prefix_scan::<EventEnvelope>(Cf::Events, &layout::events_prefix(&t), MAX_FORGET_EVENTS)? {
                if parse_utc_seconds(&event.timestamp_utc).is_some_and(|s| s < limit) {
                    out.push(event);
                }
            }
        }
        if selector.entity_ref.is_none() && selector.session_id.is_none() && selector.before_utc.is_none() {
            return Err(ApiError::InvalidRequest("selector is empty".into()));
        }
        out.retain(|e| e.scope.workspace_id == workspace_id && !e.forgotten);
        Ok(out)
    }

    fn forget_entity(&self, batch: &mut Batch, value: &str, workspace_id: &str, reason: &str, receipt: &mut ForgetReceipt, deletes: &mut Vec<String>) -> ApiResult<()> {
        let parsed = Ref::parse(value).map_err(|e| ApiError::InvalidRequest(e.to_string()))?;
        let t = self.tenant().to_string();
        let loaded = zcie_query::load_ref(&self.store, &self.objects, &t, value)?;
        let Some(loaded) = loaded.filter(|l| l.scope.workspace_id == workspace_id) else {
            return Err(ApiError::NotFound(value.into()));
        };
        let _ = loaded;
        if parsed.kind == "mem" && zcie_memory::retract(&self.store, batch, &t, &parsed.id)? {
            receipt.records_retracted += 1;
        }
        let tombstone = TombstoneValue { v: 1, forgotten_utc: now_utc(), reason: reason.to_string() };
        batch.put(Cf::Policies, layout::tombstone(&t, value), &tombstone)?;
        receipt.edges_removed += zcie_graph::delete_all_edges_of(&self.store, batch, &t, value)? as u64;
        deletes.push(value.to_string());
        Ok(())
    }

    /// Marks events forgotten, retracts their records and removes their edges. Returns blobs to delete.
    fn forget_events(&self, batch: &mut Batch, events: &[EventEnvelope], reason: &str, receipt: &mut ForgetReceipt, deletes: &mut Vec<String>) -> ApiResult<Vec<String>> {
        let t = self.tenant().to_string();
        let ids: Vec<EventId> = events.iter().map(|e| e.event_id.clone()).collect();
        let mut blobs = Vec::new();
        for event in events {
            let mut forgotten = event.clone();
            forgotten.forgotten = true;
            forgotten.payload_ref = None;
            batch.put(Cf::Events, layout::event(&t, event.seq), &forgotten)?;
            let event_ref = format!("ev:{}", event.event_id);
            let tombstone = TombstoneValue { v: 1, forgotten_utc: now_utc(), reason: reason.to_string() };
            batch.put(Cf::Policies, layout::tombstone(&t, &event_ref), &tombstone)?;
            receipt.edges_removed += zcie_graph::delete_all_edges_of(&self.store, batch, &t, &event_ref)? as u64;
            deletes.push(event_ref);
            if let Some(hash) = &event.payload_ref {
                blobs.extend(self.release_blob(batch, hash)?);
            }
            receipt.events_removed += 1;
        }
        for memory_ref in zcie_memory::retention::retract_records_of_events(&self.store, batch, &t, &ids)? {
            receipt.records_retracted += 1;
            receipt.edges_removed += zcie_graph::delete_all_edges_of(&self.store, batch, &t, &memory_ref)? as u64;
            deletes.push(memory_ref);
        }
        Ok(blobs)
    }

    /// Decrements the blob reference count. Returns the hash if no reference remains.
    fn release_blob(&self, batch: &mut Batch, hash: &str) -> ApiResult<Option<String>> {
        let t = self.tenant().to_string();
        let key = layout::blob(&t, hash);
        let current = batch.pending::<BlobValue>(Cf::Lookup, &key).or(self.store.get::<BlobValue>(Cf::Lookup, &key)?);
        let Some(mut value) = current else { return Ok(None) };
        value.ref_count = value.ref_count.saturating_sub(1);
        if value.ref_count == 0 {
            batch.delete(Cf::Lookup, key);
            return Ok(Some(hash.to_string()));
        }
        batch.put(Cf::Lookup, key, &value)?;
        Ok(None)
    }
}
