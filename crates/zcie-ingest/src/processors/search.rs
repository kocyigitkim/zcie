//! Search documents for entities, and the search rebuild (`docs/06-storage.md` section 9).

use zcie_search::{IndexName, SearchDoc, SearchOp};
use zcie_store::codec::NumberValue;
use zcie_store::recovery::for_each_page;
use zcie_store::watermarks::{SEARCH, reset_watermark};
use zcie_store::{Batch, Cf, layout};
use zcie_types::{
    CommitRecord, EventEnvelope, FileRecord, JobRecord, MemoryRecord, MemoryStatus, Ref, Scope, SymbolRecord, TaskRecord,
    truncate_chars,
};

use crate::error::Result;
use crate::ingest::Ingest;
use crate::processors::Output;

/// Entities that one page of a rebuild or a vector fill reads.
const PAGE: usize = 1_000;

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

/// Makes the upsert operation of an event. A long text (a pasted document) is indexed as
/// passages, so that the passage that answers a question gets a fair score (ADR-013).
pub fn event_op(event: &EventEnvelope, text: &str) -> SearchOp {
    let mut doc = base_doc(format!("ev:{}", event.event_id), "event", &event.scope, event.seq);
    doc.name = format!("{} {}", event.event_type.name(), truncate_chars(text, 80));
    let parts = zcie_search::passages(text);
    if parts.len() > 1 {
        return SearchOp::UpsertPassages(IndexName::Conversations, Box::new(doc), parts);
    }
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
    if let Some(context) = &record.context {
        doc.body = format!("{}\nprevious message: {context}", doc.body);
    }
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
    // A long document (Markdown, text) is indexed as passages; code has its symbols.
    if index == IndexName::Documents {
        let parts = zcie_search::passages(text.unwrap_or_default());
        if parts.len() > 1 {
            return Some(SearchOp::UpsertPassages(index, Box::new(doc), parts));
        }
    }
    doc.body = text.unwrap_or_default().to_string();
    Some(SearchOp::Upsert(index, Box::new(doc)))
}

/// Most comment lines above a symbol that its document includes.
const MAX_DOC_LINES: usize = 8;

/// Returns the comment lines right above line `start` (doc comments); attribute lines are skipped.
/// Lines at the top of a file that the context of a symbol takes from its file comment.
const FILE_COMMENT_LINES: usize = 15;

/// The context of a symbol for the index (ADR-017, contextual retrieval with no model): the words
/// of its path, its containers (module, type) and the first comment line of its file.
fn symbol_context(symbol: &SymbolRecord, lines: &[&str]) -> String {
    let path_words = symbol.path.replace(['/', '.', '_', '-'], " ");
    let containers = symbol.qualified_name.rsplit_once(['.', ':']).map(|(c, _)| c.replace("::", " ")).unwrap_or_default();
    let file_comment = lines
        .iter()
        .take(FILE_COMMENT_LINES)
        .map(|l| l.trim())
        .find(|l| (l.starts_with("//") || l.starts_with('#') || l.starts_with("\"\"\"")) && !l.starts_with("#!") && !l.starts_with("#[") && l.len() > 4)
        .unwrap_or_default();
    format!("context: {path_words} {containers} {file_comment}")
}

fn doc_comments(lines: &[&str], start: usize) -> String {
    let mut out: Vec<&str> = Vec::new();
    for line in lines[..start.min(lines.len())].iter().rev().map(|l| l.trim()) {
        if line.starts_with("#[") || line.starts_with('@') {
            continue;
        }
        let comment = ["///", "//", "/**", "*", "#"].iter().any(|p| line.starts_with(p));
        if !comment || out.len() == MAX_DOC_LINES {
            break;
        }
        out.push(line);
    }
    out.reverse();
    out.join("\n")
}

/// Makes the upsert operation of a symbol. `lines` are the lines of the file. The body has the
/// signature, the doc comments above the symbol and the source.
pub fn symbol_op(symbol: &SymbolRecord, lines: &[&str]) -> SearchOp {
    let mut doc = base_doc(format!("sym:{}", symbol.symbol_id), "symbol", &symbol.scope, 0);
    doc.path = symbol.path.clone();
    doc.name = symbol.qualified_name.clone();
    let start = (symbol.span[0] as usize).saturating_sub(1);
    let end = (symbol.span[1] as usize).min(lines.len());
    let source = if start < end { lines[start..end].join("\n") } else { String::new() };
    doc.body = format!("{} {}\n{}\n{}\n{}", symbol.kind.name(), symbol.signature, doc_comments(lines, start), symbol_context(symbol, lines), source);
    SearchOp::Upsert(IndexName::Code, Box::new(doc))
}

/// Vectors stored under one write lock during the startup fill.
const FILL_BATCH: usize = 512;

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
    pub(crate) fn rebuild_search_job(&self, job: &JobRecord) -> Result<Output> {
        let t = self.tenant().to_string();
        // Each rebuild starts with "delete all". A rebuild that is not newer than the last started
        // one has nothing to add, and running it would delete the documents of that rebuild.
        let key = layout::registry(&t, "search", "rebuild_seq");
        let last: Option<NumberValue> = self.store.get(Cf::Registry, &key)?;
        if last.is_some_and(|last| job.source_seq <= last.value) {
            tracing::info!(seq = job.source_seq, "search rebuild skipped: a newer rebuild started");
            return Ok(Output::default());
        }
        self.store.put_sync(Cf::Registry, &key, &NumberValue::new(self.event_seq().max(job.source_seq)))?;
        tracing::info!(seq = job.source_seq, "search rebuild started");
        reset_watermark(&self.store, &t, SEARCH, 0)?;
        self.send(SearchOp::DeleteAll);
        // Memory, tasks, commits and conversations first: they are small and the context hooks
        // need them at once. Files and symbols (most of the data) follow.
        self.each_record_op(&t, &mut |op| self.send(op))?;
        self.each_event_op(&t)?;
        self.each_file_and_symbol_op(&t, true, &mut |op| self.send(op))?;
        zcie_search::flush(&self.search)?;
        tracing::info!(seq = job.source_seq, "search rebuild completed");
        Ok(Output::default())
    }

    fn each_event_op(&self, t: &str) -> Result<()> {
        let mut result: Result<()> = Ok(());
        for_each_page::<EventEnvelope>(&self.store, Cf::Events, &layout::events_prefix(t), PAGE, &mut |page| {
            for (_, event) in page.iter().filter(|(_, e)| e.event_type.is_conversation()) {
                match zcie_store::events::payload_text(&self.objects, event) {
                    Ok(Some(text)) => self.send(event_op(event, &text)),
                    Ok(None) => {}
                    Err(error) => result = Err(error.into()),
                }
            }
            Ok(())
        })?;
        result
    }

    /// Fills the vector index from the store (ADR-011). An existing vector is kept: the search
    /// writer made it from a newer state.
    pub fn fill_vectors(&self, vectors: &zcie_vector::Vectors) -> Result<()> {
        let t = self.tenant().to_string();
        // Vectors are computed with no lock and stored in batches under one write lock, so that
        // queries during the fill wait little (a lock for each vector slowed both).
        let mut batch: Vec<(String, String, Vec<f32>)> = Vec::with_capacity(FILL_BATCH);
        let mut embed = |op: SearchOp| {
            if let SearchOp::Upsert(_, doc) = op {
                let entity = zcie_vector::Entity { kind: &doc.kind, name: &doc.name, path: &doc.path, body: &doc.body };
                if let Some(vector) = vectors.embed_entity(&entity) {
                    batch.push((doc.workspace_id.clone(), doc.entity_ref.clone(), vector));
                }
                if batch.len() >= FILL_BATCH {
                    vectors.index.insert_many_if_absent(&batch);
                    batch.clear();
                }
            }
        };
        // Files are not embedded, so their documents (and passages) are not built here.
        self.each_record_op(&t, &mut embed)?;
        self.each_file_and_symbol_op(&t, false, &mut embed)?;
        vectors.index.insert_many_if_absent(&batch);
        Ok(())
    }

    /// Calls `f` with the search operation of each memory record, task and commit.
    fn each_record_op(&self, t: &str, f: &mut dyn FnMut(SearchOp)) -> Result<()> {
        for_each_page::<MemoryRecord>(&self.store, Cf::Entities, &layout::entity_prefix(t, "mem"), PAGE, &mut |page| {
            page.iter().for_each(|(_, record)| f(memory_op(record)));
            Ok(())
        })?;
        for_each_page::<TaskRecord>(&self.store, Cf::Entities, &layout::entity_prefix(t, "task"), PAGE, &mut |page| {
            page.iter().for_each(|(_, task)| f(task_op(task)));
            Ok(())
        })?;
        for_each_page::<CommitRecord>(&self.store, Cf::Entities, &layout::entity_prefix(t, "commit"), PAGE, &mut |page| {
            page.iter().for_each(|(_, commit)| f(commit_op(commit)));
            Ok(())
        })?;
        Ok(())
    }

    /// Calls `f` with the search operation of each current symbol, and of each file if `with_files`.
    /// The store is read in pages, so the memory use does not grow with the store.
    fn each_file_and_symbol_op(&self, t: &str, with_files: bool, f: &mut dyn FnMut(SearchOp)) -> Result<()> {
        let mut result: Result<()> = Ok(());
        for_each_page::<FileRecord>(&self.store, Cf::Entities, &layout::entity_prefix(t, "file"), PAGE, &mut |page| {
            for (_, file) in page.iter().filter(|(_, file)| !file.deleted) {
                if let Err(error) = self.file_and_symbol_ops(t, file, with_files, f) {
                    result = Err(error);
                }
            }
            Ok(())
        })?;
        result
    }

    fn file_and_symbol_ops(&self, t: &str, file: &FileRecord, with_files: bool, f: &mut dyn FnMut(SearchOp)) -> Result<()> {
        let text = self.file_text(file);
        if with_files && let Some(op) = file_op(file, text.as_deref()) {
            f(op);
        }
        let content = text.unwrap_or_default();
        let lines: Vec<&str> = content.lines().collect();
        for symbol in zcie_symbols::file_symbols(&self.store, t, file.file_id.as_str())? {
            f(symbol_op(&symbol, &lines));
        }
        Ok(())
    }

    pub(crate) fn send(&self, op: SearchOp) {
        if self.search.send(op).is_err() {
            tracing::error!("search writer is stopped");
        }
    }
}
