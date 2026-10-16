//! `IndexCommit` handler (`docs/07-ingest-and-indexing.md` section 7.1).

use std::path::Path;

use zcie_store::values::{CommitIdValue, FileIdValue};
use zcie_store::{Batch, Cf, layout};
use zcie_types::{CommitId, CommitRecord, EdgeStatus, Evidence, EvidenceKind, FileRecord, JobRecord, Relation, Scope, WorktreeRecord};

use crate::error::{Error, Result};
use crate::ingest::Ingest;
use crate::processors::Output;
use crate::processors::search::commit_op;

impl Ingest {
    /// Stores one commit, its `ChangedInCommit` edges and its trailer links.
    pub(crate) fn index_commit(&self, job: &JobRecord) -> Result<Output> {
        let mut output = Output::default();
        let worktree = self.commit_worktree(&job.scope)?;
        let root = Path::new(&worktree.root_path);
        let info = zcie_git::commit_info(root, &job.target)?;
        let t = self.tenant().to_string();
        let commit_id = CommitId::from_sha(&info.sha);
        let commit_ref = format!("commit:{commit_id}");
        let scope = Scope { worktree_id: Some(worktree.worktree_id.clone()), ..job.scope.clone() };
        for changed in &info.changed {
            self.link_changed_path(&mut output, &worktree, &scope, &changed.path, &commit_ref, &info.sha)?;
        }
        let unresolved = self.link_trailers(&mut output.batch, &scope, &info.message, &commit_ref, &info.sha)?;
        let record = CommitRecord {
            v: 1,
            commit_id: commit_id.clone(),
            scope,
            sha: info.sha.clone(),
            author: info.author.clone(),
            author_utc: info.author_utc.clone(),
            message: info.message.clone(),
            changed_paths: info.changed.iter().map(|c| format!("{} {}", c.status, c.path)).collect(),
            trailers: info.trailers.clone(),
            unresolved_trailers: unresolved,
        };
        output.batch.put(Cf::Entities, layout::entity(&t, "commit", commit_id.as_str()), &record)?;
        let lookup = CommitIdValue { v: 1, commit_id: commit_id.0.clone() };
        output.batch.put(Cf::Lookup, layout::commit_sha(&t, &worktree.repository_id, &info.sha), &lookup)?;
        output.search_ops.push(commit_op(&record));
        output.changed_refs.push(commit_ref);
        Ok(output)
    }

    fn commit_worktree(&self, scope: &Scope) -> Result<WorktreeRecord> {
        let worktrees = zcie_memory::worktrees_of_workspace(&self.store, self.tenant(), &scope.workspace_id)?;
        worktrees
            .into_iter()
            .find(|w| Some(&w.worktree_id) == scope.worktree_id.as_ref() || Some(&w.repository_id) == scope.repository_id.as_ref())
            .ok_or_else(|| Error::NotFound("worktree of commit".into()))
    }

    fn link_changed_path(&self, output: &mut Output, worktree: &WorktreeRecord, scope: &Scope, path: &str, commit_ref: &str, sha: &str) -> Result<()> {
        let t = self.tenant().to_string();
        let key = layout::path(&t, &worktree.worktree_id, path);
        let Some(found) = self.store.get::<FileIdValue>(Cf::Lookup, &key)? else { return Ok(()) };
        let file_ref = format!("file:{}", found.file_id);
        let evidence = Evidence { kind: EvidenceKind::GitHistory, source_event_ids: Vec::new(), revision: Some(sha.into()), note: Some("git diff-tree".into()) };
        let edge = zcie_graph::new_edge(scope, &file_ref, Relation::ChangedInCommit, commit_ref, EdgeStatus::Verified, evidence);
        zcie_graph::put_edge(&self.store, &mut output.batch, edge)?;
        let entity_key = layout::entity(&t, "file", &found.file_id);
        if let Some(mut file) = self.store.get::<FileRecord>(Cf::Entities, &entity_key)?
            && file.git_revision.is_none()
        {
            file.git_revision = Some(sha.to_string());
            output.batch.put(Cf::Entities, entity_key, &file)?;
        }
        output.changed_refs.push(file_ref);
        Ok(())
    }

    /// Adds `task -Implements-> commit` edges for `ZCIE-Task` trailers. Returns unresolved values.
    fn link_trailers(&self, batch: &mut Batch, scope: &Scope, message: &str, commit_ref: &str, sha: &str) -> Result<Vec<String>> {
        let t = self.tenant().to_string();
        let mut unresolved = Vec::new();
        for value in zcie_git::task_trailers(message) {
            let task = match value.strip_prefix("task:") {
                Some(id) => zcie_memory::get_task(&self.store, batch, &t, id)?,
                None => zcie_memory::find_by_external_ref(&self.store, batch, &t, &scope.workspace_id, &value)?,
            };
            let Some(task) = task else {
                unresolved.push(value);
                continue;
            };
            let evidence = Evidence { kind: EvidenceKind::CommitTrailer, source_event_ids: Vec::new(), revision: Some(sha.into()), note: Some(format!("ZCIE-Task: {value}")) };
            let task_ref = format!("task:{}", task.task_id);
            let edge = zcie_graph::new_edge(scope, &task_ref, Relation::Implements, commit_ref, EdgeStatus::Verified, evidence);
            zcie_graph::put_edge(&self.store, batch, edge)?;
        }
        Ok(unresolved)
    }
}
