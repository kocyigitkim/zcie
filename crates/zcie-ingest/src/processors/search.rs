//! Search documents for entities, and the search rebuild (`docs/06-storage.md` section 9).

use zcie_search::{IndexName, SearchDoc, SearchOp};
use zcie_store::watermarks::{SEARCH, reset_watermark};
use zcie_store::{Batch, Cf, layout};
use zcie_types::{
    CommitRecord, EventEnvelope, FileRecord, JobRecord, MemoryRecord, MemoryStatus, Ref, Scope, SymbolRecord, TaskRecord,
    truncate_chars,
};

use crate::error::Result;
use crate::ingest::Ingest;
use crate::processors::Output;

/// Maximum entities of one kind in a rebuild.
const MAX_REBUILD_ITEMS: usize = 10_000_000;

fn base_doc(entity_ref: String, kind: &str, scope: &Scope, seq: u64) -> SearchDoc {
    SearchDoc {
        entity_ref,
        kind: kind.to_string(),
        tenant_id: scope.tenant_id.clone(),
        workspace_id: scope.workspace_id.clone(),
        repository_id: scope.repository_id.clone().unwrap_or_default(),
        worktree_id: scope.worktree_id.clone().unwrap_or_default(),
        seq,
        ..SearchDoc::default()
    }
}

/// Makes the upsert operation of an event.
pub fn event_op(event: &EventEnvelope, text: &str) -> SearchOp {
    let mut doc = base_doc(format!("ev:{}", event.event_id), "event", &event.scope, event.seq);
    doc.name = format!("{} {}", event.event_type.name(), truncate_chars(text, 80));
    doc.body = text.to_string();
    SearchOp::Upsert(IndexName::Conversations, Box::new(doc))
}

/// Makes the operation of a memory record. A retracted record is deleted.
pub fn memory_op(record: &MemoryRecord) -> SearchOp {
    let entity_ref = format!("mem:{}", record.memory_id);
    if record.status == MemoryStatus::Retracted {
        return SearchOp::Delete(entity_ref);
    }
    let mut doc = base_doc(entity_ref, "memory", &record.scope, record.generation);
    doc.name = truncate_chars(&record.text, 120);
    doc.body = format!("{} {}", record.kind.name(), record.text);
    SearchOp::Upsert(IndexName::Conversations, Box::new(doc))
}

/// Makes the upsert operation of a task.
pub fn task_op(task: &TaskRecord) -> SearchOp {
    let mut doc = base_doc(format!("task:{}", task.task_id), "task", &task.scope, task.generation);
    doc.name = task.title.clone();
    doc.body = format!("{} {} {}", task.external_ref.clone().unwrap_or_default(), task.title, task.state.name());
    SearchOp::Upsert(IndexName::Conversations, Box::new(doc))
}

/// Makes the upsert operation of a commit.
pub fn commit_op(commit: &CommitRecord) -> SearchOp {
    let mut doc = base_doc(format!("commit:{}", commit.commit_id), "commit", &commit.scope, 0);
    doc.name = commit.message.lines().next().unwrap_or_default().to_string();
    doc.body = format!("{} {} {}", commit.sha, commit.message, commit.changed_paths.join(" "));
    SearchOp::Upsert(IndexName::Conversations, Box::new(doc))
}

/// Makes the operation of a file with its text, or `None` for a file with no text.
pub fn file_op(file: &FileRecord, text: Option<&str>) -> Option<SearchOp> {
    let index = if file.language.is_some() {
        IndexName::Code
    } else if text.is_some() {
        IndexName::Documents
    } else {
        return None;
    };
    let mut doc = base_doc(format!("file:{}", file.file_id), "file", &file.scope, file.indexed_seq);
    doc.path = file.path.clone();
    doc.name = file.path.rsplit('/').next().unwrap_or(&file.path).to_string();
    doc.body = text.unwrap_or_default().to_string();
    Some(SearchOp::Upsert(index, Box::new(doc)))
}

/// Makes the upsert operation of a symbol. `lines` are the lines of the file.
pub fn symbol_op(symbol: &SymbolRecord, lines: &[&str]) -> SearchOp {
    let mut doc = base_doc(format!("sym:{}", symbol.symbol_id), "symbol", &symbol.scope, 0);
    doc.path = symbol.path.clone();
    doc.name = symbol.qualified_name.clone();
    let start = (symbol.span[0] as usize).saturating_sub(1);
    let end = (symbol.span[1] as usize).min(lines.len());
    let source = if start < end { lines[start..end].join("\n") } else { String::new() };
    doc.body = format!("{} {}\n{}", symbol.kind.name(), symbol.signature, source);
    SearchOp::Upsert(IndexName::Code, Box::new(doc))
}

impl Ingest {
    /// Returns the redacted text of a stored file, or `None`.
    pub fn file_text(&self, file: &FileRecord) -> Option<String> {
        if !file.content_stored || file.is_binary {
            return None;
        }
        let text = self.objects.get_text(&file.content_hash).ok()?;
        Some(if self.config.redact_secrets { zcie_security::redact(&text).0 } else { text })
    }

    /// Adds search operations for changed memory and task refs. Reads the batch first.
    pub fn ops_for_refs(&self, batch: &Batch, refs: &[String]) -> Result<Vec<SearchOp>> {
        let t = self.tenant();
        let mut ops = Vec::new();
        for value in refs {
            let Ok(parsed) = Ref::parse(value) else { continue };
            let kind = match parsed.kind.as_str() {
                "mem" | "task" => parsed.kind.clone(),
                _ => continue,
            };
            let key = layout::entity(t, &kind, &parsed.id);
            if kind == "mem" {
                let record = batch.pending::<MemoryRecord>(Cf::Entities, &key).or(self.store.get(Cf::Entities, &key)?);
                ops.extend(record.map(|r| memory_op(&r)));
            } else {
                let task = batch.pending::<TaskRecord>(Cf::Entities, &key).or(self.store.get(Cf::Entities, &key)?);
                ops.extend(task.map(|t| task_op(&t)));
            }
        }
        Ok(ops)
    }

    /// The `UpdateSearch` handler: deletes all documents and indexes all current entities again.
    pub(crate) fn rebuild_search_job(&self, _job: &JobRecord) -> Result<Output> {
        let t = self.tenant().to_string();
        reset_watermark(&self.store, &t, SEARCH, 0)?;
        self.send(SearchOp::DeleteAll);
        self.rebuild_files_and_symbols(&t)?;
        for (_, record) in self.store.prefix_scan::<MemoryRecord>(Cf::Entities, &layout::entity_prefix(&t, "mem"), MAX_REBUILD_ITEMS)? {
            self.send(memory_op(&record));
        }
        for (_, task) in self.store.prefix_scan::<TaskRecord>(Cf::Entities, &layout::entity_prefix(&t, "task"), MAX_REBUILD_ITEMS)? {
            self.send(task_op(&task));
        }
        for (_, commit) in self.store.prefix_scan::<CommitRecord>(Cf::Entities, &layout::entity_prefix(&t, "commit"), MAX_REBUILD_ITEMS)? {
            self.send(commit_op(&commit));
        }
        for (_, event) in self.store.prefix_scan::<EventEnvelope>(Cf::Events, &layout::events_prefix(&t), MAX_REBUILD_ITEMS)? {
            if event.event_type.is_conversation()
                && let Some(text) = zcie_store::events::payload_text(&self.objects, &event)?
            {
                self.send(event_op(&event, &text));
            }
        }
        zcie_search::flush(&self.search)?;
        Ok(Output::default())
    }

    fn rebuild_files_and_symbols(&self, t: &str) -> Result<()> {
        for (_, file) in self.store.prefix_scan::<FileRecord>(Cf::Entities, &layout::entity_prefix(t, "file"), MAX_REBUILD_ITEMS)? {
            if file.deleted {
                continue;
            }
            let text = self.file_text(&file);
            if let Some(op) = file_op(&file, text.as_deref()) {
                self.send(op);
            }
            let content = text.unwrap_or_default();
            let lines: Vec<&str> = content.lines().collect();
            for symbol in zcie_symbols::file_symbols(&self.store, t, file.file_id.as_str())? {
                self.send(symbol_op(&symbol, &lines));
            }
        }
        Ok(())
    }

    pub(crate) fn send(&self, op: SearchOp) {
        if self.search.send(op).is_err() {
            tracing::error!("search writer is stopped");
        }
    }
}
