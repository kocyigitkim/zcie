//! Read operations: query, expand, entity, timeline, health.

use serde_json::{Value, json};
use zcie_security::{AuthContext, Permission};
use zcie_store::recovery::dead_letter_count;
use zcie_store::watermarks::read_watermarks;
use zcie_store::{Cf, layout};
use zcie_types::api::{HealthReport, QueueReport, TimelineItem};
use zcie_types::{
    Classification, CommitRecord, ExpandRequest, ExpandResponse, FileRecord, MemoryRecord, QueryRequest, QueryResponse,
    Ref, Scope, SymbolRecord, TaskRecord, truncate_chars,
};

use crate::engine::Engine;
use crate::error::{ApiError, ApiResult};

impl Engine {
    /// Answers a query (`POST /v1/query`). A global or cross-project query reads each project
    /// on its own and merges the results (ADR-009).
    pub fn query(&self, auth: &AuthContext, request: QueryRequest) -> ApiResult<QueryResponse> {
        self.require(auth, Permission::Read)?;
        zcie_telemetry::Counters::inc(&self.ingest.counters.queries);
        let start = std::time::Instant::now();
        let hits = zcie_telemetry::Counters::get(&self.ingest.counters.cache_hits);
        let targets = self.query_targets(auth, &request)?;
        let response = match targets.as_slice() {
            [one] => {
                let ctx = self.query_context(auth, one, request.repository_id.clone());
                self.query.run(&ctx, &request)?
            }
            _ => crate::multi::query_many(self, auth, &request, &targets)?,
        };
        let cached = zcie_telemetry::Counters::get(&self.ingest.counters.cache_hits) > hits;
        let workspaces: Vec<&str> = targets.iter().map(|t| t.workspace_id.as_str()).collect();
        self.ingest.counters.record(json!({
            "op": "query", "ws": workspaces.first().copied().unwrap_or_default(), "projects": workspaces.len(),
            "client": auth.client_name, "ms": ms(start), "question": truncate_chars(&request.question, 160),
            "class": response.class, "results": response.results.len(), "tokens": response.token_estimate,
            "budget": response.token_budget, "truncated": response.truncated, "cached": cached,
        }));
        Ok(response)
    }

    /// Expands refs (`POST /v1/expand`).
    pub fn expand(&self, auth: &AuthContext, request: ExpandRequest) -> ApiResult<ExpandResponse> {
        self.require(auth, Permission::Read)?;
        let start = std::time::Instant::now();
        let workspace = self.resolve_workspace(auth, &request.workspace, false)?;
        let ctx = self.query_context(auth, &workspace, None);
        let response = self.query.expand(&ctx, &request)?;
        self.ingest.counters.record(json!({
            "op": "expand", "ws": workspace.workspace_id, "client": auth.client_name, "ms": ms(start),
            "refs": request.refs.len(), "items": response.items.len(), "tokens": response.token_estimate,
            "truncated": response.truncated, "first_ref": request.refs.first(),
        }));
        Ok(response)
    }

    /// Returns the stored record of a ref as JSON (`GET /v1/entities/{ref}`).
    pub fn get_entity(&self, auth: &AuthContext, entity_ref: &str) -> ApiResult<Value> {
        self.require(auth, Permission::Read)?;
        let parsed = Ref::parse(entity_ref).map_err(|e| ApiError::InvalidRequest(e.to_string()))?;
        let t = self.tenant().to_string();
        if self.store.exists(Cf::Policies, &layout::tombstone(&t, entity_ref))? {
            return Err(ApiError::NotFound(entity_ref.into()));
        }
        let key = layout::entity(&t, &parsed.kind, &parsed.id);
        let (value, scope, classification) = match parsed.kind.as_str() {
            "ev" => return self.get_event_json(auth, &parsed.id),
            "file" => record_json(self.store.get::<FileRecord>(Cf::Entities, &key)?, |r| r.scope.clone())?,
            "sym" => record_json(self.store.get::<SymbolRecord>(Cf::Entities, &key)?, |r| r.scope.clone())?,
            "mem" => record_json(self.store.get::<MemoryRecord>(Cf::Entities, &key)?, |r| r.scope.clone())?,
            "task" => record_json(self.store.get::<TaskRecord>(Cf::Entities, &key)?, |r| r.scope.clone())?,
            "commit" => record_json(self.store.get::<CommitRecord>(Cf::Entities, &key)?, |r| r.scope.clone())?,
            _ => return Err(ApiError::NotFound(entity_ref.into())),
        }
        .ok_or_else(|| ApiError::NotFound(entity_ref.into()))?;
        self.check_record_scope(auth, &scope, classification)?;
        Ok(value)
    }

    fn get_event_json(&self, auth: &AuthContext, event_id: &str) -> ApiResult<Value> {
        let event = zcie_store::events::get_event_by_id(&self.store, self.tenant(), event_id)?
            .filter(|e| !e.forgotten)
            .ok_or_else(|| ApiError::NotFound(format!("ev:{event_id}")))?;
        self.check_record_scope(auth, &event.scope, event.classification)?;
        let payload = zcie_store::events::payload_text(&self.objects, &event)?;
        Ok(json!({ "event": event, "payload": payload }))
    }

    /// Returns the timeline of a ref (`GET /v1/timeline/{ref}`).
    pub fn get_timeline(&self, auth: &AuthContext, entity_ref: &str) -> ApiResult<Vec<TimelineItem>> {
        self.require(auth, Permission::Read)?;
        let entity = self.get_entity(auth, entity_ref)?;
        let scope_value = entity.get("scope").or_else(|| entity.get("event").and_then(|e| e.get("scope")));
        let scope: Scope = scope_value
            .and_then(|s| serde_json::from_value(s.clone()).ok())
            .ok_or_else(|| ApiError::NotFound(entity_ref.into()))?;
        let fanout = self.query_fanout();
        Ok(zcie_memory::timeline(&self.store, &self.objects, self.tenant(), &scope.workspace_id, entity_ref, fanout)?)
    }

    fn query_fanout(&self) -> usize {
        self.query.fanout()
    }

    /// Returns daemon health. Without auth, only liveness fields are filled (`GET /v1/health`).
    pub fn get_health(&self, auth: Option<&AuthContext>) -> ApiResult<HealthReport> {
        let mut report = HealthReport {
            status: "ok".into(),
            version: self.config.version.clone(),
            watermarks: None,
            queue: None,
            storage_bytes: None,
            uptime_s: self.started.elapsed().as_secs(),
        };
        if auth.is_some_and(|a| a.can(Permission::Read)) {
            let sizes = self.ingest.queue.sizes();
            report.watermarks = Some(read_watermarks(&self.store, self.tenant())?);
            report.queue = Some(QueueReport {
                interactive: sizes.interactive as u64,
                background: sizes.background as u64,
                dead_letter: dead_letter_count(&self.store, self.tenant())?,
            });
            report.storage_bytes = Some(self.store.estimated_size() + self.objects.total_bytes());
        }
        Ok(report)
    }

    /// Starts a full search index rebuild (`POST /v1/admin/rebuild-search`).
    pub fn rebuild_search(&self, auth: &AuthContext) -> ApiResult<String> {
        self.require(auth, Permission::Admin)?;
        let scope = Scope { tenant_id: self.tenant().to_string(), principal_id: auth.principal_id.clone(), ..Scope::default() };
        let job = zcie_types::JobRecord::new(zcie_types::JobKind::UpdateSearch, scope, zcie_types::JobPriority::Interactive, self.ingest.event_seq(), "all".into());
        Ok(self.ingest.enqueue_job(job)?.0)
    }
}

/// Milliseconds since `start`, with 0.1 ms precision.
pub(crate) fn ms(start: std::time::Instant) -> f64 {
    (start.elapsed().as_secs_f64() * 10_000.0).round() / 10.0
}

type RecordParts = Option<(Value, Scope, Classification)>;

fn record_json<T: serde::Serialize>(record: Option<T>, scope: impl Fn(&T) -> Scope) -> ApiResult<RecordParts> {
    match record {
        Some(record) => {
            let value = serde_json::to_value(&record).map_err(ApiError::internal)?;
            Ok(Some((value, scope(&record), Classification::ProjectPrivate)))
        }
        None => Ok(None),
    }
}
