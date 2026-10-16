//! Project operations: a project is identified by the ID in `<root>/.claude/zcie_id`
//! (ADR-009). In storage a project is a workspace record whose ID is the project ID.

use zcie_security::{AuthContext, Permission};
use zcie_store::values::WorkspaceIdValue;
use zcie_store::{Batch, Cf, layout};
use zcie_types::api::{
    EnsureProjectRequest, ForgetRequest, ForgetSelector, ProjectInfo, ProjectList, RegisterRepositoryRequest,
};
use zcie_types::WorkspaceRecord;

use crate::engine::Engine;
use crate::error::{ApiError, ApiResult};

/// Maximum projects that one list returns.
const MAX_PROJECTS: usize = 10_000;
/// A time after all events, used to forget all events of a project.
const END_OF_TIME: &str = "9999-12-31T00:00:00Z";

impl Engine {
    /// Finds or makes a project, and registers its root folder (`POST /v1/projects`).
    pub fn ensure_project(&self, auth: &AuthContext, request: EnsureProjectRequest) -> ApiResult<ProjectInfo> {
        self.require(auth, Permission::Write)?;
        let name = request.name.clone().filter(|n| !n.trim().is_empty()).unwrap_or_else(|| "project".into());
        let record = match &request.project_id {
            Some(id) if zcie_types::ids::is_project_id(id) => match self.find_workspace(id)? {
                Some(record) => record,
                None => self.create_workspace(auth, &name, Some(id))?,
            },
            Some(id) => return Err(ApiError::InvalidRequest(format!("{id} is not a valid project ID"))),
            None => self.create_workspace(auth, &name, Some(&zcie_types::ids::new_id(zcie_types::ids::prefix::PROJECT)))?,
        };
        if !auth.allows_workspace(&record.name) && !auth.allows_workspace(&record.workspace_id) {
            return Err(ApiError::Forbidden(format!("project {}", record.workspace_id)));
        }
        if record.name == record.workspace_id && request.name.is_some() {
            self.rename_project(auth, &record.workspace_id, &name)?;
        }
        if let Some(root) = &request.root {
            let registration = RegisterRepositoryRequest { workspace: record.workspace_id.clone(), path: root.clone(), project: None };
            self.register_repository(auth, registration)?;
        }
        self.project_info(&record.workspace_id)
    }

    /// Returns one project with its root folders.
    pub fn project_info(&self, id: &str) -> ApiResult<ProjectInfo> {
        let record = self.find_workspace(id)?.ok_or_else(|| ApiError::NotFound(format!("project {id}")))?;
        let roots = zcie_memory::worktrees_of_workspace(&self.store, self.tenant(), &record.workspace_id)?
            .into_iter()
            .map(|w| w.root_path)
            .collect();
        Ok(ProjectInfo { project_id: record.workspace_id, name: record.name, roots, created_utc: record.created_utc })
    }

    /// Lists the projects that the API key permits (`GET /v1/projects`).
    pub fn list_projects(&self, auth: &AuthContext) -> ApiResult<ProjectList> {
        self.require(auth, Permission::Read)?;
        let mut projects = Vec::new();
        for record in self.permitted_projects(auth)? {
            projects.push(self.project_info(&record.workspace_id)?);
        }
        projects.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(ProjectList { projects })
    }

    /// Returns the project records that the API key permits.
    pub(crate) fn permitted_projects(&self, auth: &AuthContext) -> ApiResult<Vec<WorkspaceRecord>> {
        let prefix = layout::registry_prefix(self.tenant(), "workspace");
        Ok(self
            .store
            .prefix_scan::<WorkspaceRecord>(Cf::Registry, &prefix, MAX_PROJECTS)?
            .into_iter()
            .map(|(_, record)| record)
            .filter(|r| auth.allows_workspace(&r.name) || auth.allows_workspace(&r.workspace_id))
            .collect())
    }

    /// Renames a project. The ID does not change (`POST /v1/projects/{id}/rename`).
    pub fn rename_project(&self, auth: &AuthContext, id_or_name: &str, new_name: &str) -> ApiResult<ProjectInfo> {
        self.require(auth, Permission::Write)?;
        let new_name = new_name.trim();
        if new_name.is_empty() || zcie_types::ids::is_project_id(new_name) {
            return Err(ApiError::InvalidRequest("the new name is empty or looks like an ID".into()));
        }
        let mut record = self.resolve_workspace(auth, id_or_name, false)?;
        let _guard = self.ingest_write_guard();
        let t = self.tenant().to_string();
        if self.store.exists(Cf::Lookup, &layout::workspace_name(&t, new_name))? {
            return Err(ApiError::InvalidRequest(format!("another project is named {new_name}")));
        }
        let mut batch = Batch::new();
        batch.delete(Cf::Lookup, layout::workspace_name(&t, &record.name));
        record.name = new_name.to_string();
        batch.put(Cf::Registry, layout::registry(&t, "workspace", &record.workspace_id), &record)?;
        batch.put(Cf::Lookup, layout::workspace_name(&t, new_name), &WorkspaceIdValue { v: 1, workspace_id: record.workspace_id.clone() })?;
        self.store.write_batch(batch, true)?;
        drop(_guard);
        self.project_info(&record.workspace_id)
    }

    /// Forgets all events and memory of a project and removes it from the registry
    /// (`DELETE /v1/projects/{id}`). Needs `Admin`.
    pub fn delete_project(&self, auth: &AuthContext, id_or_name: &str) -> ApiResult<ProjectInfo> {
        self.require(auth, Permission::Admin)?;
        let record = self.resolve_workspace(auth, id_or_name, false)?;
        let info = self.project_info(&record.workspace_id)?;
        let selector = ForgetSelector { before_utc: Some(END_OF_TIME.into()), ..ForgetSelector::default() };
        let forget = ForgetRequest { workspace: record.workspace_id.clone(), selector, reason: "project deleted".into() };
        self.forget(auth, forget)?;
        self.remove_registry(&record)?;
        self.cache.invalidate_workspace(&record.workspace_id);
        Ok(info)
    }

    fn remove_registry(&self, record: &WorkspaceRecord) -> ApiResult<()> {
        let t = self.tenant().to_string();
        let worktrees = zcie_memory::worktrees_of_workspace(&self.store, &t, &record.workspace_id)?;
        let mut batch = Batch::new();
        for worktree in &worktrees {
            batch.delete(Cf::Registry, layout::registry(&t, "worktree", &worktree.worktree_id));
            batch.delete(Cf::Registry, layout::registry(&t, "repository", &worktree.repository_id));
        }
        batch.delete(Cf::Registry, layout::registry(&t, "workspace", &record.workspace_id));
        batch.delete(Cf::Lookup, layout::workspace_name(&t, &record.name));
        self.store.write_batch(batch, true)?;
        if let Some(hook) = self.worktree_removed_hook.get() {
            for worktree in &worktrees {
                hook(worktree);
            }
        }
        Ok(())
    }
}

