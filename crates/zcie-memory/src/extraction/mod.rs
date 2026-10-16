//! Tier A extraction of one event (`docs/08-memory-and-graph.md` section 3.2).

pub mod tier_a;

use zcie_store::{Batch, ObjectStore, Store};
use zcie_types::{
    CommandRunPayload, ConfirmationPayload, EdgeStatus, EventEnvelope, EventType, EvidenceKind, FileChangePayload,
    MemoryKind, MemoryRecord, Ref, Relation, TaskState, TestResultPayload,
};

use crate::capture::{find_file_ref, link_requirement, turn_events};
use crate::conflict::mark_contradiction;
use crate::error::Result;
use crate::mentions::add_mentions;
use crate::records::{evidence, memories_of_event, new_memory, put_derived_from, put_memory};
use crate::tasks::{create_task, find_by_external_ref, get_task, set_task_state};
use tier_a::{Matches, find_matches};

/// Refs that an extraction made or changed.
#[derive(Debug, Default, Clone)]
pub struct Outcome {
    /// Changed entity refs (for search updates and cache invalidation).
    pub changed_refs: Vec<String>,
}

/// Confidence of a decision record (X-07).
const DECISION_CONFIDENCE: f32 = 0.6;
/// Confidence of a preference record (X-08).
const PREFERENCE_CONFIDENCE: f32 = 0.5;

/// Runs Tier A extraction for one event and adds all records and edges to the batch.
///
/// An event that already has memory records is not processed again.
pub fn process_event(store: &Store, objects: &ObjectStore, batch: &mut Batch, event: &EventEnvelope, text: &str) -> Result<Outcome> {
    if !memories_of_event(store, &event.scope.tenant_id, event.event_id.as_str())?.is_empty() {
        return Ok(Outcome::default());
    }
    let mut outcome = Outcome::default();
    match event.event_type {
        EventType::UserMessage => process_user_message(store, objects, batch, event, text, &mut outcome)?,
        EventType::AssistantMessage => process_assistant_message(store, batch, event, text, &mut outcome)?,
        EventType::TestResult => process_test_result(store, batch, event, text, &mut outcome)?,
        EventType::CommandRun => process_command(store, batch, event, text, &mut outcome)?,
        EventType::ToolResult => add_observations(store, batch, event, &find_matches(text), &mut outcome)?,
        EventType::UserConfirmation => process_confirmation(store, batch, event, text, &mut outcome)?,
        _ => {}
    }
    Ok(outcome)
}

fn add_record(store: &Store, batch: &mut Batch, event: &EventEnvelope, kind: MemoryKind, text: &str, confidence: f32, outcome: &mut Outcome) -> Result<MemoryRecord> {
    let record = new_memory(&event.scope, kind, text, confidence, &event.event_id, &event.timestamp_utc);
    put_memory(batch, &record)?;
    put_derived_from(store, batch, &record, event)?;
    outcome.changed_refs.push(format!("mem:{}", record.memory_id));
    Ok(record)
}

fn process_user_message(store: &Store, objects: &ObjectStore, batch: &mut Batch, event: &EventEnvelope, text: &str, outcome: &mut Outcome) -> Result<()> {
    let matches = find_matches(text);
    let requirement = add_record(store, batch, event, MemoryKind::UserRequirement, text, 1.0, outcome)?;
    add_markers(store, batch, event, &matches, outcome)?;
    let req_ref = format!("mem:{}", requirement.memory_id);
    outcome.changed_refs.extend(add_mentions(store, batch, event, &req_ref, &matches)?);
    let task_refs = apply_task_rule(store, batch, event, text, &matches, &req_ref)?;
    outcome.changed_refs.extend(task_refs.iter().cloned());
    for (_, value) in turn_events(store, event)? {
        if value.event_type != EventType::FileChanged.name() {
            continue;
        }
        let Some(file_event) = zcie_store::events::get_event(store, &event.scope.tenant_id, value.seq)? else { continue };
        let Some(path) = file_event_path(store, objects, &file_event)? else { continue };
        let Some(file_ref) = find_file_ref(store, batch, &event.scope.tenant_id, &event.scope.workspace_id, &path)? else { continue };
        let sources = [&event.event_id, &file_event.event_id];
        outcome.changed_refs.extend(link_requirement(store, batch, &requirement, &task_refs, &sources, &file_ref)?);
    }
    Ok(())
}

/// Returns the relative path of a file event, resolved against the worktrees of its workspace.
pub fn file_event_path(store: &Store, objects: &ObjectStore, event: &EventEnvelope) -> Result<Option<String>> {
    let text = zcie_store::events::payload_text(objects, event)?;
    let Some(payload) = text.and_then(|t| serde_json::from_str::<FileChangePayload>(&t).ok()) else {
        return Ok(None);
    };
    let hint = event.git.as_ref().map(|g| g.worktree_id.as_str()).or(event.scope.worktree_id.as_deref());
    let resolved = crate::capture::resolve_worktree_path(store, &event.scope.tenant_id, &event.scope.workspace_id, hint, &payload.path)?;
    Ok(resolved.map(|(_, rel)| rel).or(Some(payload.path)))
}

fn add_markers(store: &Store, batch: &mut Batch, event: &EventEnvelope, matches: &Matches, outcome: &mut Outcome) -> Result<Vec<MemoryRecord>> {
    let mut out = Vec::new();
    for sentence in &matches.decisions {
        out.push(add_record(store, batch, event, MemoryKind::DesignDecision, sentence, DECISION_CONFIDENCE, outcome)?);
    }
    for sentence in &matches.preferences {
        out.push(add_record(store, batch, event, MemoryKind::Preference, sentence, PREFERENCE_CONFIDENCE, outcome)?);
    }
    Ok(out)
}

fn apply_task_rule(store: &Store, batch: &mut Batch, event: &EventEnvelope, text: &str, matches: &Matches, req_ref: &str) -> Result<Vec<String>> {
    let tenant = &event.scope.tenant_id;
    let mut task_ids = Vec::new();
    for task_id in &matches.internal_task_ids {
        if get_task(store, batch, tenant, task_id)?.is_some() {
            task_ids.push(task_id.clone());
        }
    }
    for external_ref in &matches.external_task_refs {
        let found = find_by_external_ref(store, batch, tenant, &event.scope.workspace_id, external_ref)?;
        let task = match found {
            Some(task) => task,
            None => create_task(store, batch, &event.scope, text, Some(external_ref), &event.event_id)?,
        };
        task_ids.push(task.task_id.0);
    }
    let mut refs = Vec::new();
    for task_id in task_ids {
        let task_ref = format!("task:{task_id}");
        let evidence = evidence(EvidenceKind::Derivation, &[&event.event_id], Some("explicit task ref"));
        let edge = zcie_graph::new_edge(&event.scope, &task_ref, Relation::RequestedBy, req_ref, EdgeStatus::Verified, evidence);
        zcie_graph::put_edge(store, batch, edge)?;
        if !refs.contains(&task_ref) {
            refs.push(task_ref);
        }
    }
    Ok(refs)
}

fn process_assistant_message(store: &Store, batch: &mut Batch, event: &EventEnvelope, text: &str, outcome: &mut Outcome) -> Result<()> {
    let matches = find_matches(text);
    let decisions: Vec<String> = matches.decisions.clone();
    for sentence in decisions {
        let record = add_record(store, batch, event, MemoryKind::DesignDecision, &sentence, DECISION_CONFIDENCE, outcome)?;
        let record_ref = format!("mem:{}", record.memory_id);
        outcome.changed_refs.extend(add_mentions(store, batch, event, &record_ref, &matches)?);
    }
    Ok(())
}

fn add_observations(store: &Store, batch: &mut Batch, event: &EventEnvelope, matches: &Matches, outcome: &mut Outcome) -> Result<()> {
    for line in &matches.error_lines {
        add_record(store, batch, event, MemoryKind::Observation, line, 1.0, outcome)?;
    }
    Ok(())
}

fn process_command(store: &Store, batch: &mut Batch, event: &EventEnvelope, text: &str, outcome: &mut Outcome) -> Result<()> {
    let (exit_code, output) = match serde_json::from_str::<CommandRunPayload>(text) {
        Ok(payload) => (payload.exit_code, payload.output.unwrap_or_default()),
        Err(_) => (None, text.to_string()),
    };
    if exit_code == Some(0) {
        return Ok(());
    }
    add_observations(store, batch, event, &find_matches(&output), outcome)
}

fn process_test_result(store: &Store, batch: &mut Batch, event: &EventEnvelope, text: &str, outcome: &mut Outcome) -> Result<()> {
    let payload: TestResultPayload = serde_json::from_str(text).unwrap_or(TestResultPayload {
        name: zcie_types::truncate_chars(text, 120),
        status: if text.to_lowercase().contains("fail") { "failed".into() } else { "passed".into() },
        output: None,
        refs: Vec::new(),
    });
    let summary = format!("test {} {}", payload.name, payload.status);
    add_record(store, batch, event, MemoryKind::Observation, &summary, 1.0, outcome)?;
    let failed = payload.status.eq_ignore_ascii_case("failed");
    let event_ref = format!("ev:{}", event.event_id);
    for value in &payload.refs {
        let Ok(target) = Ref::parse(value) else { continue };
        let target_ref = target.to_string();
        add_test_edge(store, batch, event, &target_ref, Relation::TestedBy, &event_ref)?;
        if failed {
            add_test_edge(store, batch, event, &target_ref, Relation::FailedIn, &event_ref)?;
        }
        if target.kind == "task" && get_task(store, batch, &event.scope.tenant_id, &target.id)?.is_some() {
            let state = if failed { TaskState::Failed } else { TaskState::Resolved };
            set_task_state(store, batch, &event.scope.tenant_id, &target.id, state, &event.event_id)?;
        }
        outcome.changed_refs.push(target_ref);
    }
    Ok(())
}

fn add_test_edge(store: &Store, batch: &mut Batch, event: &EventEnvelope, from: &str, relation: Relation, to: &str) -> Result<()> {
    let evidence = evidence(EvidenceKind::TestOutcome, &[&event.event_id], None);
    let edge = zcie_graph::new_edge(&event.scope, from, relation, to, EdgeStatus::Verified, evidence);
    zcie_graph::put_edge(store, batch, edge)?;
    Ok(())
}

fn process_confirmation(store: &Store, batch: &mut Batch, event: &EventEnvelope, text: &str, outcome: &mut Outcome) -> Result<()> {
    let Ok(payload) = serde_json::from_str::<ConfirmationPayload>(text) else { return Ok(()) };
    let Some(relation) = Relation::from_name(&payload.relation) else { return Ok(()) };
    if Ref::parse(&payload.from).is_err() || Ref::parse(&payload.to).is_err() {
        return Ok(());
    }
    if relation == Relation::Contradicts {
        mark_contradiction(store, batch, event, &payload.from, &payload.to)?;
    } else {
        let evidence = evidence(EvidenceKind::UserConfirmation, &[&event.event_id], None);
        let edge = zcie_graph::new_edge(&event.scope, &payload.from, relation, &payload.to, EdgeStatus::Verified, evidence);
        zcie_graph::put_edge(store, batch, edge)?;
    }
    outcome.changed_refs.push(payload.from);
    outcome.changed_refs.push(payload.to);
    Ok(())
}
