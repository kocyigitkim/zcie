//! An in-process engine in a temporary folder.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use tempfile::TempDir;
use zcie_api::Engine;
use zcie_core::{EngineConfig, InstancePaths, RunningEngine, start_engine};
use zcie_security::{AuthContext, Permission};
use zcie_types::api::{CaptureReceipt, CaptureRequest, RegisterRepositoryRequest, RegisterRepositoryResponse};
use zcie_types::event::EVENT_SCHEMA;
use zcie_types::{EventSource, QueryMode, QueryRequest, QueryResponse};

/// Default workspace name of the tests.
pub const WORKSPACE: &str = "atheron";
/// Maximum wait for background work.
const IDLE_TIMEOUT: Duration = Duration::from_secs(30);

/// Returns an admin auth context with access to all workspaces.
pub fn admin() -> AuthContext {
    AuthContext::local_admin()
}

/// Returns an auth context of `principal` with read and write access to `workspaces`.
pub fn workspace_auth(principal: &str, workspaces: &[&str]) -> AuthContext {
    AuthContext {
        tenant_id: "local".into(),
        principal_id: principal.into(),
        client_name: principal.into(),
        workspaces: workspaces.iter().map(|w| w.to_string()).collect(),
        permissions: vec![Permission::Read, Permission::Write],
        policy_version: 0,
    }
}

/// Returns a read-only auth context for `workspaces`.
pub fn read_only_auth(principal: &str, workspaces: &[&str]) -> AuthContext {
    AuthContext { permissions: vec![Permission::Read], ..workspace_auth(principal, workspaces) }
}

/// A running engine in a temporary home folder. The folder is deleted on drop.
pub struct TestEngine {
    /// The temporary folder. It contains `home/` and fixture repositories.
    pub dir: TempDir,
    running: Option<RunningEngine>,
}

impl TestEngine {
    /// Starts an engine with test settings (2 workers, 50 ms debounce).
    pub fn start() -> TestEngine {
        Self::start_with(|_| {})
    }

    /// Starts an engine. `change` can edit the configuration first.
    pub fn start_with(change: impl FnOnce(&mut EngineConfig)) -> TestEngine {
        let dir = tempfile::tempdir().expect("temp dir");
        let mut config = test_config();
        change(&mut config);
        let running = start_in(dir.path(), config);
        TestEngine { dir, running: Some(running) }
    }

    /// Stops the engine cleanly and starts it again on the same folder.
    pub fn restart(&mut self) {
        if let Some(running) = self.running.take() {
            running.shutdown();
        }
        self.running = Some(start_in(self.dir.path(), test_config()));
    }

    /// Returns the engine.
    pub fn engine(&self) -> &Arc<Engine> {
        &self.running.as_ref().expect("engine runs").engine
    }

    /// Returns the home folder.
    pub fn home(&self) -> PathBuf {
        self.dir.path().join("home")
    }

    /// Captures one event as the admin. `payload` is the payload text.
    pub fn capture(&self, event_type: &str, session: &str, turn: Option<&str>, payload: &str) -> CaptureReceipt {
        let request = capture_request(event_type, session, turn, payload);
        self.engine().capture_event(&admin(), request).expect("capture")
    }

    /// Registers a repository folder in the test workspace.
    pub fn register(&self, path: &Path) -> RegisterRepositoryResponse {
        let request = RegisterRepositoryRequest { workspace: WORKSPACE.into(), path: path.to_string_lossy().into(), project: None };
        self.engine().register_repository(&admin(), request).expect("register")
    }

    /// Runs a query in the test workspace as the admin.
    pub fn query(&self, question: &str, budget: u32) -> QueryResponse {
        let request = QueryRequest {
            workspace: WORKSPACE.into(),
            question: question.into(),
            mode: QueryMode::Auto,
            token_budget: Some(budget),
            ..QueryRequest::default()
        };
        self.engine().query(&admin(), request).expect("query")
    }

    /// Waits until no job is pending and all watermarks reach the canonical watermark.
    pub fn wait_idle(&self) {
        let engine = self.engine();
        let started = Instant::now();
        while started.elapsed() < IDLE_TIMEOUT {
            let pending = zcie_store::recovery::pending_jobs(&engine.store, "local").map(|j| j.len()).unwrap_or(1);
            let wm = zcie_store::watermarks::read_watermarks(&engine.store, "local").expect("watermarks");
            let canonical = engine.ingest.event_seq();
            let search_idle = engine.ingest.search.pending() == 0;
            if pending == 0 && search_idle && wm.graph_seq >= canonical && wm.search_seq >= canonical && engine.ingest.overlay.is_empty() {
                std::thread::sleep(Duration::from_millis(50));
                return;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        panic!("engine did not become idle within {IDLE_TIMEOUT:?}");
    }
}

impl Drop for TestEngine {
    fn drop(&mut self) {
        if let Some(running) = self.running.take() {
            running.shutdown();
        }
    }
}

/// Returns the test configuration.
pub fn test_config() -> EngineConfig {
    let mut config = EngineConfig::default();
    config.ingest.worker_count = 2;
    config.ingest.debounce_ms = 50;
    config
}

fn start_in(dir: &Path, config: EngineConfig) -> RunningEngine {
    let home = dir.join("home");
    let paths = InstancePaths { data: home.join("instances").join("local"), home };
    start_engine(paths, config).expect("engine starts")
}

/// Makes a capture request in the test workspace.
pub fn capture_request(event_type: &str, session: &str, turn: Option<&str>, payload: &str) -> CaptureRequest {
    CaptureRequest {
        schema: EVENT_SCHEMA.into(),
        idempotency_key: format!("test:{session}:{}:{}", turn.unwrap_or("-"), ulid_like()),
        timestamp_utc: None,
        workspace: WORKSPACE.into(),
        repository_id: None,
        source: EventSource { app: "test".into(), session_id: session.into(), turn_id: turn.map(str::to_string) },
        event_type: event_type.into(),
        payload: Some(payload.into()),
        git: None,
        classification: None,
    }
}

fn ulid_like() -> String {
    zcie_types::ids::new_id("k_")
}
