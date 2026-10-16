//! Write operations: capture, repository registration, tasks, links and corrections.

use std::path::{Path, PathBuf};

use zcie_ingest::{memory_op, task_op};
use zcie_security::{AuthContext, Permission};
use zcie_store::{Batch, Cf, layout};
use zcie_types::api::{
    CaptureReceipt, CaptureRequest, CorrectRequest, CreateTaskRequest, LinkRequest, LinkResponse,
    RegisterRepositoryRequest, RegisterRepositoryResponse,
};
use zcie_types::ids::{new_id, prefix};
use zcie_types::{
    EdgeStatus, EventEnvelope, Evidence, EvidenceKind, JobKind, JobPriority, JobRecord, MemoryRecord, ProjectRecord,
    Ref, Relation, RepositoryRecord, TaskRecord, WorktreeRecord, truncate_chars,
};

use crate::engine::Engine;
use crate::error::{ApiError, ApiResult};

impl Engine {
    /// Stores one event durably (`POST /v1/events`).
    pub fn capture_event(&self, auth: &AuthContext, request: CaptureRequest) -> ApiResult<CaptureReceipt> {
        self.require(auth, Permission::Write)?;
        let workspace = self.resolve_workspace(auth, &request.workspace, true)?;
        let mut scope = self.request_scope(auth, &workspace);
        scope.repository_id = request.repository_id.clone();
        scope.worktree_id = request.git.as_ref().map(|g| g.worktree_id.clone());
        Ok(self.ingest.capture(&scope, &request)?)
    }

    /// Registers a repository folder and starts a full scan (`POST /v1/repositories`).
    pub fn register_repository(&self, auth: &AuthContext, request: RegisterRepositoryRequest) -> ApiResult<RegisterRepositoryResponse> {
        self.require(auth, Permission::Write)?;
        let workspace = self.resolve_workspace(auth, &request.workspace, true)?;
        let root = resolve_root(&request.path)?;
        let git_available = zcie_git::is_git_repository(&root);
        let root = if git_available { zcie_git::toplevel(&root).unwrap_or(root) } else { root };
        let root_text = root.to_string_lossy().to_string();
        let worktree = match self.find_worktree_by_root(&workspace.workspace_id, &root_text)? {
            Some(worktree) => worktree,
            None => self.create_repository(&workspace.workspace_id, &root, git_available, request.project.as_deref())?,
        };
        let mut scope = self.request_scope(auth, &workspace);
        scope.repository_id = Some(worktree.repository_id.clone());
        scope.worktree_id = Some(worktree.worktree_id.clone());
        let job = JobRecord::new(JobKind::ScanRepository, scope, JobPriority::Background, self.ingest.event_seq(), worktree.worktree_id.clone());
        let job_id = self.ingest.enqueue_job(job)?;
        if let Some(hook) = self.worktree_hook.get() {
            hook(&worktree);
        }
        Ok(RegisterRepositoryResponse {
            repository_id: worktree.repository_id,
            worktree_id: worktree.worktree_id,
            scan_job_id: job_id.0,
            git_available,
        })
    }

    fn find_worktree_by_root(&self, workspace_id: &str, root: &str) -> ApiResult<Option<WorktreeRecord>> {
        let worktrees = zcie_memory::worktrees_of_workspace(&self.store, self.tenant(), workspace_id)?;
        Ok(worktrees.into_iter().find(|w| w.root_path == root))
    }

    fn create_repository(&self, workspace_id: &str, root: &Path, has_git: bool, project: Option<&str>) -> ApiResult<WorktreeRecord> {
        let t = self.tenant().to_string();
        let mut batch = Batch::new();
        let project_id = match project {
            Some(name) => {
                let record = ProjectRecord { v: 1, project_id: new_id(prefix::PROJECT), workspace_id: workspace_id.into(), name: name.into() };
                batch.put(Cf::Registry, layout::registry(&t, "project", &record.project_id), &record)?;
                Some(record.project_id)
            }
            None => None,
        };
        let name = root.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "repository".into());
        let repository = RepositoryRecord { v: 1, repository_id: new_id(prefix::REPOSITORY), workspace_id: workspace_id.into(), project_id, name, has_git };
        let worktree = WorktreeRecord {
            v: 1,
            worktree_id: new_id(prefix::WORKTREE),
            repository_id: repository.repository_id.clone(),
            workspace_id: workspace_id.into(),
            root_path: root.to_string_lossy().to_string(),
            last_scanned_head: None,
            last_scan_utc: None,
        };
        batch.put(Cf::Registry, layout::registry(&t, "repository", &repository.repository_id), &repository)?;
        batch.put(Cf::Registry, layout::registry(&t, "worktree", &worktree.worktree_id), &worktree)?;
        self.store.write_batch(batch, true)?;
        Ok(worktree)
    }

    /// Makes a task from an event (`POST /v1/tasks`).
    pub fn create_task(&self, auth: &AuthContext, request: CreateTaskRequest) -> ApiResult<TaskRecord> {
        self.require(auth, Permission::Write)?;
        let workspace = self.resolve_workspace(auth, &request.workspace, false)?;
        if request.title.trim().is_empty() {
            return Err(ApiError::InvalidRequest("title is empty".into()));
        }
        let event = self.workspace_event(&request.source_event_id, &workspace.workspace_id)?;
        let scope = self.request_scope(auth, &workspace);
        let mut batch = Batch::new();
        let task = zcie_memory::create_task(&self.store, &mut batch, &scope, &request.title, request.external_ref.as_deref(), &event.event_id)?;
        self.store.write_batch(batch, true)?;
        let _ = self.ingest.search.send(task_op(&task));
        Ok(task)
    }

    pub(crate) fn workspace_event(&self, event_id: &str, workspace_id: &str) -> ApiResult<EventEnvelope> {
        let event = zcie_store::events::get_event_by_id(&self.store, self.tenant(), event_id)?
            .filter(|e| !e.forgotten)
            .ok_or_else(|| ApiError::NotFound(format!("event {event_id}")))?;
        if event.scope.workspace_id != workspace_id {
            return Err(ApiError::Forbidden("event is in another workspace".into()));
        }
        Ok(event)
    }

    /// Adds a link confirmed by the user or by a client trace (`POST /v1/links`).
    pub fn link_entities(&self, auth: &AuthContext, request: LinkRequest) -> ApiResult<LinkResponse> {
        self.require(auth, Permission::Write)?;
        let workspace = self.resolve_workspace(auth, &request.workspace, false)?;
        let kind = match request.evidence.as_str() {
            "user_confirmation" => EvidenceKind::UserConfirmation,
            "client_trace" => EvidenceKind::ClientTrace,
            other => return Err(ApiError::InvalidRequest(format!("evidence {other} is not permitted"))),
        };
        let event = self.workspace_event(&request.source_event_id, &workspace.workspace_id)?;
        let ctx = self.query_context(auth, &workspace, None);
        for value in [&request.from, &request.to] {
            Ref::parse(value).map_err(|e| ApiError::InvalidRequest(e.to_string()))?;
            if self.query.load_allowed(&ctx, value)?.is_none() {
                return Err(ApiError::NotFound(value.clone()));
            }
        }
        let mut batch = Batch::new();
        let edge_id = if request.relation == Relation::Contradicts {
            zcie_memory::mark_contradiction(&self.store, &mut batch, &event, &request.from, &request.to)?;
            String::new()
        } else {
            let evidence = Evidence { kind, source_event_ids: vec![event.event_id.clone()], revision: None, note: request.note.as_deref().map(|n| truncate_chars(n, 200)) };
            let edge = zcie_graph::new_edge(&event.scope, &request.from, request.relation, &request.to, EdgeStatus::Verified, evidence);
            zcie_graph::put_edge(&self.store, &mut batch, edge)?.0
        };
        self.store.write_batch(batch, true)?;
        self.cache.invalidate_workspace(&workspace.workspace_id);
        Ok(LinkResponse { edge_id })
    }

    /// Corrects a memory record. The old record is kept as `Superseded` (`POST /v1/memory/{id}/correct`).
    pub fn correct_memory(&self, auth: &AuthContext, memory_id: &str, request: CorrectRequest) -> ApiResult<MemoryRecord> {
        self.require(auth, Permission::Write)?;
        let batch_view = Batch::new();
        let old = zcie_memory::get_memory(&self.store, &batch_view, self.tenant(), memory_id)?
            .ok_or_else(|| ApiError::NotFound(format!("mem:{memory_id}")))?;
        let workspace = self.check_record_scope(auth, &old.scope, zcie_types::Classification::ProjectPrivate)?;
        let event = self.workspace_event(&request.source_event_id, &workspace.workspace_id)?;
        let mut batch = Batch::new();
        let new = zcie_memory::correct_memory(&self.store, &mut batch, self.tenant(), memory_id, &request.text, &event)?;
        let old_now = zcie_memory::get_memory(&self.store, &batch, self.tenant(), memory_id)?;
        self.store.write_batch(batch, true)?;
        let _ = self.ingest.search.send(memory_op(&new));
        if let Some(old_now) = old_now {
            let _ = self.ingest.search.send(memory_op(&old_now));
        }
        self.cache.invalidate_workspace(&workspace.workspace_id);
        Ok(new)
    }
}

fn resolve_root(path: &str) -> ApiResult<PathBuf> {
    let candidate = PathBuf::from(path);
    let absolute = candidate.canonicalize().map_err(|_| ApiError::InvalidRequest(format!("path does not exist: {path}")))?;
    if !absolute.is_dir() {
        return Err(ApiError::InvalidRequest(format!("path is not a folder: {path}")));
    }
    Ok(absolute)
}
