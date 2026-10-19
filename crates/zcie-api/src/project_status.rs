//! Index state of one project (`GET /v1/projects/{id}/status`): what is indexed and what waits.
//! The counts read key prefixes; the pending jobs and the memory records are read in pages.

use zcie_store::recovery::for_each_page;
use zcie_store::{Cf, layout};
use zcie_types::api::ProjectStatus;
use zcie_types::{JobRecord, MemoryRecord};

use crate::engine::Engine;
use crate::error::ApiResult;
use zcie_security::{AuthContext, Permission};

/// Records that one page of a scan reads.
const PAGE: usize = 1_000;

impl Engine {
    /// Returns the index state of a project.
    pub fn project_status(&self, auth: &AuthContext, id_or_name: &str) -> ApiResult<ProjectStatus> {
        self.require(auth, Permission::Read)?;
        let record = self.resolve_workspace(auth, id_or_name, false)?;
        let t = self.tenant().to_string();
        let ws = record.workspace_id.clone();
        let worktrees = zcie_memory::worktrees_of_workspace(&self.store, &t, &ws)?;
        let mut files = 0;
        let mut commits = 0;
        for worktree in &worktrees {
            files += zcie_store::files::current_paths(&self.store, &t, &worktree.worktree_id, usize::MAX)?.len();
            commits += self.store.count_prefix(Cf::Lookup, &layout::commits_prefix(&t, &worktree.repository_id))?;
        }
        let symbols = self.store.count_prefix(Cf::Lookup, &layout::symbol_names_prefix(&t, &ws))?;
        let mut pending = 0u64;
        for_each_page::<JobRecord>(&self.store, Cf::Jobs, &layout::pending_prefix(&t), PAGE, &mut |page| {
            pending += page.iter().filter(|(_, job)| job.scope.workspace_id == ws).count() as u64;
            Ok(())
        })?;
        let mut memories = 0u64;
        for_each_page::<MemoryRecord>(&self.store, Cf::Entities, &layout::entity_prefix(&t, "mem"), PAGE, &mut |page| {
            memories += page.iter().filter(|(_, m)| m.scope.workspace_id == ws).count() as u64;
            Ok(())
        })?;
        Ok(ProjectStatus {
            project: self.project_info(&ws)?,
            files: files as u64,
            symbols: symbols as u64,
            commits: commits as u64,
            memories,
            pending_jobs: pending,
            ready: pending == 0,
            indexed: worktrees.iter().any(|w| w.indexed),
        })
    }
}
