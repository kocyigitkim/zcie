//! Write path of ZCIE: durable capture, the job runner and job processors.
//! See `docs/07-ingest-and-indexing.md`.

pub mod capture;
pub mod config;
pub mod dedupe;
pub mod error;
pub mod ingest;
pub mod overlay;
pub mod processors;
pub mod queues;

pub use config::IngestConfig;
pub use error::{Error, Result};
pub use ingest::Ingest;
pub use overlay::LiveOverlay;
pub use processors::message::worktree_scope;
pub use processors::search::{commit_op, event_op, file_op, memory_op, symbol_op, task_op};
