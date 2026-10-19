//! Capture path (`docs/07-ingest-and-indexing.md` section 2, `docs/06-storage.md` section 6).

use serde_json::Value;
use zcie_store::codec::NumberValue;
use zcie_store::events::{BlobValue, SeqValue};
use zcie_store::values::{Marker, TurnValue};
use zcie_store::watermarks::{CANONICAL, put_watermark};
use zcie_store::{Batch, Cf, layout};
use zcie_types::api::{CaptureReceipt, CaptureRequest};
use zcie_types::event::EVENT_SCHEMA;
use zcie_types::{
    CaptureReceiptValue, EventEnvelope, EventId, EventType, FileChangePayload, JobKind, JobPriority, JobRecord, Scope,
    now_utc,
};

use crate::dedupe::find_existing;
use crate::error::{Error, Result};
use crate::ingest::Ingest;

/// Maximum length of an idempotency key.
const MAX_IDEMPOTENCY_KEY: usize = 256;
/// Marker for truncated output.
const TRUNCATED_MARKER: &str = "[truncated]";

impl Ingest {
    /// Stores one event durably and only once. Returns after the sync write.
    pub fn capture(&self, scope: &Scope, request: &CaptureRequest) -> Result<CaptureReceipt> {
        let event_type = validate(request, self.config.max_payload_bytes)?;
        if event_type == EventType::AssistantMessage && !self.config.assistant_messages {
            return Err(Error::Forbidden("assistant messages are not captured (capture.assistant_messages)".into()));
        }
        if let Some(receipt) = find_existing(&self.store, self.tenant(), &idem_key(scope, &request.idempotency_key), self.event_seq())? {
            return Ok(receipt);
        }
        let (payload, redactions) = self.prepare_payload(event_type, request.payload.as_deref());
        let payload_ref = match &payload {
            Some(text) => Some(self.objects.put(text.as_bytes())?),
            None => None,
        };
        let mut seq_guard = self.lock_seq();
        if let Some(receipt) = find_existing(&self.store, self.tenant(), &idem_key(scope, &request.idempotency_key), *seq_guard)? {
            return Ok(receipt);
        }
        let mut envelope = new_envelope(scope, request, event_type, *seq_guard + 1);
        envelope.payload_size = payload.as_ref().map(|p| p.len() as u64).unwrap_or(0);
        envelope.payload_ref = payload_ref;
        envelope.redactions = redactions;
        let job = job_for(&envelope);
        let batch = self.capture_batch(&envelope, job.as_ref())?;
        self.store.write_batch(batch, self.config.sync_writes)?;
        *seq_guard = envelope.seq;
        drop(seq_guard);
        zcie_telemetry::Counters::inc(&self.counters.events_captured);
        self.after_capture(&envelope, payload.as_deref(), job);
        let seq = envelope.seq;
        Ok(CaptureReceipt { event_id: envelope.event_id.0, seq, durable: true, indexed: false, duplicate: false, canonical_seq: seq })
    }

    fn capture_batch(&self, event: &EventEnvelope, job: Option<&JobRecord>) -> Result<Batch> {
        let t = self.tenant();
        let id = event.event_id.as_str();
        let mut batch = Batch::new();
        batch.put(Cf::Events, layout::event(t, event.seq), event)?;
        let receipt = CaptureReceiptValue { v: 1, event_id: id.to_string(), seq: event.seq };
        batch.put(Cf::Lookup, layout::idem(t, &idem_key(&event.scope, &event.idempotency_key)), &receipt)?;
        batch.put(Cf::Lookup, layout::event_id(t, id), &SeqValue { v: 1, seq: event.seq })?;
        batch.put(Cf::Registry, layout::event_seq_counter(t), &NumberValue::new(event.seq))?;
        put_watermark(&mut batch, t, CANONICAL, event.seq)?;
        if let Some(hash) = &event.payload_ref {
            let old: Option<BlobValue> = self.store.get(Cf::Lookup, &layout::blob(t, hash))?;
            let value = BlobValue { v: 1, size: event.payload_size, ref_count: old.map(|b| b.ref_count).unwrap_or(0) + 1 };
            batch.put(Cf::Lookup, layout::blob(t, hash), &value)?;
        }
        if let Some(turn_id) = &event.source.turn_id {
            let value = TurnValue { v: 1, event_type: event.event_type.name().to_string(), seq: event.seq };
            batch.put(Cf::Lookup, layout::turn(t, &event.source.session_id, turn_id, id), &value)?;
        }
        batch.put(Cf::Lookup, layout::session_event(t, &event.source.session_id, id), &Marker::new())?;
        if let Some(job) = job {
            batch.put(Cf::Jobs, layout::pending_job(t, job.source_seq, job.job_id.as_str()), job)?;
        }
        Ok(batch)
    }

    fn after_capture(&self, event: &EventEnvelope, payload: Option<&str>, job: Option<JobRecord>) {
        if matches!(event.event_type, EventType::FileChanged | EventType::FileDeleted)
            && let Some(path) = payload.and_then(|p| serde_json::from_str::<FileChangePayload>(p).ok())
        {
            self.add_overlay_for(&event.scope, event.git.as_ref().map(|g| g.worktree_id.as_str()), &path.path);
        }
        if let Some(job) = job
            && self.queue.push(job).is_err()
        {
            tracing::warn!("job queue is full; the job stays in the jobs column family");
        }
    }

    fn add_overlay_for(&self, scope: &Scope, hint: Option<&str>, path: &str) {
        let hint = hint.or(scope.worktree_id.as_deref());
        if let Ok(Some((worktree, rel))) = zcie_memory::resolve_worktree_path(&self.store, self.tenant(), &scope.workspace_id, hint, path) {
            self.overlay.add(&worktree.worktree_id, &rel);
        }
    }

    /// Truncates command and tool output and redacts secrets. Returns the payload and the redaction count.
    fn prepare_payload(&self, event_type: EventType, payload: Option<&str>) -> (Option<String>, u32) {
        let Some(text) = payload else { return (None, 0) };
        let mut text = text.to_string();
        if matches!(event_type, EventType::CommandRun | EventType::ToolResult | EventType::TestResult) {
            text = truncate_output(&text, self.config.command_output_max_bytes);
        }
        if self.config.redact_secrets {
            let (redacted, count) = zcie_security::redact(&text);
            return (Some(redacted), count);
        }
        (Some(text), 0)
    }
}

fn new_envelope(scope: &Scope, request: &CaptureRequest, event_type: EventType, seq: u64) -> EventEnvelope {
    EventEnvelope {
        v: 1,
        schema: EVENT_SCHEMA.to_string(),
        event_id: EventId::new(),
        seq,
        idempotency_key: request.idempotency_key.clone(),
        timestamp_utc: request.timestamp_utc.clone().unwrap_or_else(now_utc),
        scope: scope.clone(),
        source: request.source.clone(),
        event_type,
        payload_ref: None,
        payload_size: 0,
        git: request.git.clone(),
        classification: request.classification.unwrap_or_default(),
        redactions: 0,
        forgotten: false,
    }
}

fn validate(request: &CaptureRequest, max_payload: usize) -> Result<EventType> {
    if request.schema != EVENT_SCHEMA {
        return Err(Error::Invalid(format!("schema must be {EVENT_SCHEMA}")));
    }
    let key_len = request.idempotency_key.chars().count();
    if key_len == 0 || key_len > MAX_IDEMPOTENCY_KEY {
        return Err(Error::Invalid("idempotency_key must have 1 to 256 characters".into()));
    }
    if request.source.session_id.is_empty() {
        return Err(Error::Invalid("source.session_id is empty".into()));
    }
    if request.payload.as_ref().is_some_and(|p| p.len() > max_payload) {
        return Err(Error::PayloadTooLarge);
    }
    serde_json::from_value::<EventType>(Value::String(request.event_type.clone()))
        .map_err(|_| Error::Invalid(format!("unknown event type: {}", request.event_type)))
}

/// Returns the stored form of an idempotency key. Keys are unique per project (ADR-009): the same
/// client key in two projects, or in a deleted and a new project, makes two events.
fn idem_key(scope: &Scope, key: &str) -> String {
    format!("{}/{key}", scope.workspace_id)
}

/// Makes the `ExtractMemory` job of an event, if the type needs one.
fn job_for(event: &EventEnvelope) -> Option<JobRecord> {
    if !event.event_type.needs_job() {
        return None;
    }
    let priority = match event.event_type {
        EventType::UserMessage
        | EventType::FileChanged
        | EventType::FileDeleted
        | EventType::TestResult
        | EventType::UserConfirmation
        | EventType::GitCommit => JobPriority::Interactive,
        _ => JobPriority::Background,
    };
    Some(JobRecord::new(JobKind::ExtractMemory, event.scope.clone(), priority, event.seq, event.event_id.0.clone()))
}

/// Truncates a text or the `output` field of a JSON object to `max` bytes.
pub fn truncate_output(text: &str, max: usize) -> String {
    if let Ok(Value::Object(mut map)) = serde_json::from_str::<Value>(text) {
        if let Some(Value::String(output)) = map.get("output")
            && output.len() > max
        {
            let cut = format!("{} {TRUNCATED_MARKER}", cut_bytes(output, max));
            map.insert("output".into(), Value::String(cut));
            map.insert("truncated".into(), Value::Bool(true));
        }
        return Value::Object(map).to_string();
    }
    if text.len() > max { format!("{} {TRUNCATED_MARKER}", cut_bytes(text, max)) } else { text.to_string() }
}

fn cut_bytes(text: &str, max: usize) -> &str {
    let mut end = max.min(text.len());
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}
