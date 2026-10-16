//! `ScanRepository` handler (`docs/07-ingest-and-indexing.md` section 4.2).

use std::collections::HashSet;
use std::path::Path;

use zcie_store::values::FileIdValue;
use zcie_store::{Cf, layout};
use zcie_types::job::file_target;
use zcie_types::{FileRecord, JobKind, JobPriority, JobRecord, RepositoryRecord, WorktreeRecord, now_utc};

use crate::error::{Error, Result};
use crate::ingest::Ingest;
use crate::processors::Output;

/// Number of recent commits that a scan indexes.
pub const SCAN_COMMITS: usize = 200;
/// Maximum stored paths per worktree that a scan compares.
const MAX_PATHS: usize = 10_000_000;

impl Ingest {
    /// Lists the files of a worktree and makes `IndexFile`, `DeleteFile` and `IndexCommit` jobs.
    pub(crate) fn scan_worktree(&self, job: &JobRecord) -> Result<Output> {
        let t = self.tenant().to_string();
        let key = layout::registry(&t, "worktree", &job.target);
        let mut worktree: WorktreeRecord = self.store.get(Cf::Registry, &key)?.ok_or_else(|| Error::NotFound(job.target.clone()))?;
        let repository: Option<RepositoryRecord> = self.store.get(Cf::Registry, &layout::registry(&t, "repository", &worktree.repository_id))?;
        let has_git = repository.map(|r| r.has_git).unwrap_or(false);
        let root = Path::new(&worktree.root_path).to_path_buf();
        let files = if has_git {
            zcie_fs::files_from_list(&root, &zcie_git::list_tracked_files(&root)?, &self.config.exclude)
        } else {
            zcie_fs::discover_files(&root, &self.config.exclude)?
        };
        let mut output = Output::default();
        let current: HashSet<String> = files.iter().map(|f| f.rel_path.clone()).collect();
        for path in &current {
            self.overlay.add(&worktree.worktree_id, path);
            output.new_jobs.push(self.scan_job(job, &worktree, JobKind::IndexFile, file_target(&worktree.worktree_id, path)));
        }
        self.add_delete_jobs(job, &worktree, &current, &mut output)?;
        if has_git {
            self.add_commit_jobs(job, &worktree, &mut output)?;
            worktree.last_scanned_head = zcie_git::head_commit(&root).ok();
        }
        worktree.last_scan_utc = Some(now_utc());
        output.batch.put(Cf::Registry, key, &worktree)?;
        Ok(output)
    }

    fn scan_job(&self, job: &JobRecord, worktree: &WorktreeRecord, kind: JobKind, target: String) -> JobRecord {
        let mut scope = job.scope.clone();
        scope.repository_id = Some(worktree.repository_id.clone());
        scope.worktree_id = Some(worktree.worktree_id.clone());
        JobRecord::new(kind, scope, JobPriority::Background, job.source_seq, target)
    }

    fn add_delete_jobs(&self, job: &JobRecord, worktree: &WorktreeRecord, current: &HashSet<String>, output: &mut Output) -> Result<()> {
        let t = self.tenant().to_string();
        let prefix = layout::path_prefix(&t, &worktree.worktree_id);
        for (key, value) in self.store.prefix_scan::<FileIdValue>(Cf::Lookup, &prefix, MAX_PATHS)? {
            let path = zcie_store::keys::split_key(&key).get(3).cloned().unwrap_or_default();
            if current.contains(&path) {
                continue;
            }
            let record: Option<FileRecord> = self.store.get(Cf::Entities, &layout::entity(&t, "file", &value.file_id))?;
            if record.is_some_and(|r| !r.deleted) {
                output.new_jobs.push(self.scan_job(job, worktree, JobKind::DeleteFile, file_target(&worktree.worktree_id, &path)));
            }
        }
        Ok(())
    }

    fn add_commit_jobs(&self, job: &JobRecord, worktree: &WorktreeRecord, output: &mut Output) -> Result<()> {
        let t = self.tenant().to_string();
        let root = Path::new(&worktree.root_path);
        let shas = zcie_git::recent_commits(root, SCAN_COMMITS).unwrap_or_default();
        for sha in shas {
            if self.store.exists(Cf::Lookup, &layout::commit_sha(&t, &worktree.repository_id, &sha))? {
                continue;
            }
            output.new_jobs.push(self.scan_job(job, worktree, JobKind::IndexCommit, sha));
        }
        Ok(())
    }
}
