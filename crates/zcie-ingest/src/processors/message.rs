//! `ExtractMemory` handler (`docs/08-memory-and-graph.md` section 3.2).

use zcie_store::values::FileIdValue;
use zcie_store::{Batch, Cf, layout};
use zcie_types::job::file_target;
use zcie_types::{
    EventEnvelope, EventType, FileChangePayload, FileId, GitCommitPayload, JobKind, JobPriority, JobRecord, Scope,
    WorktreeRecord,
};

use crate::error::Result;
use crate::ingest::Ingest;
use crate::processors::Output;
use crate::processors::search::event_op;

impl Ingest {
    /// Runs Tier A extraction or makes follow-up jobs for one event.
    pub(crate) fn process_extract(&self, job: &JobRecord) -> Result<Output> {
        let mut output = Output::default();
        let Some(event) = zcie_store::events::get_event_by_id(&self.store, self.tenant(), &job.target)? else {
            return Ok(output);
        };
        if event.forgotten {
            return Ok(output);
        }
        let text = zcie_store::events::payload_text(&self.objects, &event)?.unwrap_or_default();
        match event.event_type {
            EventType::FileChanged | EventType::FileDeleted => self.extract_file_event(&event, &text, &mut output)?,
            EventType::GitCommit => self.extract_commit_event(&event, &text, &mut output)?,
            _ => {
                let outcome = zcie_memory::process_event(&self.store, &self.objects, &mut output.batch, &event, &text)?;
                output.changed_refs = outcome.changed_refs;
            }
        }
        if event.event_type.is_conversation() {
            output.search_ops.push(event_op(&event, &text));
        }
        let ops = self.ops_for_refs(&output.batch, &output.changed_refs)?;
        output.search_ops.extend(ops);
        Ok(output)
    }

    fn extract_file_event(&self, event: &EventEnvelope, text: &str, output: &mut Output) -> Result<()> {
        let Ok(payload) = serde_json::from_str::<FileChangePayload>(text) else { return Ok(()) };
        let hint = event.git.as_ref().map(|g| g.worktree_id.as_str()).or(event.scope.worktree_id.as_deref());
        let resolved = zcie_memory::resolve_worktree_path(&self.store, self.tenant(), &event.scope.workspace_id, hint, &payload.path)?;
        let Some((worktree, rel)) = resolved else { return Ok(()) };
        let file_id = self.file_id_for(&mut output.batch, &worktree.worktree_id, &rel)?;
        let file_ref = format!("file:{file_id}");
        let linked = zcie_memory::link_file_change_to_turn(&self.store, &mut output.batch, event, &file_ref)?;
        output.changed_refs.extend(linked);
        let exists = std::path::Path::new(&worktree.root_path).join(&rel).exists();
        let kind = if event.event_type == EventType::FileDeleted || !exists { JobKind::DeleteFile } else { JobKind::IndexFile };
        let scope = worktree_scope(&event.scope, &worktree);
        let target = file_target(&worktree.worktree_id, &rel);
        output.new_jobs.push(JobRecord::new(kind, scope, JobPriority::Interactive, event.seq, target));
        Ok(())
    }

    fn extract_commit_event(&self, event: &EventEnvelope, text: &str, output: &mut Output) -> Result<()> {
        let Ok(payload) = serde_json::from_str::<GitCommitPayload>(text) else { return Ok(()) };
        let worktrees = zcie_memory::worktrees_of_workspace(&self.store, self.tenant(), &event.scope.workspace_id)?;
        let hint = event.git.as_ref().map(|g| g.worktree_id.clone());
        let chosen = worktrees.iter().find(|w| {
            Some(&w.worktree_id) == hint.as_ref() || Some(&w.repository_id) == event.scope.repository_id.as_ref()
        });
        let worktree = match chosen.or_else(|| worktree_with_commit(&worktrees, &payload.sha)) {
            Some(worktree) => worktree,
            None => {
                tracing::warn!(sha = %payload.sha, "no registered worktree has the commit; event kept, no IndexCommit job");
                return Ok(());
            }
        };
        let scope = worktree_scope(&event.scope, worktree);
        output.new_jobs.push(JobRecord::new(JobKind::IndexCommit, scope, JobPriority::Interactive, event.seq, payload.sha));
        Ok(())
    }

    /// Returns the file ID of a worktree path. Makes and stores a new ID if none exists.
    pub fn file_id_for(&self, batch: &mut Batch, worktree_id: &str, rel_path: &str) -> Result<FileId> {
        let key = layout::path(self.tenant(), worktree_id, rel_path);
        let found = match batch.pending::<FileIdValue>(Cf::Lookup, &key) {
            Some(value) => Some(value),
            None => self.store.get::<FileIdValue>(Cf::Lookup, &key)?,
        };
        if let Some(value) = found {
            return Ok(FileId(value.file_id));
        }
        let file_id = FileId::new();
        batch.put(Cf::Lookup, key, &FileIdValue { v: 1, file_id: file_id.0.clone() })?;
        Ok(file_id)
    }
}

/// Returns the worktree whose Git repository contains the commit.
fn worktree_with_commit<'a>(worktrees: &'a [WorktreeRecord], sha: &str) -> Option<&'a WorktreeRecord> {
    let object = format!("{sha}^{{commit}}");
    worktrees
        .iter()
        .find(|w| zcie_git::run_git(std::path::Path::new(&w.root_path), &["cat-file", "-e", &object]).is_ok())
}

/// Returns the scope of a worktree inside the workspace of `base`.
pub fn worktree_scope(base: &Scope, worktree: &WorktreeRecord) -> Scope {
    Scope {
        repository_id: Some(worktree.repository_id.clone()),
        worktree_id: Some(worktree.worktree_id.clone()),
        ..base.clone()
    }
}
