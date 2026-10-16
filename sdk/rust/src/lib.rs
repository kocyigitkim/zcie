//! Rust HTTP client for the ZCIE daemon (`docs/10-api-and-gateways.md` section 6).
//!
//! The client is a thin wrapper. It contains no engine logic.

pub mod error;

use std::time::Duration;

use reqwest::blocking::{Client as HttpClient, RequestBuilder};
use serde::Serialize;
use serde::de::DeserializeOwned;
use zcie_types::MemoryRecord;
use zcie_types::api::{
    CaptureReceipt, CaptureRequest, CorrectRequest, CreateTaskRequest, ErrorBody, ForgetReceipt,
    ForgetRequest, HealthReport, JobResponse, LinkRequest, LinkResponse, RegisterRepositoryRequest,
    RegisterRepositoryResponse, TimelineResponse,
};
use zcie_types::query::{ExpandLevel, ExpandRequest, ExpandResponse, QueryRequest, QueryResponse};
use zcie_types::task::TaskRecord;

pub use error::{Error, Result};

/// Default daemon address.
pub const DEFAULT_ENDPOINT: &str = "http://127.0.0.1:47831";
/// Default request timeout in seconds.
pub const DEFAULT_TIMEOUT_SECS: u64 = 30;

/// A client for one daemon and one default workspace.
#[derive(Debug, Clone)]
pub struct Client {
    endpoint: String,
    api_key: String,
    workspace: String,
    http: HttpClient,
}

impl Client {
    /// Makes a client with the default timeout.
    pub fn new(endpoint: &str, api_key: &str, workspace: &str) -> Client {
        Client::with_timeout(endpoint, api_key, workspace, Duration::from_secs(DEFAULT_TIMEOUT_SECS))
    }

    /// Makes a client with a custom request timeout.
    pub fn with_timeout(endpoint: &str, api_key: &str, workspace: &str, timeout: Duration) -> Client {
        let http = HttpClient::builder()
            .timeout(timeout)
            .build()
            .unwrap_or_else(|_| HttpClient::new());
        Client {
            endpoint: endpoint.trim_end_matches('/').to_string(),
            api_key: api_key.to_string(),
            workspace: workspace.to_string(),
            http,
        }
    }

    /// Returns the default workspace of the client.
    pub fn workspace(&self) -> &str {
        &self.workspace
    }

    /// Captures one event. Sets the workspace if the request has none.
    pub fn capture(&self, mut request: CaptureRequest) -> Result<CaptureReceipt> {
        if request.workspace.is_empty() {
            request.workspace = self.workspace.clone();
        }
        self.post("/v1/events", &request)
    }

    /// Registers a repository folder.
    pub fn register_repository(&self, path: &str, project: Option<&str>) -> Result<RegisterRepositoryResponse> {
        let request = RegisterRepositoryRequest {
            workspace: self.workspace.clone(),
            path: path.to_string(),
            project: project.map(str::to_string),
        };
        self.post("/v1/repositories", &request)
    }

    /// Runs a query with the default mode.
    pub fn query(&self, question: &str, token_budget: u32) -> Result<QueryResponse> {
        let request = QueryRequest {
            workspace: self.workspace.clone(),
            question: question.to_string(),
            token_budget: Some(token_budget),
            ..QueryRequest::default()
        };
        self.query_request(request)
    }

    /// Runs a full query request. Sets the workspace if the request has none.
    pub fn query_request(&self, mut request: QueryRequest) -> Result<QueryResponse> {
        if request.workspace.is_empty() {
            request.workspace = self.workspace.clone();
        }
        self.post("/v1/query", &request)
    }

    /// Expands refs into evidence, source or audit content.
    pub fn expand(&self, refs: &[String], level: ExpandLevel, token_budget: Option<u32>) -> Result<ExpandResponse> {
        let request = ExpandRequest {
            workspace: self.workspace.clone(),
            refs: refs.to_vec(),
            level,
            token_budget,
            expected_revision: None,
        };
        self.post("/v1/expand", &request)
    }

    /// Reads one entity as JSON.
    pub fn get_entity(&self, entity_ref: &str) -> Result<serde_json::Value> {
        self.get(&format!("/v1/entities/{}", encode_path_part(entity_ref)))
    }

    /// Reads the timeline of one entity.
    pub fn get_timeline(&self, entity_ref: &str) -> Result<TimelineResponse> {
        self.get(&format!("/v1/timeline/{}", encode_path_part(entity_ref)))
    }

    /// Makes a task. Sets the workspace if the request has none.
    pub fn create_task(&self, mut request: CreateTaskRequest) -> Result<TaskRecord> {
        if request.workspace.is_empty() {
            request.workspace = self.workspace.clone();
        }
        self.post("/v1/tasks", &request)
    }

    /// Adds a confirmed link. Sets the workspace if the request has none.
    pub fn link(&self, mut request: LinkRequest) -> Result<LinkResponse> {
        if request.workspace.is_empty() {
            request.workspace = self.workspace.clone();
        }
        self.post("/v1/links", &request)
    }

    /// Corrects a memory record.
    pub fn correct_memory(&self, memory_id: &str, request: CorrectRequest) -> Result<MemoryRecord> {
        self.post(&format!("/v1/memory/{}/correct", encode_path_part(memory_id)), &request)
    }

    /// Forgets events and records. Sets the workspace if the request has none.
    pub fn forget(&self, mut request: ForgetRequest) -> Result<ForgetReceipt> {
        if request.workspace.is_empty() {
            request.workspace = self.workspace.clone();
        }
        self.post("/v1/forget", &request)
    }

    /// Reads the daemon health report.
    pub fn health(&self) -> Result<HealthReport> {
        self.get("/v1/health")
    }

    /// Starts a rebuild of the search index.
    pub fn rebuild_search(&self) -> Result<JobResponse> {
        self.post("/v1/admin/rebuild-search", &serde_json::json!({}))
    }

    fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        let builder = self.http.get(format!("{}{path}", self.endpoint));
        self.send(builder)
    }

    fn post<B: Serialize, T: DeserializeOwned>(&self, path: &str, body: &B) -> Result<T> {
        let builder = self.http.post(format!("{}{path}", self.endpoint)).json(body);
        self.send(builder)
    }

    fn send<T: DeserializeOwned>(&self, builder: RequestBuilder) -> Result<T> {
        let mut builder = builder;
        if !self.api_key.is_empty() {
            builder = builder.bearer_auth(&self.api_key);
        }
        let response = builder.send().map_err(|e| Error::Http(e.to_string()))?;
        let status = response.status();
        let body = response.text().map_err(|e| Error::Http(e.to_string()))?;
        if !status.is_success() {
            return Err(api_error(status.as_u16(), &body));
        }
        serde_json::from_str(&body).map_err(|e| Error::Decode(e.to_string()))
    }
}

fn api_error(status: u16, body: &str) -> Error {
    match serde_json::from_str::<ErrorBody>(body) {
        Ok(parsed) => Error::Api { status, code: parsed.error.code, message: parsed.error.message },
        Err(_) => Error::Api { status, code: "http_error".to_string(), message: body.to_string() },
    }
}

/// Percent-encodes one URL path part. Keeps unreserved characters only.
pub fn encode_path_part(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        let keep = byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~');
        if keep {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}
