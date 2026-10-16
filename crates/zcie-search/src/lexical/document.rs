//! Search documents and write operations.

use std::sync::mpsc::SyncSender;

use crate::lexical::schema::IndexName;

/// One document for the search index.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SearchDoc {
    /// Entity ref. Unique per document.
    pub entity_ref: String,
    /// Entity kind, for example `symbol`.
    pub kind: String,
    /// Tenant ID.
    pub tenant_id: String,
    /// Workspace ID.
    pub workspace_id: String,
    /// Repository ID, or empty.
    pub repository_id: String,
    /// Worktree ID, or empty.
    pub worktree_id: String,
    /// File path, or empty.
    pub path: String,
    /// Name or title.
    pub name: String,
    /// Full text.
    pub body: String,
    /// Sequence of the source state.
    pub seq: u64,
}

/// One operation for the search writer.
#[derive(Debug)]
pub enum SearchOp {
    /// Replaces the document with the same ref in the index.
    Upsert(IndexName, Box<SearchDoc>),
    /// Deletes the documents with this ref from all indexes.
    Delete(String),
    /// Deletes all documents of all indexes.
    DeleteAll,
    /// Commits now. Sends `true` on the channel after the commit.
    Flush(SyncSender<bool>),
    /// Commits and stops the writer thread.
    Stop,
}
