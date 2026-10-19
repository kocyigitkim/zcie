//! Loads entities by ref and turns them into result items.

use zcie_store::{Cf, ObjectStore, Store, layout};
use zcie_types::{
    Classification, CommitRecord, EventEnvelope, FileRecord, MemoryKind, MemoryRecord, MemoryStatus, Ref, ResultItem, Scope,
    SymbolRecord, TaskRecord, truncate_chars,
};

use crate::error::Result;

/// Characters of a title.
const TITLE_CHARS: usize = 160;
/// Characters of the full text of an event or memory snippet; the planner shows a window of it.
const EVENT_SNIPPET_CHARS: usize = 200_000;
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
        project: None,
        context: None,
        follow_up: None,
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
        "mem" => match store.get::<MemoryRecord>(Cf::Entities, &key("mem"))? {
            Some(m) if m.status != MemoryStatus::Retracted => Some(memory_with_source(store, objects, tenant, &m)?),
            _ => None,
        },
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
    out.context = record.context.clone();
    loaded(out, record.scope.clone(), Some(record.valid_from.clone()))
}

/// Makes the item of a memory record. If the source message is longer than the record, its text
/// becomes the snippet, so that a reason in the next sentence ("We rejected MySQL because ...") or a
/// passage of a long pasted document is not lost.
fn memory_with_source(store: &Store, objects: &ObjectStore, tenant: &str, record: &MemoryRecord) -> Result<Loaded> {
    let mut out = memory_item(record);
    // A decision or preference is one sentence of a message; a request record keeps only the first
    // `MAX_MEMORY_TEXT` characters of the message. Both show the full message (the planner windows it).
    let from_message = matches!(record.kind, MemoryKind::DesignDecision | MemoryKind::Preference | MemoryKind::UserRequirement);
    let Some(source) = record.source_event_ids.first().filter(|_| from_message) else {
        return Ok(out);
    };
    let Some(event) = zcie_store::events::get_event_by_id(store, tenant, &source.0)?.filter(|e| !e.forgotten) else {
        return Ok(out);
    };
    // A sentence of an assistant answer is not a fact that the user confirmed (ADR-020, D-073).
    if event.event_type == zcie_types::EventType::AssistantMessage {
        out.item.evidence_status = Some(ASSISTANT_STATEMENT.into());
    }
    let Some(text) = zcie_store::events::payload_text(objects, &event).ok().flatten() else { return Ok(out) };
    if text.trim_end_matches(['.', ' ', '\n']).chars().count() <= record.text.trim_end_matches(['.', ' ']).chars().count() {
        return Ok(out);
    }
    // A request is the message itself. A decision or preference is one sentence: it gets the text
    // around the sentence, not the full message, so that a sentence of a long document does not
    // match a question by the words of other parts of the document.
    out.item.snippet = match record.kind {
        MemoryKind::UserRequirement => Some(truncate_chars(&text, EVENT_SNIPPET_CHARS)),
        _ => sentence_window(&text, &record.text).or(out.item.snippet),
    };
    Ok(out)
}

/// Evidence status of a memory record from an assistant answer.
pub const ASSISTANT_STATEMENT: &str = "assistant_statement";

/// Characters before the sentence in a sentence window.
const WINDOW_BEFORE_CHARS: usize = 120;
/// Characters of a sentence window (less than the query snippet window, so it is not cut again).
const SENTENCE_WINDOW_CHARS: usize = 390;

/// Returns the part of `source` around `sentence`: `WINDOW_BEFORE_CHARS` before it and the rest of
/// `SENTENCE_WINDOW_CHARS` after its start. `None` if the source does not have the sentence.
fn sentence_window(source: &str, sentence: &str) -> Option<String> {
    let probe: String = sentence.trim().chars().take(60).collect();
    let at = source.find(probe.trim_end_matches(['.', ' ']))?;
    let before: Vec<char> = source[..at].chars().rev().take(WINDOW_BEFORE_CHARS).collect();
    let after: String = source[at..].chars().take(SENTENCE_WINDOW_CHARS - before.len()).collect();
    let prefix = if before.len() < source[..at].chars().count() { "… " } else { "" };
    let suffix = if at + after.len() < source.len() { " …" } else { "" };
    Some(format!("{prefix}{}{after}{suffix}", before.iter().rev().collect::<String>()))
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

/// Score factor of tool-run logs (`command.run`, `tool.result` events): an agent runs many commands
/// whose text repeats the words of the task, so they would push the facts out of the results.
const TOOL_LOG_FACTOR: f32 = 0.5;
/// Score factor of a sentence of an assistant answer: a fact of the user ranks first (ADR-020).
const ASSISTANT_FACTOR: f32 = 0.8;

/// Score factor of the source of a candidate: tool logs and assistant statements rank lower.
pub(crate) fn source_factor(item: &ResultItem) -> f32 {
    if item.kind == "event" && (item.title.starts_with("command.run") || item.title.starts_with("tool.result")) {
        TOOL_LOG_FACTOR
    } else if item.evidence_status.as_deref() == Some(ASSISTANT_STATEMENT) {
        ASSISTANT_FACTOR
    } else {
        1.0
    }
}
