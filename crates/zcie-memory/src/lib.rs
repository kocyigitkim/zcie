//! Structured memory: Tier A extraction, memory records, tasks, client-trace links,
//! corrections and timelines. See `docs/08-memory-and-graph.md`.

pub mod capture;
pub mod conflict;
pub mod error;
pub mod extraction;
pub mod mentions;
pub mod provenance;
pub mod records;
pub mod retention;
pub mod tasks;
pub mod timeline;

pub use capture::{find_file_ref, link_file_change_to_turn, resolve_worktree_path, worktrees_of_workspace};
pub use conflict::{correct_memory, mark_contradiction, retract};
pub use error::{Error, Result};
pub use extraction::{Outcome, file_event_path, process_event};
pub use records::{backfill_decision_index, decisions_of_workspace, get_memory, memories_of_event};
pub use tasks::{create_task, find_by_external_ref, get_task, set_task_state};
pub use timeline::timeline;
