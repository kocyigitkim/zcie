# 03 — Architecture

Status: Normative.

> **Note:** Implementation decisions of milestone 1 are in `docs/adr/ADR-008-implementation-decisions.md`. Where they differ from this file, ADR-008 applies.

## 1. Process model

ZCIE has three kinds of processes.

| Process | Binary | Source folder | Function |
|---------|--------|---------------|----------|
| Daemon | `zcied` | `apps/zcied` | Owns the database. Runs the HTTP gateway and all workers. Single writer. |
| CLI | `zcie` | `apps/zcie` | Management commands. Sends HTTP requests to the daemon. |
| MCP gateway | `zcie-mcp` | `gateways/mcp` | MCP stdio server. Changes MCP tool calls into HTTP requests to the daemon. |

Rules:

- Only `zcied` opens RocksDB, Tantivy and the object store (F-02).
- At startup, `zcied` takes an exclusive lock on `~/.zcie/run/daemon.lock`. If the lock is held, `zcied` SHALL stop with an error.
- The CLI, the MCP gateway, the SDKs and the adapters SHALL use HTTP only (F-03).
- The HTTP gateway is a library crate. `zcied` links it and serves it on `127.0.0.1:47831`.

```
+------------------+  +-------------+  +-----------------+  +-----------------+
| Claude Code hook |  | zcie (CLI)  |  | Custom agent    |  | Claude Code /   |
| (adapter)        |  |             |  | (Rust SDK)      |  | other MCP host  |
+--------+---------+  +------+------+  +--------+--------+  +--------+--------+
         |                   |                  |                    | stdio
         |                   |                  |           +--------v--------+
         |                   |                  |           | zcie-mcp        |
         |                   |                  |           +--------+--------+
         +-------------------+---------+--------+--------------------+
                                       | HTTP/JSON 127.0.0.1:47831
                          +------------v-------------------------------+
                          | zcied (daemon)                             |
                          |  gateways/http  ->  zcie-api               |
                          |  zcie-security (auth, scope)               |
                          |  zcie-query / zcie-context (read path)     |
                          |  zcie-ingest (write path, workers)         |
                          |  zcie-store (RocksDB + object store)       |
                          |  zcie-search (Tantivy, derived)            |
                          +--------------------------------------------+
                                       |
                                ~/.zcie/instances/local/
```

## 2. Crates

### 2.1 Crate list

| Crate | Folder | Milestone 1 | Responsibility |
|-------|--------|-------------|----------------|
| `zcie-types` | `crates/zcie-types` | Yes | All shared types from `docs/05-data-model.md`. No I/O. |
| `zcie-telemetry` | `crates/zcie-telemetry` | Yes | Log setup, timers, counters. |
| `zcie-security` | `crates/zcie-security` | Yes | API key check, scope check, secret redaction, exclusion globs. |
| `zcie-store` | `crates/zcie-store` | Yes | RocksDB CFs, key helper, object store, write batches, watermarks, recovery. |
| `zcie-runtime` | `crates/zcie-runtime` | Yes | Bounded job queue, worker pool, shutdown signal. |
| `zcie-fs` | `crates/zcie-fs` | Yes | File discovery, file watcher, BLAKE3 hash, binary detection. |
| `zcie-git` | `crates/zcie-git` | Yes | Calls the `git` program. Reads HEAD, commits, changed paths, trailers. |
| `zcie-parse` | `crates/zcie-parse` | Yes | Tree-sitter parser per language. Extracts symbols and imports. |
| `zcie-symbols` | `crates/zcie-symbols` | Yes | Stable symbol IDs. Writes symbol records. |
| `zcie-graph` | `crates/zcie-graph` | Yes | Writes and reads edges. Bounded traversal. Edge status rules. |
| `zcie-memory` | `crates/zcie-memory` | Yes | Raw turns, Tier A extraction, memory records, tasks, corrections. |
| `zcie-search` | `crates/zcie-search` | Yes | Tantivy index for code, documents and conversations. |
| `zcie-cache` | `crates/zcie-cache` | Yes | L0 LRU cache with scope- and generation-keyed entries. |
| `zcie-ingest` | `crates/zcie-ingest` | Yes | Capture path, idempotency, job dispatch to processors. |
| `zcie-query` | `crates/zcie-query` | Yes | Query classification and the retrieval route. |
| `zcie-context` | `crates/zcie-context` | Yes | Capsules, refs, token budget (token firewall). |
| `zcie-api` | `crates/zcie-api` | Yes | The `Engine` struct with the core operations. No transport. |
| `zcie-core` | `crates/zcie-core` | Yes | Loads configuration, opens the store, starts workers, builds `Engine`. |
| `zcie-gateway-http` | `gateways/http` | Yes | Axum routes that call `zcie-api`. |
| `zcie-mcp` | `gateways/mcp` | Yes | Binary. MCP stdio server with two tools. |
| `zcie-sdk` | `sdk/rust` | Yes | Rust HTTP client. |
| `zcie-lsp` | `crates/zcie-lsp` | OPTIONAL | LSP enrichment. Folder only. |
| `zcie-vector` | `crates/zcie-vector` | OPTIONAL | `VectorIndex` trait only. Folder only. |
| gRPC gateway | `gateways/grpc` | OPTIONAL | Folder only. |
| IPC gateway | `gateways/ipc` | OPTIONAL | Folder only. |
| `zcie-inspect` | `apps/zcie-inspect` | OPTIONAL | Folder only. |

Do not create a `Cargo.toml` for an OPTIONAL crate in milestone 1.

### 2.2 Dependency direction

A crate MAY depend only on crates in a lower level. A crate SHALL NOT depend on a crate in the same level, except where the table says so.

| Level | Crates | May depend on |
|-------|--------|---------------|
| 0 | `zcie-types` | none |
| 1 | `zcie-telemetry`, `zcie-security` | level 0 |
| 2 | `zcie-store`, `zcie-runtime` | levels 0–1 |
| 3 | `zcie-fs`, `zcie-git`, `zcie-parse`, `zcie-cache` | levels 0–2 |
| 4 | `zcie-symbols`, `zcie-graph`, `zcie-search` | levels 0–3 |
| 5 | `zcie-memory` | levels 0–4 |
| 6 | `zcie-ingest` | levels 0–5 |
| 7 | `zcie-query`, `zcie-context` | levels 0–6 (`zcie-query` MAY use `zcie-context`) |
| 8 | `zcie-api` | levels 0–7 |
| 9 | `zcie-core` | levels 0–8 |
| 10 | `zcie-gateway-http`, `apps/zcied` | levels 0–9 |
| — | `zcie-mcp`, `apps/zcie`, `zcie-sdk` | `zcie-types` and `zcie-sdk` only |

Rules:

- Gateways and adapters SHALL NOT access a CF directly. They call `zcie-api` or HTTP.
- `zcie-mcp` and `apps/zcie` use `zcie-sdk` for HTTP calls.
- Circular dependencies are not permitted. Cargo rejects them.

### 2.3 Main public items

| Crate | Public items |
|-------|--------------|
| `zcie-store` | `Store::open`, `Store::write_batch`, `Store::get`, `Store::prefix_scan`, `Store::snapshot`, `key()`, `seq_part()`, `ObjectStore::put`, `ObjectStore::get`, `Watermarks` |
| `zcie-runtime` | `JobQueue`, `WorkerPool`, `Shutdown` |
| `zcie-fs` | `discover_files`, `hash_file`, `is_binary`, `FileWatcher` |
| `zcie-git` | `head_commit`, `list_tracked_files`, `commit_info`, `commits_since`, `parse_trailers` |
| `zcie-parse` | `ParserPool`, `parse_file -> ParsedFile { symbols, imports, quality }` |
| `zcie-symbols` | `SymbolIndex::upsert_file_symbols`, `SymbolIndex::find_by_name` |
| `zcie-graph` | `Graph::add_edge`, `Graph::edges_from`, `Graph::edges_to`, `Graph::traverse` |
| `zcie-memory` | `MemoryStore::extract_tier_a`, `MemoryStore::correct`, `MemoryStore::timeline` |
| `zcie-search` | `SearchIndex::open`, `index_documents`, `commit`, `search`, `rebuild` |
| `zcie-cache` | `L0Cache::get`, `L0Cache::put`, `L0Cache::invalidate_scope` |
| `zcie-ingest` | `Ingest::capture`, `Ingest::run_job` |
| `zcie-query` | `QueryPlanner::classify`, `QueryPlanner::run` |
| `zcie-context` | `ContextCompiler::build_capsule`, `estimate_tokens` |
| `zcie-api` | `Engine` (operations in `docs/10-api-and-gateways.md`) |
| `zcie-core` | `start_engine(config) -> Engine` |

## 3. Write path

```
client event
  -> gateway (HTTP)                  : parse JSON, auth (zcie-security)
  -> Engine::capture_event           : scope check, size check, redaction
  -> Ingest::capture                 : idempotency lookup
  -> ObjectStore::put                : write blob, fsync
  -> Store::write_batch (sync)       : event + idempotency key + seq + job
  -> return receipt { event_id, seq, durable: true, indexed: false }
  -> JobQueue (in memory, bounded)   : job ID
  -> worker                          : run processor for the job type
  -> processor writes derived records: memory, task, edges, files, symbols
  -> canonical watermark update
  -> search writer task              : Tantivy add + commit
  -> search watermark update
  -> L0Cache::invalidate_scope
```

## 4. Read path

```
query request
  -> gateway (HTTP / MCP)            : auth, scope
  -> QueryPlanner::classify          : query class
  -> L0Cache::get                    : hit -> return
  -> exact lookup (lookup CF)        : found -> build capsule -> return
  -> SearchIndex::search             : if the class needs text search
  -> Graph::traverse                 : if the class needs relations
  -> freshness check (watermarks + live overlay)
  -> ContextCompiler::build_capsule  : token budget, refs, citations
  -> L0Cache::put
  -> response
```

## 5. Runtime tasks

| Task | Count | Function |
|------|-------|----------|
| Tokio runtime | 1 (multi-thread) | Runs all async tasks. |
| HTTP server | 1 | Serves `gateways/http`. |
| Ingest workers | `ingest.worker_count` (default 4) | Run jobs from `JobQueue`. Parsing runs in `spawn_blocking`. |
| Search writer | 1 | Owns the only Tantivy `IndexWriter`. Receives documents through a bounded channel. Commits every 1 second or every 1000 documents. |
| File watcher | 1 per worktree | Sends debounced file change events to the capture path. |
| Recovery | 1 at startup | Runs before the HTTP server starts. See `docs/06-storage.md` section 8. |

Rules:

- RocksDB calls are blocking. Call them inside `tokio::task::spawn_blocking` when they can be slow (scans, batches).
- Interactive queries SHALL NOT wait for the job queue.
- Shutdown: stop the HTTP server, stop the watcher, finish running jobs, commit Tantivy, flush RocksDB, remove the lock.
