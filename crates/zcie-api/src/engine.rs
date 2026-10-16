//! The `Engine` struct: one object with all core operations (`docs/10-api-and-gateways.md` section 2).

use std::sync::{Arc, OnceLock, RwLock};
use std::time::Instant;

use zcie_cache::L0Cache;
use zcie_ingest::Ingest;
use zcie_query::{QueryContext, QueryEngine};
use zcie_security::{AuthContext, ClientsFile, Permission};
use zcie_store::values::WorkspaceIdValue;
use zcie_store::{Cf, ObjectStore, Store, layout};
use zcie_types::ids::{new_id, prefix};
use zcie_types::{Scope, WorkspaceRecord, WorktreeRecord, now_utc};

use crate::error::{ApiError, ApiResult};

/// A callback that `zcie-core` sets to start a file watcher for a new worktree.
pub type WorktreeHook = Box<dyn Fn(&WorktreeRecord) + Send + Sync>;

/// Settings of the API layer.
#[derive(Debug, Clone)]
pub struct ApiConfig {
    /// Tenant ID.
    pub tenant: String,
    /// Daemon version.
    pub version: String,
}

/// All core operations. Transport code calls only this struct.
pub struct Engine {
    /// Canonical store.
    pub store: Arc<Store>,
    /// Blob store.
    pub objects: Arc<ObjectStore>,
    /// Write path.
    pub ingest: Arc<Ingest>,
    /// Read path.
    pub query: QueryEngine,
    /// L0 cache.
    pub cache: Arc<L0Cache>,
    /// API keys. Reloaded on SIGHUP.
    pub clients: RwLock<ClientsFile>,
    /// Development admin key from the environment, if set.
    pub dev_key: Option<String>,
    /// Settings.
    pub config: ApiConfig,
    pub(crate) started: Instant,
    pub(crate) worktree_hook: OnceLock<WorktreeHook>,
}

/// The parts that `zcie-core` builds for the engine.
pub struct EngineParts {
    /// Canonical store.
    pub store: Arc<Store>,
    /// Blob store.
    pub objects: Arc<ObjectStore>,
    /// Write path.
    pub ingest: Arc<Ingest>,
    /// Read path.
    pub query: QueryEngine,
    /// L0 cache.
    pub cache: Arc<L0Cache>,
    /// API keys.
    pub clients: ClientsFile,
    /// Development admin key.
    pub dev_key: Option<String>,
    /// Settings.
    pub config: ApiConfig,
}

impl Engine {
    /// Makes the engine from its parts.
    pub fn new(parts: EngineParts) -> Engine {
        Engine {
            store: parts.store,
            objects: parts.objects,
            ingest: parts.ingest,
            query: parts.query,
            cache: parts.cache,
            clients: RwLock::new(parts.clients),
            dev_key: parts.dev_key,
            config: parts.config,
            started: Instant::now(),
            worktree_hook: OnceLock::new(),
        }
    }

    /// Sets the callback that starts a watcher for a new worktree. Only the first call has an effect.
    pub fn set_worktree_hook(&self, hook: WorktreeHook) {
        let _ = self.worktree_hook.set(hook);
    }

    /// Checks a bearer header and returns the auth context.
    pub fn authenticate(&self, header: Option<&str>) -> ApiResult<AuthContext> {
        let clients = self.clients.read().unwrap_or_else(|e| e.into_inner());
        Ok(zcie_security::authenticate(header, &clients, self.dev_key.as_deref())?)
    }

    /// Replaces the API key list.
    pub fn reload_clients(&self, clients: ClientsFile) {
        *self.clients.write().unwrap_or_else(|e| e.into_inner()) = clients;
    }

    pub(crate) fn tenant(&self) -> &str {
        &self.config.tenant
    }

    pub(crate) fn require(&self, auth: &AuthContext, permission: Permission) -> ApiResult<()> {
        if auth.can(permission) {
            Ok(())
        } else {
            Err(ApiError::Forbidden(format!("{permission:?} permission is required")))
        }
    }

    /// Resolves a workspace name or ID. With `create`, a missing name makes a new workspace.
    pub(crate) fn resolve_workspace(&self, auth: &AuthContext, name_or_id: &str, create: bool) -> ApiResult<WorkspaceRecord> {
        let name = name_or_id.trim();
        if name.is_empty() {
            return Err(ApiError::InvalidRequest("workspace is empty".into()));
        }
        if let Some(record) = self.find_workspace(name)? {
            if !auth.allows_workspace(&record.name) && !auth.allows_workspace(&record.workspace_id) {
                return Err(ApiError::Forbidden(format!("workspace {name}")));
            }
            return Ok(record);
        }
        if !create || name.starts_with(prefix::WORKSPACE) {
            return Err(ApiError::NotFound(format!("workspace {name}")));
        }
        if !auth.allows_workspace(name) {
            return Err(ApiError::Forbidden(format!("workspace {name}")));
        }
        self.create_workspace(auth, name)
    }

    fn find_workspace(&self, name_or_id: &str) -> ApiResult<Option<WorkspaceRecord>> {
        let t = self.tenant();
        let id = if name_or_id.starts_with(prefix::WORKSPACE) {
            name_or_id.to_string()
        } else {
            match self.store.get::<WorkspaceIdValue>(Cf::Lookup, &layout::workspace_name(t, name_or_id))? {
                Some(value) => value.workspace_id,
                None => return Ok(None),
            }
        };
        Ok(self.store.get(Cf::Registry, &layout::registry(t, "workspace", &id))?)
    }

    fn create_workspace(&self, auth: &AuthContext, name: &str) -> ApiResult<WorkspaceRecord> {
        let _guard = self.ingest_write_guard();
        if let Some(record) = self.find_workspace(name)? {
            return Ok(record);
        }
        let record = WorkspaceRecord {
            v: 1,
            workspace_id: new_id(prefix::WORKSPACE),
            name: name.to_string(),
            principal_id: auth.principal_id.clone(),
            created_utc: now_utc(),
        };
        let t = self.tenant();
        let mut batch = zcie_store::Batch::new();
        batch.put(Cf::Registry, layout::registry(t, "workspace", &record.workspace_id), &record)?;
        let value = WorkspaceIdValue { v: 1, workspace_id: record.workspace_id.clone() };
        batch.put(Cf::Lookup, layout::workspace_name(t, name), &value)?;
        self.store.write_batch(batch, true)?;
        Ok(record)
    }

    /// Serializes registry writes. Uses the workspace lock of this module.
    fn ingest_write_guard(&self) -> std::sync::MutexGuard<'static, ()> {
        static REGISTRY_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        REGISTRY_LOCK.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Returns the request scope of a workspace.
    pub(crate) fn request_scope(&self, auth: &AuthContext, workspace: &WorkspaceRecord) -> Scope {
        Scope { tenant_id: self.tenant().to_string(), ..Scope::workspace(&auth.principal_id, &workspace.workspace_id) }
    }

    /// Returns the query context of a workspace.
    pub(crate) fn query_context(&self, auth: &AuthContext, workspace: &WorkspaceRecord, repository_id: Option<String>) -> QueryContext {
        let mut scope = self.request_scope(auth, workspace);
        scope.repository_id = repository_id;
        QueryContext { auth: auth.clone(), scope }
    }

    /// Checks that a record scope is readable by the auth context, with no request workspace.
    pub(crate) fn check_record_scope(&self, auth: &AuthContext, scope: &Scope, classification: zcie_types::Classification) -> ApiResult<WorkspaceRecord> {
        let workspace = self
            .find_workspace(&scope.workspace_id)?
            .ok_or_else(|| ApiError::NotFound("workspace".into()))?;
        if !auth.allows_workspace(&workspace.name) && !auth.allows_workspace(&workspace.workspace_id) {
            return Err(ApiError::Forbidden("workspace".into()));
        }
        let request = self.request_scope(auth, &workspace);
        if !zcie_security::can_read(auth, &request, scope, classification) {
            return Err(ApiError::Forbidden("scope".into()));
        }
        Ok(workspace)
    }
}
