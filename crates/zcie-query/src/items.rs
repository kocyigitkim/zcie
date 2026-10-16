//! Loads entities by ref and turns them into result items.

use zcie_store::{Cf, ObjectStore, Store, layout};
use zcie_types::{
    Classification, CommitRecord, EventEnvelope, FileRecord, MemoryRecord, MemoryStatus, Ref, ResultItem, Scope,
    SymbolRecord, TaskRecord, truncate_chars,
};

use crate::error::Result;

/// Characters of a title.
const TITLE_CHARS: usize = 160;
/// Characters of an event snippet.
const EVENT_SNIPPET_CHARS: usize = 500;
/// Characters of a short revision.
const SHORT_REVISION: usize = 12;

/// One loaded entity with the data that scope checks and ranking need.
#[derive(Debug, Clone)]
pub struct Loaded {
    /// The result item.
    pub item: ResultItem,
    /// The record scope.
    pub scope: Scope,
    /// The record classification.
    pub classification: Classification,
    /// RFC 3339 time of the entity, if known.
    pub time: Option<String>,
}

fn item(entity_ref: String, kind: &str, title: String) -> ResultItem {
    ResultItem {
        entity_ref,
        kind: kind.to_string(),
        title: truncate_chars(&title, TITLE_CHARS),
        path: None,
        range: None,
        status: None,
        evidence_status: None,
        revision: None,
        score: 0.0,
        snippet: None,
    }
}

fn loaded(item: ResultItem, scope: Scope, time: Option<String>) -> Loaded {
    Loaded { item, scope, classification: Classification::ProjectPrivate, time }
}

/// Returns a short form of a hash or a SHA.
pub fn short(revision: &str) -> String {
    truncate_chars(revision.trim_start_matches("b3:"), SHORT_REVISION)
}

/// Loads one entity by ref. Returns `None` for missing, deleted, retracted or forgotten entities.
pub fn load_ref(store: &Store, objects: &ObjectStore, tenant: &str, value: &str) -> Result<Option<Loaded>> {
    let Ok(parsed) = Ref::parse(value) else { return Ok(None) };
    if store.exists(Cf::Policies, &layout::tombstone(tenant, value))? || store.exists(Cf::Policies, &layout::tombstone(tenant, &parsed.id))? {
        return Ok(None);
    }
    let key = |kind: &str| layout::entity(tenant, kind, &parsed.id);
    Ok(match parsed.kind.as_str() {
        "sym" => store.get::<SymbolRecord>(Cf::Entities, &key("sym"))?.map(|s| symbol_item(&s)),
        "file" => store.get::<FileRecord>(Cf::Entities, &key("file"))?.filter(|f| !f.deleted).map(|f| file_item(&f)),
        "mem" => store
            .get::<MemoryRecord>(Cf::Entities, &key("mem"))?
            .filter(|m| m.status != MemoryStatus::Retracted)
            .map(|m| memory_item(&m)),
        "task" => store.get::<TaskRecord>(Cf::Entities, &key("task"))?.map(|t| task_item(&t)),
        "commit" => store.get::<CommitRecord>(Cf::Entities, &key("commit"))?.map(|c| commit_item(&c)),
        "ev" => match zcie_store::events::get_event_by_id(store, tenant, &parsed.id)? {
            Some(event) if !event.forgotten => Some(event_item(objects, &event)),
            _ => None,
        },
        _ => None,
    })
}

/// Makes the item of a symbol.
pub fn symbol_item(symbol: &SymbolRecord) -> Loaded {
    let mut out = item(format!("sym:{}", symbol.symbol_id), "symbol", symbol.qualified_name.clone());
    out.path = Some(symbol.path.clone());
    out.range = Some(symbol.span);
    out.status = Some("verified".into());
    out.evidence_status = Some("verified_ast".into());
    out.revision = Some(short(&symbol.file_hash));
    out.snippet = Some(symbol.signature.clone());
    loaded(out, symbol.scope.clone(), None)
}

/// Makes the item of a file.
pub fn file_item(file: &FileRecord) -> Loaded {
    let mut out = item(format!("file:{}", file.file_id), "file", file.path.clone());
    out.path = Some(file.path.clone());
    out.revision = Some(short(file.git_revision.as_deref().unwrap_or(&file.content_hash)));
    loaded(out, file.scope.clone(), None)
}

/// Makes the item of a memory record.
pub fn memory_item(record: &MemoryRecord) -> Loaded {
    let mut out = item(format!("mem:{}", record.memory_id), "memory", record.text.clone());
    if record.text.chars().count() > TITLE_CHARS {
        out.snippet = Some(record.text.clone());
    }
    out.status = Some(record.status.name().into());
    out.evidence_status = Some(format!("{}:{}", record.kind.name(), record.extractor));
    loaded(out, record.scope.clone(), Some(record.valid_from.clone()))
}

/// Makes the item of a task.
pub fn task_item(task: &TaskRecord) -> Loaded {
    let title = match &task.external_ref {
        Some(external) => format!("{external} {}", task.title),
        None => task.title.clone(),
    };
    let mut out = item(format!("task:{}", task.task_id), "task", title);
    out.status = Some(task.state.name().into());
    loaded(out, task.scope.clone(), Some(task.created_utc.clone()))
}

/// Makes the item of a commit.
pub fn commit_item(commit: &CommitRecord) -> Loaded {
    let title = commit.message.lines().next().unwrap_or_default().to_string();
    let mut out = item(format!("commit:{}", commit.commit_id), "commit", title);
    out.revision = Some(short(&commit.sha));
    out.status = Some("verified".into());
    out.evidence_status = Some("verified_commit".into());
    loaded(out, commit.scope.clone(), Some(commit.author_utc.clone()))
}

/// Makes the item of an event.
pub fn event_item(objects: &ObjectStore, event: &EventEnvelope) -> Loaded {
    let text = zcie_store::events::payload_text(objects, event).ok().flatten().unwrap_or_default();
    let title = format!("{}: {}", event.event_type.name(), text);
    let mut out = item(format!("ev:{}", event.event_id), "event", title);
    if text.chars().count() > TITLE_CHARS {
        out.snippet = Some(truncate_chars(&text, EVENT_SNIPPET_CHARS));
    }
    Loaded { item: out, scope: event.scope.clone(), classification: event.classification, time: Some(event.timestamp_utc.clone()) }
}
