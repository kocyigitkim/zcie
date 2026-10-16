//! The search index and its single writer thread (`docs/07-ingest-and-indexing.md` section 9.3).

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, SendError, SyncSender, sync_channel};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use tantivy::directory::MmapDirectory;
use tantivy::{Index, IndexReader, IndexWriter, ReloadPolicy, TantivyDocument, Term};
use zcie_store::Store;
use zcie_store::watermarks::{GRAPH, SEARCH, raise_watermark, read_watermark};

use crate::error::{Error, Result};
use crate::lexical::document::{SearchDoc, SearchOp};
use crate::lexical::schema::{Fields, IndexName, build_schema, expand_identifiers};

/// Commit interval of the writer.
const COMMIT_INTERVAL: Duration = Duration::from_secs(1);
/// Commit after this many pending operations.
const COMMIT_BATCH: usize = 500;
/// Memory budget of one Tantivy writer.
const WRITER_MEMORY_BYTES: usize = 50_000_000;
/// Capacity of the operation channel.
const CHANNEL_CAPACITY: usize = 10_000;

/// The three Tantivy indexes with their readers.
pub struct SearchIndex {
    pub(crate) indexes: Vec<Index>,
    pub(crate) readers: Vec<IndexReader>,
    pub(crate) fields: Fields,
}

impl SearchIndex {
    /// Opens or creates the three indexes in `root`.
    pub fn open(root: &Path) -> Result<SearchIndex> {
        let (schema, fields) = build_schema();
        let mut indexes = Vec::new();
        let mut readers = Vec::new();
        for name in IndexName::ALL {
            let folder = root.join(name.folder());
            std::fs::create_dir_all(&folder)?;
            let index = Index::open_or_create(MmapDirectory::open(&folder)?, schema.clone())?;
            let reader = index.reader_builder().reload_policy(ReloadPolicy::OnCommitWithDelay).try_into()?;
            indexes.push(index);
            readers.push(reader);
        }
        Ok(SearchIndex { indexes, readers, fields })
    }
}

/// A sender of search operations. Clone it for each producer.
///
/// It counts the write operations that are sent but not committed yet.
#[derive(Clone)]
pub struct SearchSender {
    inner: SyncSender<SearchOp>,
    pending: Arc<AtomicU64>,
}

impl SearchSender {
    /// Sends one operation to the writer thread.
    pub fn send(&self, op: SearchOp) -> std::result::Result<(), SendError<SearchOp>> {
        let counted = matches!(op, SearchOp::Upsert(..) | SearchOp::Delete(_) | SearchOp::DeleteAll);
        if counted {
            self.pending.fetch_add(1, Ordering::SeqCst);
        }
        let result = self.inner.send(op);
        if result.is_err() && counted {
            self.pending.fetch_sub(1, Ordering::SeqCst);
        }
        result
    }

    /// Counts `count` operations as pending before they are sent. Call `release` after the sends.
    ///
    /// A job holds its operations while it writes its batch, so an observer never sees
    /// "no pending job and no pending search operation" between the batch write and the sends.
    pub fn hold(&self, count: u64) {
        self.pending.fetch_add(count, Ordering::SeqCst);
    }

    /// Removes a count that `hold` added.
    pub fn release(&self, count: u64) {
        self.pending.fetch_sub(count, Ordering::SeqCst);
    }

    /// Returns the number of write operations that are not committed yet.
    pub fn pending(&self) -> u64 {
        self.pending.load(Ordering::SeqCst)
    }
}

/// The single writer of the search indexes.
pub struct SearchWriter {
    sender: SearchSender,
    handle: Option<JoinHandle<()>>,
}

struct WriterState {
    dirty: bool,
    applied: u64,
    pending: Arc<AtomicU64>,
    writers: Vec<IndexWriter<TantivyDocument>>,
    readers: Vec<IndexReader>,
    fields: Fields,
    store: Arc<Store>,
    tenant: String,
}

impl SearchWriter {
    /// Starts the writer thread.
    pub fn start(search: &SearchIndex, store: Arc<Store>, tenant: &str) -> Result<SearchWriter> {
        let mut writers = Vec::new();
        for index in &search.indexes {
            writers.push(index.writer(WRITER_MEMORY_BYTES)?);
        }
        let pending = Arc::new(AtomicU64::new(0));
        let state = WriterState {
            dirty: false,
            applied: 0,
            pending: Arc::clone(&pending),
            writers,
            readers: search.readers.clone(),
            fields: search.fields,
            store,
            tenant: tenant.to_string(),
        };
        let (inner, receiver) = sync_channel(CHANNEL_CAPACITY);
        let sender = SearchSender { inner, pending };
        let handle = std::thread::Builder::new()
            .name("zcie-search-writer".into())
            .spawn(move || writer_loop(state, receiver))?;
        Ok(SearchWriter { sender, handle: Some(handle) })
    }

    /// Returns a sender for search operations.
    pub fn sender(&self) -> SearchSender {
        self.sender.clone()
    }

    /// Commits now and waits for the commit.
    pub fn flush(&self) -> Result<()> {
        flush(&self.sender)
    }

    /// Commits and stops the writer thread. Other senders can still exist; `Stop` ends the loop.
    pub fn stop(mut self) {
        let _ = self.flush();
        let _ = self.sender.inner.send(SearchOp::Stop);
        if let Some(handle) = self.handle.take()
            && handle.join().is_err()
        {
            tracing::error!("search writer panicked");
        }
    }
}

/// Sends a flush operation and waits for the commit.
pub fn flush(sender: &SearchSender) -> Result<()> {
    let (ack, done) = sync_channel(1);
    sender.inner.send(SearchOp::Flush(ack)).map_err(|_| Error::WriterStopped)?;
    done.recv().map_err(|_| Error::WriterStopped)?;
    Ok(())
}

fn writer_loop(mut state: WriterState, receiver: Receiver<SearchOp>) {
    let mut pending = 0usize;
    let mut last_commit = Instant::now();
    loop {
        let wait = COMMIT_INTERVAL.saturating_sub(last_commit.elapsed()).max(Duration::from_millis(10));
        let stop = match receiver.recv_timeout(wait) {
            Ok(SearchOp::Flush(ack)) => {
                let stop = commit(&mut state, &receiver);
                let _ = ack.send(true);
                stop
            }
            Ok(SearchOp::Stop) | Err(RecvTimeoutError::Disconnected) => {
                commit(&mut state, &receiver);
                true
            }
            Ok(op) => {
                apply(&mut state, op);
                pending += 1;
                if pending < COMMIT_BATCH {
                    continue;
                }
                commit(&mut state, &receiver)
            }
            Err(RecvTimeoutError::Timeout) => commit(&mut state, &receiver),
        };
        pending = 0;
        last_commit = Instant::now();
        if stop {
            break;
        }
    }
}

/// Reads `graph_seq` (G), drains the channel, commits and writes `search_seq = G`.
/// Returns true if a `Stop` operation was drained.
fn commit(state: &mut WriterState, receiver: &Receiver<SearchOp>) -> bool {
    let graph = read_watermark(&state.store, &state.tenant, GRAPH).unwrap_or(0);
    let mut acks = Vec::new();
    let mut stop = false;
    while let Ok(op) = receiver.try_recv() {
        match op {
            SearchOp::Flush(ack) => acks.push(ack),
            SearchOp::Stop => stop = true,
            other => apply(state, other),
        }
    }
    if state.dirty {
        for writer in &mut state.writers {
            if let Err(error) = writer.commit() {
                tracing::error!(%error, "search commit failed");
                return stop;
            }
        }
        for reader in &state.readers {
            let _ = reader.reload();
        }
        state.dirty = false;
    }
    state.pending.fetch_sub(state.applied, Ordering::SeqCst);
    state.applied = 0;
    if let Err(error) = raise_watermark(&state.store, &state.tenant, SEARCH, graph) {
        tracing::error!(%error, "search watermark update failed");
    }
    for ack in acks {
        let _ = ack.send(true);
    }
    stop
}

fn apply(state: &mut WriterState, op: SearchOp) {
    let fields = state.fields;
    if matches!(op, SearchOp::Upsert(..) | SearchOp::Delete(_) | SearchOp::DeleteAll) {
        state.dirty = true;
        state.applied += 1;
    }
    match op {
        SearchOp::Upsert(name, doc) => {
            let writer = &mut state.writers[name.position()];
            writer.delete_term(Term::from_field_text(fields.entity_ref, &doc.entity_ref));
            if let Err(error) = writer.add_document(to_document(&fields, &doc)) {
                tracing::error!(%error, "search add failed");
            }
        }
        SearchOp::Delete(entity_ref) => {
            for writer in &mut state.writers {
                writer.delete_term(Term::from_field_text(fields.entity_ref, &entity_ref));
            }
        }
        SearchOp::DeleteAll => {
            for writer in &mut state.writers {
                if let Err(error) = writer.delete_all_documents() {
                    tracing::error!(%error, "search delete all failed");
                }
            }
        }
        SearchOp::Flush(ack) => {
            let _ = ack.send(true);
        }
        SearchOp::Stop => {}
    }
}

fn to_document(fields: &Fields, doc: &SearchDoc) -> TantivyDocument {
    let mut out = TantivyDocument::default();
    out.add_text(fields.entity_ref, &doc.entity_ref);
    out.add_text(fields.kind, &doc.kind);
    out.add_text(fields.tenant_id, &doc.tenant_id);
    out.add_text(fields.workspace_id, &doc.workspace_id);
    out.add_text(fields.repository_id, &doc.repository_id);
    out.add_text(fields.worktree_id, &doc.worktree_id);
    out.add_text(fields.path, &doc.path);
    out.add_text(fields.name, expand_identifiers(&doc.name));
    out.add_text(fields.body, expand_identifiers(&doc.body));
    out.add_u64(fields.seq, doc.seq);
    out
}
