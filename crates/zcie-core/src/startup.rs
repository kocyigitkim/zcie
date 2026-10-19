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
use zcie_store::recovery::{first_pending_jobs, read_event_seq};
use zcie_store::watermarks::{GRAPH, SEARCH, read_watermark};
use zcie_store::{ObjectStore, Store};
use zcie_telemetry::Counters;
use zcie_types::{JobKind, JobPriority, JobRecord, LOCAL_TENANT, Scope};
use zcie_vector::Vectors;

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
    let vectors = load_vectors(&config, &paths);
    let search_writer = SearchWriter::start(&search, Arc::clone(&store), LOCAL_TENANT, vectors.clone())?;
    let queue = Arc::new(JobQueue::new(config.ingest.queue_capacity));
    let counters = Arc::new(Counters::default());
    let _ = counters.activity.set(zcie_telemetry::ActivityLog::new(&paths.home.join("stats").join("activity.jsonl")));
    let ingest = Arc::new(Ingest::new(
        Arc::clone(&store),
        Arc::clone(&objects),
        Arc::clone(&queue),
        search_writer.sender(),
        counters,
        ingest_config(&config, &paths)?,
        event_seq,
    ));
    let cache = Arc::new(L0Cache::new(config.cache.l0_capacity_entries));
    let query = QueryEngine::new(Arc::clone(&store), Arc::clone(&objects), search, Arc::clone(&cache), Arc::clone(&ingest), query_config(&config))
        .with_vectors(vectors.clone());
    let (clients, new_admin_key) = crate::clients::ensure_clients(&paths.clients_toml())?;
    let dev_key = std::env::var(&config.server.api_key_env).ok().filter(|k| !k.is_empty());
    let api_config = ApiConfig { tenant: LOCAL_TENANT.into(), version: env!("CARGO_PKG_VERSION").into() };
    let engine = Arc::new(Engine::new(EngineParts { store, objects, ingest, query, cache, clients, dev_key, config: api_config }));
    let migrated = run_migrations(&engine);
    let rebuilding = recover_jobs(&engine, rebuild_needed || migrated)?;
    let shutdown = Shutdown::new();
    let (workers, watchers, maintenance) = start_background(&engine, &config, queue, &shutdown)?;
    // A search rebuild sends every document through the writer, which also fills the vectors.
    if let Some(vectors) = vectors.filter(|_| !rebuilding) {
        fill_vectors(Arc::clone(&engine.ingest), vectors);
    }
    Ok(RunningEngine { engine, new_admin_key, paths, workers, search_writer, shutdown, watchers, maintenance })
}

/// Starts the job workers, the file watchers and the maintenance thread.
fn start_background(engine: &Arc<Engine>, config: &EngineConfig, queue: Arc<JobQueue>, shutdown: &Shutdown) -> Result<(WorkerPool, Arc<Watchers>, Maintenance)> {
    let handler_ingest = Arc::clone(&engine.ingest);
    let handler: zcie_runtime::JobHandler = Arc::new(move |job| handler_ingest.handle_job(job));
    let workers = WorkerPool::start(config.ingest.worker_count, queue, shutdown.clone(), handler)?;
    let watchers = Arc::new(Watchers::new(Arc::clone(&engine.ingest), Duration::from_millis(config.ingest.debounce_ms)));
    watchers.attach(engine);
    start_watchers(engine, &watchers);
    let maintenance = Maintenance::start(Arc::clone(engine), shutdown.clone());
    Ok((workers, watchers, maintenance))
}

/// A watcher start slower than this probably waits for a macOS privacy prompt.
const SLOW_WATCH_START: Duration = Duration::from_secs(20);

/// Starts the watchers of the indexed folders in a thread: the first access of a launchd daemon
/// to a protected folder (Desktop, Documents) can wait for the macOS privacy check, and the HTTP
/// gateway must not wait for it.
fn start_watchers(engine: &Arc<Engine>, watchers: &Arc<Watchers>) {
    let (engine, watchers) = (Arc::clone(engine), Arc::clone(watchers));
    let spawned = std::thread::Builder::new().name("zcie-watch-start".into()).spawn(move || {
        let started = std::time::Instant::now();
        match watchers.start_all_and_reconcile(&engine) {
            Ok(()) if started.elapsed() > SLOW_WATCH_START => tracing::warn!(
                ms = started.elapsed().as_millis() as u64,
                "file watchers started slowly: macOS may ask for access to a project folder; allow it (System Settings > Privacy & Security > Files and Folders, or Full Disk Access for <home>/bin/zcied)"
            ),
            Ok(()) => tracing::info!(ms = started.elapsed().as_millis() as u64, "file watchers started"),
            Err(error) => tracing::warn!(%error, "file watchers did not start"),
        }
    });
    if let Err(error) = spawned {
        tracing::warn!(%error, "file watcher thread did not start");
    }
}

/// Loads the vector model of `<home>/models/<vector.model>` (or an installed known model for the
/// default) if `vector.enabled` is true.
fn load_vectors(config: &EngineConfig, paths: &InstancePaths) -> Option<Arc<Vectors>> {
    let models = paths.home.join("models");
    let folder = zcie_vector::model_folder(&models, &config.vector.model).filter(|_| config.vector.enabled);
    let Some(folder) = folder else {
        tracing::info!(model = %config.vector.model, "vector search is off (no model or disabled)");
        return None;
    };
    let min_similarity = config.vector.min_similarity.unwrap_or_else(|| zcie_vector::default_min_similarity(&folder));
    match Vectors::load(&folder, min_similarity) {
        Ok(vectors) => {
            tracing::info!(folder = %folder.display(), "vector model loaded");
            Some(vectors)
        }
        Err(error) => {
            tracing::warn!(%error, "vector model cannot be loaded; vector search is off");
            None
        }
    }
}

/// Fills the vector index from the store in the background. The writer keeps it current after that.
fn fill_vectors(ingest: Arc<Ingest>, vectors: Arc<Vectors>) {
    let spawned = std::thread::Builder::new().name("zcie-vector-fill".into()).spawn(move || {
        let started = std::time::Instant::now();
        match ingest.fill_vectors(&vectors) {
            Ok(()) => tracing::info!(vectors = vectors.index.len(), ms = started.elapsed().as_millis() as u64, "vector index filled"),
            Err(error) => tracing::warn!(%error, "vector fill failed"),
        }
    });
    if let Err(error) = spawned {
        tracing::warn!(%error, "vector fill thread did not start");
    }
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
        context: zcie_query::capsule_rules::ContextOptions {
            capsule_rules: config.context.capsule_rules,
            repo_map_budget: config.context.repo_map_budget,
            graph_rank: config.context.graph_rank,
            compress: config.context.compress,
        },
    }
}

/// Runs one-time data migrations. A failure is logged; the engine still starts.
/// Runs the one-time store migrations. Returns true if the search index must be rebuilt.
fn run_migrations(engine: &Engine) -> bool {
    match zcie_memory::backfill_decision_index(&engine.store, LOCAL_TENANT) {
        Ok(0) => {}
        Ok(count) => tracing::info!(count, "decision index filled"),
        Err(error) => tracing::warn!(%error, "decision index backfill failed"),
    }
    let retracted = migration_count("shell commands retracted", zcie_memory::retract_shell_inputs(&engine.store, LOCAL_TENANT));
    let filled = migration_count("context of follow-up messages filled", zcie_memory::backfill_context(&engine.store, &engine.objects, LOCAL_TENANT));
    retracted + filled > 0 || first_run_of(engine, "context_text_v1")
}

/// True the first time a named index change runs: its search documents need a rebuild once
/// (ADR-017: context text of symbols and passages).
fn first_run_of(engine: &Engine, name: &str) -> bool {
    let key = zcie_store::layout::migration(LOCAL_TENANT, name);
    if engine.store.exists(zcie_store::Cf::Registry, &key).unwrap_or(true) {
        return false;
    }
    let marked = engine.store.put_sync(zcie_store::Cf::Registry, &key, &zcie_store::values::Marker::new());
    tracing::info!(name, "search documents are built again once");
    marked.is_ok()
}

/// Logs the result of one migration. Returns the number of changed records (0 on an error).
fn migration_count(name: &str, result: zcie_memory::Result<u64>) -> u64 {
    match result {
        Ok(0) => 0,
        Ok(count) => {
            tracing::info!(count, "{name}");
            count
        }
        Err(error) => {
            tracing::warn!(%error, "{name}: migration failed");
            0
        }
    }
}

/// Puts stored pending jobs into the queue (step 7) and starts a search rebuild if needed (steps 8 and 9).
/// Removes jobs of deleted projects, queues the first pending jobs and, if needed, a search
/// rebuild. Returns true if a search rebuild is queued.
fn recover_jobs(engine: &Engine, rebuild_needed: bool) -> Result<bool> {
    match engine.drop_orphan_jobs() {
        Ok(removed) => tracing::info!(removed, "jobs of deleted projects removed"),
        Err(error) => tracing::warn!(%error, "job sweep failed"),
    }
    match engine.drop_unindexed_file_jobs() {
        Ok(0) => {}
        Ok(removed) => tracing::info!(removed, "file jobs of folders that are not indexed removed"),
        Err(error) => tracing::warn!(%error, "file job sweep failed"),
    }
    let jobs = first_pending_jobs(&engine.store, LOCAL_TENANT, engine.ingest.queue.capacity())?;
    tracing::info!(pending = jobs.len(), "recovering pending jobs");
    for job in jobs {
        if engine.ingest.queue.push(job).is_err() {
            break;
        }
    }
    let graph = read_watermark(&engine.store, LOCAL_TENANT, GRAPH)?;
    let search = read_watermark(&engine.store, LOCAL_TENANT, SEARCH)?;
    let rebuild = rebuild_needed || search < graph;
    if rebuild {
        tracing::info!(search, graph, "search index is behind; rebuilding");
        let scope = Scope { tenant_id: LOCAL_TENANT.into(), ..Scope::default() };
        let job = JobRecord::new(JobKind::UpdateSearch, scope, JobPriority::Interactive, engine.ingest.event_seq(), "all".into());
        engine.ingest.enqueue_job(job)?;
    }
    engine.ingest.advance_graph_watermark()?;
    Ok(rebuild)
}

impl RunningEngine {
    /// Stops watchers and workers, commits the search index and flushes the store.
    pub fn shutdown(self) {
        self.watchers.stop_all();
        self.shutdown.trigger();
        self.engine.ingest.queue.close();
        let started = std::time::Instant::now();
        self.workers.join();
        self.maintenance.join();
        tracing::info!(ms = started.elapsed().as_millis() as u64, "workers stopped");
        self.search_writer.stop();
        tracing::info!(ms = started.elapsed().as_millis() as u64, "search writer stopped");
        if let Err(error) = self.engine.store.flush() {
            tracing::error!(%error, "store flush failed");
        }
        tracing::info!("engine stopped");
    }
}

