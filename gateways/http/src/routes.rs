//! HTTP routes (`docs/10-api-and-gateways.md` section 3).

use std::sync::Arc;

use axum::Json;
use axum::Router;
use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, Path, State};
use axum::http::HeaderMap;
use axum::routing::{get, post};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use zcie_api::{ApiError, ApiResult, Engine};
use zcie_security::AuthContext;
use zcie_types::api::{
    CaptureReceipt, CorrectRequest, ForgetReceipt, HealthReport, JobResponse, LinkResponse, ProjectInfo, ProjectList,
    RegisterRepositoryResponse, RenameProjectRequest, TimelineResponse,
};
use zcie_types::{ExpandResponse, MemoryRecord, QueryResponse, TaskRecord};

use crate::auth::{authenticate, authenticate_optional};
use crate::error::{HttpError, HttpResult};

/// Extra body bytes above the payload limit, for the JSON envelope.
const BODY_OVERHEAD: usize = 64 * 1024;

type Shared = State<Arc<Engine>>;

/// Builds the router with all routes.
pub fn router(engine: Arc<Engine>, max_payload_bytes: usize) -> Router {
    Router::new()
        .route("/v1/events", post(capture))
        .route("/v1/repositories", post(register))
        .route("/v1/query", post(query))
        .route("/v1/expand", post(expand))
        .route("/v1/entities/{entity_ref}", get(entity))
        .route("/v1/timeline/{entity_ref}", get(timeline))
        .route("/v1/tasks", post(create_task))
        .route("/v1/links", post(link))
        .route("/v1/memory/{memory_id}/correct", post(correct))
        .route("/v1/forget", post(forget))
        .route("/v1/health", get(health))
        .route("/v1/admin/rebuild-search", post(rebuild_search))
        .route("/v1/projects", get(list_projects).post(ensure_project))
        .route("/v1/projects/{project}", get(show_project).delete(delete_project))
        .route("/v1/projects/{project}/rename", post(rename_project))
        .layer(DefaultBodyLimit::max(max_payload_bytes + BODY_OVERHEAD))
        .with_state(engine)
}

fn parse<T: DeserializeOwned>(body: &Bytes) -> ApiResult<T> {
    serde_json::from_slice(body).map_err(|e| ApiError::InvalidRequest(format!("bad JSON body: {e}")))
}

/// Runs a blocking engine call on the blocking thread pool.
async fn blocking<T, F>(engine: Arc<Engine>, call: F) -> HttpResult<T>
where
    T: Send + 'static,
    F: FnOnce(&Engine) -> ApiResult<T> + Send + 'static,
{
    tokio::task::spawn_blocking(move || call(&engine))
        .await
        .map_err(|e| HttpError(ApiError::internal(e)))?
        .map_err(HttpError)
}

async fn with_auth<T, F>(engine: Arc<Engine>, headers: HeaderMap, call: F) -> HttpResult<Json<T>>
where
    T: Send + 'static,
    F: FnOnce(&Engine, &AuthContext) -> ApiResult<T> + Send + 'static,
{
    let auth = authenticate(&engine, &headers)?;
    blocking(engine, move |e| call(e, &auth)).await.map(Json)
}

async fn capture(State(engine): Shared, headers: HeaderMap, body: Bytes) -> HttpResult<Json<CaptureReceipt>> {
    let request = parse(&body)?;
    with_auth(engine, headers, move |e, a| e.capture_event(a, request)).await
}

async fn register(State(engine): Shared, headers: HeaderMap, body: Bytes) -> HttpResult<Json<RegisterRepositoryResponse>> {
    let request = parse(&body)?;
    with_auth(engine, headers, move |e, a| e.register_repository(a, request)).await
}

async fn query(State(engine): Shared, headers: HeaderMap, body: Bytes) -> HttpResult<Json<QueryResponse>> {
    let request = parse(&body)?;
    with_auth(engine, headers, move |e, a| e.query(a, request)).await
}

async fn expand(State(engine): Shared, headers: HeaderMap, body: Bytes) -> HttpResult<Json<ExpandResponse>> {
    let request = parse(&body)?;
    with_auth(engine, headers, move |e, a| e.expand(a, request)).await
}

async fn entity(State(engine): Shared, headers: HeaderMap, Path(entity_ref): Path<String>) -> HttpResult<Json<Value>> {
    with_auth(engine, headers, move |e, a| e.get_entity(a, &entity_ref)).await
}

async fn timeline(State(engine): Shared, headers: HeaderMap, Path(entity_ref): Path<String>) -> HttpResult<Json<TimelineResponse>> {
    with_auth(engine, headers, move |e, a| e.get_timeline(a, &entity_ref).map(|items| TimelineResponse { items })).await
}

async fn create_task(State(engine): Shared, headers: HeaderMap, body: Bytes) -> HttpResult<Json<TaskRecord>> {
    let request = parse(&body)?;
    with_auth(engine, headers, move |e, a| e.create_task(a, request)).await
}

async fn link(State(engine): Shared, headers: HeaderMap, body: Bytes) -> HttpResult<Json<LinkResponse>> {
    let request = parse(&body)?;
    with_auth(engine, headers, move |e, a| e.link_entities(a, request)).await
}

async fn correct(State(engine): Shared, headers: HeaderMap, Path(memory_id): Path<String>, body: Bytes) -> HttpResult<Json<MemoryRecord>> {
    let request: CorrectRequest = parse(&body)?;
    with_auth(engine, headers, move |e, a| e.correct_memory(a, &memory_id, request)).await
}

async fn forget(State(engine): Shared, headers: HeaderMap, body: Bytes) -> HttpResult<Json<ForgetReceipt>> {
    let request = parse(&body)?;
    with_auth(engine, headers, move |e, a| e.forget(a, request)).await
}

async fn health(State(engine): Shared, headers: HeaderMap) -> HttpResult<Json<HealthReport>> {
    let auth = authenticate_optional(&engine, &headers)?;
    blocking(engine, move |e| e.get_health(auth.as_ref())).await.map(Json)
}

async fn rebuild_search(State(engine): Shared, headers: HeaderMap) -> HttpResult<Json<JobResponse>> {
    with_auth(engine, headers, |e, a| e.rebuild_search(a).map(|job_id| JobResponse { job_id })).await
}

async fn list_projects(State(engine): Shared, headers: HeaderMap) -> HttpResult<Json<ProjectList>> {
    with_auth(engine, headers, |e, a| e.list_projects(a)).await
}

async fn ensure_project(State(engine): Shared, headers: HeaderMap, body: Bytes) -> HttpResult<Json<ProjectInfo>> {
    let request = parse(&body)?;
    with_auth(engine, headers, move |e, a| e.ensure_project(a, request)).await
}

async fn show_project(State(engine): Shared, headers: HeaderMap, Path(project): Path<String>) -> HttpResult<Json<ProjectInfo>> {
    with_auth(engine, headers, move |e, a| {
        let record = e.list_projects(a)?.projects.into_iter().find(|p| p.project_id == project || p.name == project);
        record.ok_or_else(|| ApiError::NotFound(format!("project {project}")))
    })
    .await
}

async fn rename_project(State(engine): Shared, headers: HeaderMap, Path(project): Path<String>, body: Bytes) -> HttpResult<Json<ProjectInfo>> {
    let request: RenameProjectRequest = parse(&body)?;
    with_auth(engine, headers, move |e, a| e.rename_project(a, &project, &request.name)).await
}

async fn delete_project(State(engine): Shared, headers: HeaderMap, Path(project): Path<String>) -> HttpResult<Json<ProjectInfo>> {
    with_auth(engine, headers, move |e, a| e.delete_project(a, &project)).await
}

/// Body of the root route, for humans.
pub fn about() -> Value {
    json!({ "name": "zcied", "api": "/v1" })
}
