//! Task rule (`docs/08-memory-and-graph.md` section 4) and task state.

use zcie_store::values::TaskIdValue;
use zcie_store::{Batch, Cf, Store, layout};
use zcie_types::{EventId, Scope, TaskId, TaskRecord, TaskState, TaskStateChange, now_utc, truncate_chars};

use crate::error::{Error, Result};

/// Maximum characters of a task title.
pub const MAX_TASK_TITLE: usize = 120;

/// Reads a task, from the batch first.
pub fn get_task(store: &Store, batch: &Batch, tenant: &str, task_id: &str) -> Result<Option<TaskRecord>> {
    let key = layout::entity(tenant, "task", task_id);
    if let Some(task) = batch.pending::<TaskRecord>(Cf::Entities, &key) {
        return Ok(Some(task));
    }
    Ok(store.get(Cf::Entities, &key)?)
}

/// Finds a task by its external ref in a workspace.
pub fn find_by_external_ref(store: &Store, batch: &Batch, tenant: &str, workspace_id: &str, external_ref: &str) -> Result<Option<TaskRecord>> {
    let key = layout::task_external(tenant, workspace_id, external_ref);
    let value = match batch.pending::<TaskIdValue>(Cf::Lookup, &key) {
        Some(value) => Some(value),
        None => store.get::<TaskIdValue>(Cf::Lookup, &key)?,
    };
    match value {
        Some(value) => get_task(store, batch, tenant, &value.task_id),
        None => Ok(None),
    }
}

/// Adds a task and its external-ref lookup to the batch.
pub fn put_task(batch: &mut Batch, task: &TaskRecord) -> Result<()> {
    let tenant = &task.scope.tenant_id;
    batch.put(Cf::Entities, layout::entity(tenant, "task", task.task_id.as_str()), task)?;
    if let Some(external_ref) = &task.external_ref {
        let value = TaskIdValue { v: 1, task_id: task.task_id.0.clone() };
        batch.put(Cf::Lookup, layout::task_external(tenant, &task.scope.workspace_id, external_ref), &value)?;
    }
    Ok(())
}

/// Makes a new open task. Returns the existing task if the external ref is already used.
pub fn create_task(store: &Store, batch: &mut Batch, scope: &Scope, title: &str, external_ref: Option<&str>, source: &EventId) -> Result<TaskRecord> {
    if let Some(external_ref) = external_ref
        && let Some(task) = find_by_external_ref(store, batch, &scope.tenant_id, &scope.workspace_id, external_ref)?
    {
        return Ok(task);
    }
    let task = TaskRecord {
        v: 1,
        task_id: TaskId::new(),
        scope: workspace_scope(scope),
        title: truncate_chars(title.trim(), MAX_TASK_TITLE),
        state: TaskState::Open,
        external_ref: external_ref.map(str::to_string),
        created_from: source.clone(),
        created_utc: now_utc(),
        history: Vec::new(),
        generation: 1,
    };
    put_task(batch, &task)?;
    Ok(task)
}

/// Changes the state of a task and records the change in its history.
pub fn set_task_state(store: &Store, batch: &mut Batch, tenant: &str, task_id: &str, state: TaskState, event: &EventId) -> Result<TaskRecord> {
    let mut task = get_task(store, batch, tenant, task_id)?.ok_or_else(|| Error::NotFound(format!("task:{task_id}")))?;
    if task.state != state {
        task.history.push(TaskStateChange { from: task.state, to: state, event_id: event.clone(), utc: now_utc() });
        task.state = state;
        task.generation += 1;
        put_task(batch, &task)?;
    }
    Ok(task)
}

/// Returns the workspace part of a scope. Tasks belong to a workspace, not a worktree.
pub fn workspace_scope(scope: &Scope) -> Scope {
    Scope { project_id: None, repository_id: None, worktree_id: None, ..scope.clone() }
}
