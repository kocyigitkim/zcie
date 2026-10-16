//! Key layout of each column family (`docs/06-storage.md` section 3).
//!
//! All code builds keys with these functions. `t` is the tenant ID.

use crate::keys::{key, prefix, seq_part};

/// `registry: t/"counter"/"event_seq"`.
pub fn event_seq_counter(t: &str) -> Vec<u8> {
    key(&[t, "counter", "event_seq"])
}

/// `registry: t/<kind>/<id>` for workspace, project, repository, worktree.
pub fn registry(t: &str, kind: &str, id: &str) -> Vec<u8> {
    key(&[t, kind, id])
}

/// Prefix of all registry records of one kind.
pub fn registry_prefix(t: &str, kind: &str) -> Vec<u8> {
    prefix(&[t, kind])
}

/// `events: t/seq`.
pub fn event(t: &str, seq: u64) -> Vec<u8> {
    key(&[t, &seq_part(seq)])
}

/// Prefix of all events.
pub fn events_prefix(t: &str) -> Vec<u8> {
    prefix(&[t])
}

/// `entities: t/<kind>/<id>`. Kinds: file, sym, mem, task, commit.
pub fn entity(t: &str, kind: &str, id: &str) -> Vec<u8> {
    key(&[t, kind, id])
}

/// Prefix of all entities of one kind.
pub fn entity_prefix(t: &str, kind: &str) -> Vec<u8> {
    prefix(&[t, kind])
}

/// `edges_out: t/from/RELATION/to` or `edges_in: t/to/RELATION/from`.
pub fn edge(t: &str, first: &str, relation: &str, second: &str) -> Vec<u8> {
    key(&[t, first, relation, second])
}

/// Prefix of all edges of one ref, optionally of one relation.
pub fn edge_prefix(t: &str, entity_ref: &str, relation: Option<&str>) -> Vec<u8> {
    match relation {
        Some(r) => prefix(&[t, entity_ref, r]),
        None => prefix(&[t, entity_ref]),
    }
}

/// `revisions: t/"watermark"/<name>`.
pub fn watermark(t: &str, name: &str) -> Vec<u8> {
    key(&[t, "watermark", name])
}

/// `revisions: t/"file_state"/<worktree>/<path>`.
pub fn file_state(t: &str, worktree_id: &str, path: &str) -> Vec<u8> {
    key(&[t, "file_state", worktree_id, path])
}

/// `revisions: t/"head"/<worktree>`.
pub fn worktree_head(t: &str, worktree_id: &str) -> Vec<u8> {
    key(&[t, "head", worktree_id])
}

/// `lookup: t/"idem"/<idempotency key>`.
pub fn idem(t: &str, idempotency_key: &str) -> Vec<u8> {
    key(&[t, "idem", idempotency_key])
}

/// `lookup: t/"event_id"/<event id>`.
pub fn event_id(t: &str, event_id: &str) -> Vec<u8> {
    key(&[t, "event_id", event_id])
}

/// `lookup: t/"ws_name"/<name>`.
pub fn workspace_name(t: &str, name: &str) -> Vec<u8> {
    key(&[t, "ws_name", name])
}

/// `lookup: t/"path"/<worktree>/<path>`.
pub fn path(t: &str, worktree_id: &str, path: &str) -> Vec<u8> {
    key(&[t, "path", worktree_id, path])
}

/// Prefix of all paths of one worktree.
pub fn path_prefix(t: &str, worktree_id: &str) -> Vec<u8> {
    prefix(&[t, "path", worktree_id])
}

/// `lookup: t/"sym_ident"/<worktree>/<path>/<qualified name>/<kind>`.
pub fn symbol_identity(t: &str, worktree_id: &str, path: &str, qualified_name: &str, kind: &str) -> Vec<u8> {
    key(&[t, "sym_ident", worktree_id, path, qualified_name, kind])
}

/// `lookup: t/"sym_name"/<workspace>/<lowercase name>/<symbol id>`.
pub fn symbol_name(t: &str, workspace_id: &str, lower_name: &str, symbol_id: &str) -> Vec<u8> {
    key(&[t, "sym_name", workspace_id, lower_name, symbol_id])
}

/// Prefix of all symbols with one lowercase name.
pub fn symbol_name_prefix(t: &str, workspace_id: &str, lower_name: &str) -> Vec<u8> {
    prefix(&[t, "sym_name", workspace_id, lower_name])
}

/// `lookup: t/"file_syms"/<file id>/<symbol id>`.
pub fn file_symbol(t: &str, file_id: &str, symbol_id: &str) -> Vec<u8> {
    key(&[t, "file_syms", file_id, symbol_id])
}

/// Prefix of all symbols of one file.
pub fn file_symbols_prefix(t: &str, file_id: &str) -> Vec<u8> {
    prefix(&[t, "file_syms", file_id])
}

/// `lookup: t/"task_ext"/<workspace>/<external ref>`.
pub fn task_external(t: &str, workspace_id: &str, external_ref: &str) -> Vec<u8> {
    key(&[t, "task_ext", workspace_id, external_ref])
}

/// `lookup: t/"commit_sha"/<repository>/<sha>`.
pub fn commit_sha(t: &str, repository_id: &str, sha: &str) -> Vec<u8> {
    key(&[t, "commit_sha", repository_id, sha])
}

/// `lookup: t/"blob"/<content hash>`.
pub fn blob(t: &str, hash: &str) -> Vec<u8> {
    key(&[t, "blob", hash])
}

/// `lookup: t/"turn"/<session>/<turn>/<event id>`.
pub fn turn(t: &str, session_id: &str, turn_id: &str, event_id: &str) -> Vec<u8> {
    key(&[t, "turn", session_id, turn_id, event_id])
}

/// Prefix of all events of one client turn.
pub fn turn_prefix(t: &str, session_id: &str, turn_id: &str) -> Vec<u8> {
    prefix(&[t, "turn", session_id, turn_id])
}

/// `lookup: t/"session"/<session>/<event id>`.
pub fn session_event(t: &str, session_id: &str, event_id: &str) -> Vec<u8> {
    key(&[t, "session", session_id, event_id])
}

/// Prefix of all events of one session.
pub fn session_prefix(t: &str, session_id: &str) -> Vec<u8> {
    prefix(&[t, "session", session_id])
}

/// `lookup: t/"mem_of_event"/<event id>/<memory id>`.
pub fn memory_of_event(t: &str, event_id: &str, memory_id: &str) -> Vec<u8> {
    key(&[t, "mem_of_event", event_id, memory_id])
}

/// Prefix of all memory records of one event.
pub fn memory_of_event_prefix(t: &str, event_id: &str) -> Vec<u8> {
    prefix(&[t, "mem_of_event", event_id])
}

/// `jobs: t/"pending"/seq/<job id>`.
pub fn pending_job(t: &str, seq: u64, job_id: &str) -> Vec<u8> {
    key(&[t, "pending", &seq_part(seq), job_id])
}

/// Prefix of all pending jobs.
pub fn pending_prefix(t: &str) -> Vec<u8> {
    prefix(&[t, "pending"])
}

/// `jobs: t/"dead"/<job id>`.
pub fn dead_job(t: &str, job_id: &str) -> Vec<u8> {
    key(&[t, "dead", job_id])
}

/// Prefix of all dead-letter jobs.
pub fn dead_prefix(t: &str) -> Vec<u8> {
    prefix(&[t, "dead"])
}

/// `policies: t/"tombstone"/<ref or event id>`.
pub fn tombstone(t: &str, target: &str) -> Vec<u8> {
    key(&[t, "tombstone", target])
}

/// `lookup: t/"decision"/<workspace>/<memory id>`: decision, preference, constraint and correction records.
pub fn decision(t: &str, workspace_id: &str, memory_id: &str) -> Vec<u8> {
    key(&[t, "decision", workspace_id, memory_id])
}

/// Prefix of all decision records of one workspace.
pub fn decision_prefix(t: &str, workspace_id: &str) -> Vec<u8> {
    prefix(&[t, "decision", workspace_id])
}

/// `registry: t/"migration"/<name>`: a one-time data migration that is complete.
pub fn migration(t: &str, name: &str) -> Vec<u8> {
    key(&[t, "migration", name])
}
