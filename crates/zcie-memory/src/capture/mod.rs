//! Client trace links and path resolution (`docs/08-memory-and-graph.md` sections 5.2 and 5.3).

use std::path::Path;

use zcie_store::values::{FileIdValue, TurnValue};
use zcie_store::{Batch, Cf, ObjectStore, Store, layout};
use zcie_types::{
    EdgeStatus, EventEnvelope, EventId, EventType, EvidenceKind, MemoryKind, MemoryRecord, Relation, Scope, WorktreeRecord,
    truncate_chars,
};

use crate::error::Result;
use crate::records::{evidence, memories_of_event};

/// Maximum number of events in one client turn.
const MAX_TURN_EVENTS: usize = 1000;

/// Most turn lookups of one session that the search for the previous user message reads.
const MAX_SESSION_TURNS: usize = 20_000;
/// Longest context text of a follow-up message.
pub const MAX_CONTEXT_CHARS: usize = 300;

/// Most neighbor messages that one search reads (shell commands typed with "!" are skipped).
const MAX_NEIGHBOR_READS: usize = 8;

/// Returns the texts (white space joined) of at most `count` user messages of the session of
/// `event`, nearest first: before it, or after it if `after` is true. The order is the event
/// sequence, not the event ID (IDs made in the same millisecond are not ordered).
pub fn session_user_messages(store: &Store, objects: &ObjectStore, event: &EventEnvelope, after: bool, count: usize) -> Result<Vec<String>> {
    let tenant = &event.scope.tenant_id;
    let prefix = layout::session_turns_prefix(tenant, &event.source.session_id);
    let user = EventType::UserMessage.name();
    let mut near: Vec<(Vec<u8>, TurnValue)> = store
        .prefix_scan::<TurnValue>(Cf::Lookup, &prefix, MAX_SESSION_TURNS)?
        .into_iter()
        .filter(|(_, value)| value.event_type == user && if after { value.seq > event.seq } else { value.seq < event.seq })
        .collect();
    near.sort_by_key(|(_, value)| if after { value.seq } else { u64::MAX - value.seq });
    let mut out = Vec::new();
    for (key, _) in near.into_iter().take(MAX_NEIGHBOR_READS) {
        let id = zcie_store::keys::last_part(&key);
        let Some(message) = zcie_store::events::get_event_by_id(store, tenant, &id)?.filter(|e| !e.forgotten) else { continue };
        let text = zcie_store::events::payload_text(objects, &message)?.unwrap_or_default();
        let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
        if !text.is_empty() && !crate::extraction::is_shell_input(&text) {
            out.push(text);
        }
        if out.len() >= count {
            break;
        }
    }
    Ok(out)
}

/// Returns the start of the user message before `event` in the same session, or `None`.
pub fn previous_user_message(store: &Store, objects: &ObjectStore, event: &EventEnvelope) -> Result<Option<String>> {
    let previous = session_user_messages(store, objects, event, false, 1)?;
    Ok(previous.into_iter().next().map(|text| truncate_chars(&text, MAX_CONTEXT_CHARS)))
}

/// Returns all worktrees of a workspace.
pub fn worktrees_of_workspace(store: &Store, tenant: &str, workspace_id: &str) -> Result<Vec<WorktreeRecord>> {
    let items = store.prefix_scan::<WorktreeRecord>(Cf::Registry, &layout::registry_prefix(tenant, "worktree"), 10_000)?;
    Ok(items.into_iter().map(|(_, w)| w).filter(|w| w.workspace_id == workspace_id).collect())
}

/// Resolves a path from a client to `(worktree, relative path)`.
///
/// An absolute path must be inside a worktree root. A relative path uses `hint_worktree` or the only worktree.
pub fn resolve_worktree_path(store: &Store, tenant: &str, workspace_id: &str, hint_worktree: Option<&str>, path: &str) -> Result<Option<(WorktreeRecord, String)>> {
    let worktrees = worktrees_of_workspace(store, tenant, workspace_id)?;
    if Path::new(path).is_absolute() {
        for worktree in worktrees {
            if let Ok(rel) = Path::new(path).strip_prefix(&worktree.root_path) {
                let rel = rel.to_string_lossy().replace('\\', "/");
                return Ok(Some((worktree, rel)));
            }
        }
        return Ok(None);
    }
    let rel = path.trim_start_matches("./").to_string();
    let chosen = match hint_worktree {
        Some(id) => worktrees.into_iter().find(|w| w.worktree_id == id),
        None if worktrees.len() == 1 => worktrees.into_iter().next(),
        None => None,
    };
    Ok(chosen.map(|w| (w, rel)))
}

/// Finds the file ref of a relative path in any worktree of the workspace.
pub fn find_file_ref(store: &Store, batch: &Batch, tenant: &str, workspace_id: &str, rel_path: &str) -> Result<Option<String>> {
    let rel = rel_path.trim_start_matches("./");
    for worktree in worktrees_of_workspace(store, tenant, workspace_id)? {
        let key = layout::path(tenant, &worktree.worktree_id, rel);
        let value = match batch.pending::<FileIdValue>(Cf::Lookup, &key) {
            Some(v) => Some(v),
            None => store.get::<FileIdValue>(Cf::Lookup, &key)?,
        };
        if let Some(value) = value {
            return Ok(Some(format!("file:{}", value.file_id)));
        }
    }
    Ok(None)
}

/// Returns the events of the same client turn as `event`, except `event`.
pub fn turn_events(store: &Store, event: &EventEnvelope) -> Result<Vec<(String, TurnValue)>> {
    let Some(turn_id) = &event.source.turn_id else { return Ok(Vec::new()) };
    let tenant = &event.scope.tenant_id;
    let prefix = layout::turn_prefix(tenant, &event.source.session_id, turn_id);
    let items = store.prefix_scan::<TurnValue>(Cf::Lookup, &prefix, MAX_TURN_EVENTS)?;
    Ok(items
        .into_iter()
        .map(|(key, value)| (zcie_store::keys::last_part(&key), value))
        .filter(|(id, _)| id != event.event_id.as_str())
        .collect())
}

/// Adds `AffectsFile` edges (ClientTrace) from the requirements and tasks of `message` to `file_ref`.
pub fn link_message_to_file(store: &Store, batch: &mut Batch, message: &EventEnvelope, file_event: &EventEnvelope, file_ref: &str) -> Result<Vec<String>> {
    let tenant = &message.scope.tenant_id;
    let mut changed = Vec::new();
    for memory in memories_of_event(store, tenant, message.event_id.as_str())? {
        if memory.kind != MemoryKind::UserRequirement {
            continue;
        }
        let mem_ref = format!("mem:{}", memory.memory_id);
        let task_refs: Vec<String> = zcie_graph::edges_to(store, tenant, &mem_ref, Some(Relation::RequestedBy))?
            .into_iter()
            .map(|edge| edge.from)
            .collect();
        let sources = [&message.event_id, &file_event.event_id];
        changed.extend(link_requirement(store, batch, &memory, &task_refs, &sources, file_ref)?);
    }
    Ok(changed)
}

/// Adds `AffectsFile` edges (ClientTrace) from one requirement and its tasks to `file_ref`.
pub fn link_requirement(store: &Store, batch: &mut Batch, requirement: &MemoryRecord, task_refs: &[String], sources: &[&EventId], file_ref: &str) -> Result<Vec<String>> {
    let mem_ref = format!("mem:{}", requirement.memory_id);
    add_affects(store, batch, &requirement.scope, &mem_ref, file_ref, sources)?;
    let mut changed = vec![mem_ref];
    for task_ref in task_refs {
        add_affects(store, batch, &requirement.scope, task_ref, file_ref, sources)?;
        changed.push(task_ref.clone());
    }
    Ok(changed)
}

fn add_affects(store: &Store, batch: &mut Batch, scope: &Scope, from: &str, file_ref: &str, sources: &[&EventId]) -> Result<()> {
    let evidence = evidence(EvidenceKind::ClientTrace, sources, Some("same session and turn"));
    let edge = zcie_graph::new_edge(scope, from, Relation::AffectsFile, file_ref, EdgeStatus::Verified, evidence);
    zcie_graph::put_edge(store, batch, edge)?;
    Ok(())
}

/// Links a processed file change to the user messages of the same turn.
pub fn link_file_change_to_turn(store: &Store, batch: &mut Batch, file_event: &EventEnvelope, file_ref: &str) -> Result<Vec<String>> {
    let mut changed = Vec::new();
    for (_, value) in turn_events(store, file_event)? {
        if value.event_type != EventType::UserMessage.name() {
            continue;
        }
        if let Some(message) = zcie_store::events::get_event(store, &file_event.scope.tenant_id, value.seq)? {
            changed.extend(link_message_to_file(store, batch, &message, file_event, file_ref)?);
        }
    }
    Ok(changed)
}
