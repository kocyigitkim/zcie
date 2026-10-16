//! Engine startup, recovery and shutdown (`docs/06-storage.md` section 8).

use std::sync::Arc;
use std::time::Duration;

use zcie_api::{ApiConfig, Engine, EngineParts};
use zcie_cache::L0Cache;
use zcie_ingest::{Ingest, IngestConfig};
use zcie_query::{QueryConfig, QueryEngine};
use zcie_runtime::{JobQueue, Shutdown, WorkerPool};
use zcie_search::{SearchIndex, SearchWriter};
use zcie_security::ExcludeSet;
use zcie_store::recovery::{pending_jobs, read_event_seq};
use zcie_store::watermarks::{GRAPH, SEARCH, read_watermark};
use zcie_store::{ObjectStore, Store};
use zcie_telemetry::Counters;
use zcie_types::{JobKind, JobPriority, JobRecord, LOCAL_TENANT, Scope};

use crate::config::EngineConfig;
use crate::error::Result;
use crate::maintenance::Maintenance;
use crate::tree::{InstancePaths, check_manifest, create_tree};
use crate::watchers::Watchers;

/// A running engine with its background parts.
pub struct RunningEngine {
    /// The engine. Gateways call it.
    pub engine: Arc<Engine>,
    /// Key of a new admin client, if `clients.toml` was made now. Print it once.
    pub new_admin_key: Option<String>,
    /// Instance paths.
    pub paths: InstancePaths,
    workers: WorkerPool,
    search_writer: SearchWriter,
    shutdown: Shutdown,
    watchers: Arc<Watchers>,
    maintenance: Maintenance,
}

/// Starts the engine: runtime tree, store, recovery, workers, search writer, watchers.
///
/// The caller holds the daemon lock (ADR-003).
pub fn start_engine(paths: InstancePaths, config: EngineConfig) -> Result<RunningEngine> {
    create_tree(&paths)?;
    check_manifest(&paths)?;
    let store = Arc::new(Store::open(&paths.rocksdb())?);
    let event_seq = read_event_seq(&store, LOCAL_TENANT)?;
    let objects = Arc::new(ObjectStore::new(paths.objects())?);
    let removed = objects.remove_temp_files()?;
    tracing::info!(event_seq, removed_temp_blobs = removed, "store opened");
    let (search, rebuild_needed) = open_search(&paths)?;
    let search = Arc::new(search);
    let search_writer = SearchWriter::start(&search, Arc::clone(&store), LOCAL_TENANT)?;
    let queue = Arc::new(JobQueue::new(config.ingest.queue_capacity));
    let ingest = Arc::new(Ingest::new(
        Arc::clone(&store),
        Arc::clone(&objects),
        Arc::clone(&queue),
        search_writer.sender(),
        Arc::new(Counters::default()),
        ingest_config(&config, &paths)?,
        event_seq,
    ));
    let cache = Arc::new(L0Cache::new(config.cache.l0_capacity_entries));
    let query = QueryEngine::new(Arc::clone(&store), Arc::clone(&objects), search, Arc::clone(&cache), Arc::clone(&ingest), query_config(&config));
    let (clients, new_admin_key) = crate::clients::ensure_clients(&paths.clients_toml())?;
    let dev_key = std::env::var(&config.server.api_key_env).ok().filter(|k| !k.is_empty());
    let api_config = ApiConfig { tenant: LOCAL_TENANT.into(), version: env!("CARGO_PKG_VERSION").into() };
    let engine = Arc::new(Engine::new(EngineParts { store, objects, ingest, query, cache, clients, dev_key, config: api_config }));
    recover_jobs(&engine, rebuild_needed)?;
    run_migrations(&engine);
    let shutdown = Shutdown::new();
    let handler_ingest = Arc::clone(&engine.ingest);
    let handler: zcie_runtime::JobHandler = Arc::new(move |job| handler_ingest.handle_job(job));
    let workers = WorkerPool::start(config.ingest.worker_count, queue, shutdown.clone(), handler)?;
    let watchers = Arc::new(Watchers::new(Arc::clone(&engine.ingest), Duration::from_millis(config.ingest.debounce_ms)));
    watchers.attach(&engine);
    watchers.start_all_and_reconcile(&engine)?;
    let maintenance = Maintenance::start(Arc::clone(&engine), shutdown.clone());
    Ok(RunningEngine { engine, new_admin_key, paths, workers, search_writer, shutdown, watchers, maintenance })
}

fn open_search(paths: &InstancePaths) -> Result<(SearchIndex, bool)> {
    match SearchIndex::open(&paths.search()) {
        Ok(index) => Ok((index, false)),
        Err(error) => {
            tracing::warn!(%error, "search index is damaged; rebuilding");
            std::fs::remove_dir_all(paths.search())?;
            std::fs::create_dir_all(paths.search())?;
            Ok((SearchIndex::open(&paths.search())?, true))
        }
    }
}

fn ingest_config(config: &EngineConfig, paths: &InstancePaths) -> Result<IngestConfig> {
    Ok(IngestConfig {
        tenant: LOCAL_TENANT.into(),
        max_payload_bytes: config.ingest.max_payload_bytes,
        sync_writes: config.storage.sync_writes,
        assistant_messages: config.capture.assistant_messages,
        command_output_max_bytes: config.capture.command_output_max_bytes,
        redact_secrets: config.security.redact_secrets,
        max_file_bytes: config.index.max_file_bytes,
        languages: config.index.languages.clone(),
        dead_letter_dir: paths.dead_letter(),
        exclude: ExcludeSet::with_defaults(&config.security.exclude_globs)?,
    })
}

fn query_config(config: &EngineConfig) -> QueryConfig {
    QueryConfig {
        tenant: LOCAL_TENANT.into(),
        default_budget: config.query.default_token_budget,
        max_budget: config.query.max_token_budget,
        max_results: config.query.max_results,
        graph_max_depth: config.query.graph_max_depth,
        graph_max_fanout: config.query.graph_max_fanout,
        negative_ttl: Duration::from_millis(config.cache.negative_ttl_ms),
    }
}

/// Runs one-time data migrations. A failure is logged; the engine still starts.
fn run_migrations(engine: &Engine) {
    match zcie_memory::backfill_decision_index(&engine.store, LOCAL_TENANT) {
        Ok(0) => {}
        Ok(count) => tracing::info!(count, "decision index filled"),
        Err(error) => tracing::warn!(%error, "decision index backfill failed"),
    }
}

/// Puts stored pending jobs into the queue (step 7) and starts a search rebuild if needed (steps 8 and 9).
fn recover_jobs(engine: &Engine, rebuild_needed: bool) -> Result<()> {
    let jobs = pending_jobs(&engine.store, LOCAL_TENANT)?;
    tracing::info!(pending = jobs.len(), "recovering pending jobs");
    for job in jobs {
        if engine.ingest.queue.push(job).is_err() {
            break;
        }
    }
    let graph = read_watermark(&engine.store, LOCAL_TENANT, GRAPH)?;
    let search = read_watermark(&engine.store, LOCAL_TENANT, SEARCH)?;
    if rebuild_needed || search < graph {
        tracing::info!(search, graph, "search index is behind; rebuilding");
        let scope = Scope { tenant_id: LOCAL_TENANT.into(), ..Scope::default() };
        let job = JobRecord::new(JobKind::UpdateSearch, scope, JobPriority::Interactive, engine.ingest.event_seq(), "all".into());
        engine.ingest.enqueue_job(job)?;
    }
    engine.ingest.advance_graph_watermark()?;
    Ok(())
}

impl RunningEngine {
    /// Stops watchers and workers, commits the search index and flushes the store.
    pub fn shutdown(self) {
        self.watchers.stop_all();
        self.shutdown.trigger();
        self.engine.ingest.queue.close();
        self.workers.join();
        self.maintenance.join();
        self.search_writer.stop();
        if let Err(error) = self.engine.store.flush() {
            tracing::error!(%error, "store flush failed");
        }
        tracing::info!("engine stopped");
    }
}

