//! Job processors. Each processor returns an `Output` that the job runner commits.

pub mod commit;
pub mod file;
pub mod message;
pub mod scan;
pub mod search;

use zcie_search::SearchOp;
use zcie_store::Batch;
use zcie_types::JobRecord;

/// The result of one job handler.
#[derive(Debug, Default)]
pub struct Output {
    /// Derived records and edges.
    pub batch: Batch,
    /// Search operations to send after the write.
    pub search_ops: Vec<SearchOp>,
    /// New jobs to store in the same batch.
    pub new_jobs: Vec<JobRecord>,
    /// `(worktree_id, path)` pairs to remove from the live overlay.
    pub done_paths: Vec<(String, String)>,
    /// Changed entity refs.
    pub changed_refs: Vec<String>,
}
