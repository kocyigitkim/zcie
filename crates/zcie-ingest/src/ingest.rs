//! The `Ingest` struct: shared state of the write path.

use std::collections::HashSet;
use std::sync::{Arc, Mutex, MutexGuard};

use zcie_parse::ParserPool;
use zcie_runtime::JobQueue;
use zcie_search::SearchSender;
use zcie_store::{ObjectStore, Store};
use zcie_telemetry::Counters;

use crate::config::IngestConfig;
use crate::overlay::LiveOverlay;

/// The write path: capture, jobs and processors.
pub struct Ingest {
    /// Canonical store.
    pub store: Arc<Store>,
    /// Blob store.
    pub objects: Arc<ObjectStore>,
    /// In-memory job queue.
    pub queue: Arc<JobQueue>,
    /// Paths with pending file jobs.
    pub overlay: LiveOverlay,
    /// Sender to the search writer.
    pub search: SearchSender,
    /// Shared counters.
    pub counters: Arc<Counters>,
    /// Settings.
    pub config: IngestConfig,
    /// Last event sequence. The mutex is the capture lock.
    pub(crate) event_seq: Mutex<u64>,
    /// Serializes the write phase of job handlers.
    pub(crate) write_lock: Mutex<()>,
    /// Job IDs that a worker runs now.
    pub(crate) running: Mutex<HashSet<String>>,
    /// Tree-sitter parsers.
    pub(crate) parser: ParserPool,
}

impl Ingest {
    /// Makes the write path. `event_seq` is the last stored event sequence.
    pub fn new(
        store: Arc<Store>,
        objects: Arc<ObjectStore>,
        queue: Arc<JobQueue>,
        search: SearchSender,
        counters: Arc<Counters>,
        config: IngestConfig,
        event_seq: u64,
    ) -> Ingest {
        Ingest {
            store,
            objects,
            queue,
            overlay: LiveOverlay::default(),
            search,
            counters,
            config,
            event_seq: Mutex::new(event_seq),
            write_lock: Mutex::new(()),
            running: Mutex::new(HashSet::new()),
            parser: ParserPool::new(),
        }
    }

    /// Returns the tenant ID.
    pub fn tenant(&self) -> &str {
        &self.config.tenant
    }

    /// Returns the last event sequence.
    pub fn event_seq(&self) -> u64 {
        *self.lock_seq()
    }

    pub(crate) fn lock_seq(&self) -> MutexGuard<'_, u64> {
        self.event_seq.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub(crate) fn lock_write(&self) -> MutexGuard<'_, ()> {
        self.write_lock.lock().unwrap_or_else(|e| e.into_inner())
    }
}
