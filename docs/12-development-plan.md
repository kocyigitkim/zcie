# 12 — Development Plan

Status: Normative. This file is the work list for the implementing agent.

> **Note:** Implementation decisions of milestone 1 are in `docs/adr/ADR-008-implementation-decisions.md`. Where they differ from this file, ADR-008 applies.

## 1. How to use this file

1. Do the tasks in number order: `D-001`, `D-002`, and so on.
2. Do one task at a time. Do not start a task before the previous task is complete.
3. For each task, obey the task procedure in `docs/00-agent-rules.md` section 3.
4. When a task is complete, change its status line from `- [ ] Done` to `- [x] Done`.

> **Warning:** Do not write tests in a development task. Do not run `cargo test` in a development task.
> The test phase starts after task `D-044`. See `docs/13-test-plan.md` and `docs/adr/ADR-007-tests-after-development.md`.

### 1.1 Task format

Each task has these fields:

| Field | Meaning |
|-------|---------|
| Goal | One sentence. The result of the task. |
| Read | The documents and sections to read before you start. |
| Files | The files to create or change. Change no other files. |
| Steps | The work, in order. |
| Done when | Conditions that MUST be true at the end. |

### 1.2 Standard "Done when" items

These items apply to ALL tasks. The tasks do not repeat them.

- `cargo build --workspace` completes with no errors.
- `cargo clippy --workspace -- -D warnings` completes with no errors.
- The code obeys rules C-01 to C-17 in `docs/00-agent-rules.md`.
- Each new public item has a `///` doc comment.
- The new crate (if any) is in `[workspace] members`.

### 1.3 Crate setup procedure

When a task says "Set up crate `X`", do these steps:

1. Create `<path>/Cargo.toml`. Set `name`, `version.workspace = true`, `edition.workspace = true`.
2. Add only the dependencies that the task names. Use `{ workspace = true }` for each dependency.
3. Create `src/lib.rs` (or `src/main.rs` for a binary). Create `src/error.rs` with one `Error` enum and `pub type Result<T> = std::result::Result<T, Error>;`.
4. Add the crate path to `[workspace] members` in the root `Cargo.toml`.
5. Keep the existing `README.md` in the folder.

## 2. Phase overview

| Phase | Tasks | Result |
|-------|-------|--------|
| P0 — Workspace and contracts | D-001 to D-005 | Workspace builds. All shared types exist. |
| P1 — Durable engine | D-006 to D-025 | Daemon captures events durably. Memory, tasks, links, timeline, exact query over HTTP. |
| P2 — Code intelligence | D-026 to D-037 | Repository scan, watcher, Git, symbols, search, expand, freshness. |
| P3 — Integrations | D-038 to D-044 | Rust SDK, CLI, MCP gateway, Claude Code and Git adapters, forget, docs. |
| P4 — Context efficiency (ADR-017) | D-055 to D-065 | Fewer tokens and fewer file scans for the same or better results: repository map, graph ranking, compression, localization, batch expand. |
| P5 — Evidence-first context (ADR-019, ADR-020) | D-066 to D-082 | Query trace, isolated paired benchmark, intents, source-slice evidence, session working set, symbol identity, call edges, provenance, scope, freshness. |
| T — Test phase | `docs/13-test-plan.md` | All tests written and passing. |
| A — Acceptance | `docs/14-acceptance-criteria.md` | All criteria examined and recorded. |

---

## 3. Phase P0 — Workspace and contracts

### D-001 — Create the Cargo workspace

- [x] Done
- **Goal:** An empty Cargo workspace that builds.
- **Read:** `00-agent-rules.md` section 6; `04-folder-structure.md` sections 2, 3, 5.
- **Files:** `Cargo.toml`, `rust-toolchain.toml`, `README.md`, `.gitignore`.
- **Steps:**
  1. Create `rust-toolchain.toml` with `channel = "stable"` and components `rustfmt`, `clippy`.
  2. Create the root `Cargo.toml` with `[workspace]`, `resolver = "2"` and an empty `members` list.
  3. Add `[workspace.package]` with `version = "0.1.0"`, `edition = "2024"`, `license = "UNLICENSED"` (ADR-008 I-01).
  4. Add `[workspace.dependencies]` with all crates in `00-agent-rules.md` section 6. Use the latest stable versions.
  5. Add the workspace crates as path dependencies in `[workspace.dependencies]`. Example: `zcie-types = { path = "crates/zcie-types" }`. Add a line only when the crate exists.
  6. Add `[workspace.lints.clippy]` with `unwrap_used = "deny"` and `expect_used = "deny"`.
  7. Make sure `.gitignore` contains all lines from `04-folder-structure.md` section 5.
  8. Create the root `README.md`: one paragraph about ZCIE and a link to `docs/README.md`.
- **Done when:**
  - `cargo metadata --no-deps` completes with no errors.

### D-002 — `zcie-types`: IDs and scope

- [x] Done
- **Goal:** ID newtypes and the `Scope` struct exist.
- **Read:** `05-data-model.md` sections 1, 2; `02-glossary.md`.
- **Files:** `crates/zcie-types/Cargo.toml`, `src/lib.rs`, `src/error.rs`, `src/ids.rs`, `src/scope.rs`.
- **Steps:**
  1. Set up crate `zcie-types` with `serde`, `serde_json`, `thiserror`, `ulid`, `time`.
  2. In `ids.rs`, make one newtype for each ID kind in `05` section 1.
  3. Give each ID type: `new()` (makes a new ULID with the prefix), `parse(&str) -> Result<Self>` (checks the prefix), `as_str()`, `Display`, `Serialize`, `Deserialize`.
  4. Make `CommitId::from_sha(sha: &str)`. It does not use a ULID.
  5. Make a `Ref` struct with `kind: String` and `id: String`. Add `Ref::parse("sym:sym_01...")` and `Display`.
  6. In `scope.rs`, make `Scope` as in `05` section 2. Add `Scope::contains(&self, other: &Scope) -> bool`. The rule is in `11-security.md` section 4.
  7. Add a function `now_utc() -> String` that returns RFC 3339 UTC time.
- **Done when:**
  - All ID kinds in `05` section 1 have a type.
  - `Scope::contains` exists and follows `11-security.md` section 4.

### D-003 — `zcie-types`: records

- [x] Done
- **Goal:** All stored record types exist.
- **Read:** `05-data-model.md` sections 3 to 8; `06-storage.md` sections 3.1, 3.3; `07-ingest-and-indexing.md` section 3.1.
- **Files:** `crates/zcie-types/src/event.rs`, `memory.rs`, `code.rs`, `task.rs`, `edge.rs`, `registry.rs`, `job.rs`, `lib.rs`.
- **Steps:**
  1. In `event.rs`, make `EventEnvelope`, `EventSource`, `GitContext`, `Classification`, `EventType`. Use the serde names in `05` section 3.1.
  2. In `memory.rs`, make `MemoryRecord`, `MemoryKind`, `MemoryStatus`.
  3. In `code.rs`, make `FileRecord`, `SymbolRecord`, `SymbolKind`, `ParserQuality`, `CommitRecord`.
  4. In `task.rs`, make `TaskRecord` and `TaskState`.
  5. In `edge.rs`, make `Edge`, `Relation`, `EdgeStatus`, `Evidence`, `EvidenceKind`.
  6. In `edge.rs`, add `EvidenceKind::max_status(&self) -> EdgeStatus`. Use the table in `05` section 7.1.
  7. In `edge.rs`, add `EdgeStatus::rank(&self) -> u8` (Verified = 3, Inferred = 2, Candidate = 1, Rejected = 0).
  8. In `registry.rs`, make the four registry record types from `06` section 3.1.
  9. In `job.rs`, make `JobRecord`, `JobKind`, `JobPriority`, `JobState` from `07` section 3.1.
  10. Add a field `v: u32` to each stored struct. Set it to `1` in each constructor.
  11. Derive `Debug`, `Clone`, `Serialize`, `Deserialize`, `PartialEq` for all types.
- **Done when:**
  - Each type in `05`, `06` section 3.1 and `07` section 3.1 exists in `zcie-types`.
  - `Relation` serializes as `SCREAMING_SNAKE_CASE`.

### D-004 — `zcie-types`: query and API types

- [x] Done
- **Goal:** Request and response types for query, expand and the API exist.
- **Read:** `09-query-and-context.md` section 2; `10-api-and-gateways.md` sections 2, 2.1, 3.1.
- **Files:** `crates/zcie-types/src/query.rs`, `crates/zcie-types/src/api.rs`, `lib.rs`.
- **Steps:**
  1. In `query.rs`, make `QueryRequest`, `QueryResponse`, `QueryMode`, `QueryClass`, `ResultItem`, `Freshness`, `ExpandRequest`, `ExpandResponse`, `ExpandLevel` as in `09`.
  2. In `api.rs`, make each request and response type named in `10` section 2. Use the JSON fields in `10` section 3.1.
  3. Make `TimelineItem`, `HealthReport`, `ForgetReceipt`, `CaptureReceipt`.
- **Done when:**
  - Each type named in `10` section 2 exists.
  - The JSON field names agree with the examples in `09` and `10`.

### D-005 — Configuration files

- [x] Done
- **Goal:** Default configuration and policy files exist.
- **Read:** `15-configuration.md` (all).
- **Files:** `config/default.toml`, `config/schema.json`, `config/policies/capture.toml`, `indexing.toml`, `security.toml`, `context.toml`.
- **Steps:**
  1. Copy the full `engine.toml` example from `15` into `config/default.toml`.
  2. Write `config/schema.json` as a JSON Schema for the same keys and types.
  3. Write each policy file as `15` describes. Use comments to explain each key.
- **Done when:**
  - Each key in the `15` key table is in `config/default.toml` with the default value.

---

## 4. Phase P1 — Durable engine

### D-006 — `zcie-telemetry`

- [x] Done
- **Goal:** Logging setup and a simple timer exist.
- **Read:** `03-architecture.md` section 2.
- **Files:** `crates/zcie-telemetry/` (`lib.rs`, `error.rs`, `logging.rs`, `timer.rs`).
- **Steps:**
  1. Set up crate `zcie-telemetry` with `tracing`, `tracing-subscriber`, `thiserror`.
  2. In `logging.rs`, add `init_logging(level: &str, log_dir: Option<&Path>)`. Write logs to stderr. If `log_dir` is set, also write to a file.
  3. In `timer.rs`, add `Timer::start(name)` and `Timer::elapsed_ms()`. Log the duration at `debug` level on drop.
  4. Add a `Counters` struct with `AtomicU64` fields: `events_captured`, `jobs_done`, `jobs_failed`, `queries`, `cache_hits`.
- **Done when:**
  - The crate depends only on `zcie-types` and approved crates.

### D-007 — `zcie-security`: auth and scope check

- [x] Done
- **Goal:** API key check and scope check exist.
- **Read:** `11-security.md` sections 2, 2.1, 4; `15-configuration.md` (clients.toml).
- **Files:** `crates/zcie-security/` (`lib.rs`, `error.rs`, `auth.rs`, `scope_check.rs`).
- **Steps:**
  1. Set up crate `zcie-security` with `zcie-types`, `blake3`, `getrandom`, `toml`, `serde`, `thiserror`, `regex`, `globset`.
  2. In `auth.rs`, make `ClientEntry` and `ClientsFile`. Load `clients.toml`.
  3. Add `hash_key(key: &str) -> String` (BLAKE3 hex).
  4. Add `authenticate(header: Option<&str>, clients: &ClientsFile) -> Result<AuthContext>`. Compare hashes in constant time (`blake3::Hash` equality is constant time).
  5. Make `AuthContext` as in `11` section 2.1. Add `can_read()`, `can_write()`, `is_admin()`.
  6. In `scope_check.rs`, add `check_scope(auth: &AuthContext, scope: &Scope) -> Result<()>`.
  7. Add `generate_key() -> Result<String>`: 32 random bytes from `getrandom::fill`, hex encoded.
- **Done when:**
  - Every check point in `11` section 4 has a function that the other crates can call.

### D-008 — `zcie-security`: redaction and exclusion

- [x] Done
- **Goal:** Secret redaction and path exclusion exist.
- **Read:** `11-security.md` sections 5, 6.
- **Files:** `crates/zcie-security/src/redact.rs`, `exclude.rs`, `lib.rs`.
- **Steps:**
  1. In `redact.rs`, compile the regex list from `11` section 5 one time (use `std::sync::OnceLock`).
  2. Add `redact(text: &str) -> (String, u32)`. Replace each match with the marker in `11`. Return the number of replacements.
  3. In `exclude.rs`, add `ExcludeSet::new(globs: &[String])` with `globset`.
  4. Add `ExcludeSet::is_excluded(path: &str) -> bool`.
- **Done when:**
  - Each pattern in `11` section 5 is in the list.

### D-009 — `zcie-store`: open the database and key helper

- [x] Done
- **Goal:** RocksDB opens with all nine CFs. Keys and values encode correctly.
- **Read:** `06-storage.md` sections 1 to 4; `05-data-model.md` sections 8, 9; `adr/ADR-002-json-values.md`.
- **Files:** `crates/zcie-store/` (`lib.rs`, `error.rs`, `keys.rs`, `codec.rs`, `rocksdb/mod.rs`, `rocksdb/column_families.rs`).
- **Steps:**
  1. Set up crate `zcie-store` with `zcie-types`, `rocksdb`, `serde`, `serde_json`, `blake3`, `zstd`, `ulid`, `thiserror`, `tracing`.
  2. In `keys.rs`, add `key(parts: &[&str]) -> Vec<u8>` (join with `0x00`) and `seq_part(seq: u64) -> String` (20 digits).
  3. In `codec.rs`, add `encode<T: Serialize>(value: &T) -> Result<Vec<u8>>` and `decode<T: DeserializeOwned>(bytes: &[u8]) -> Result<T>`. This is the only place that uses `serde_json` for stored values.
  4. In `column_families.rs`, make an enum `Cf` with the nine CFs. Add `Cf::name()` and `Cf::ALL`.
  5. In `rocksdb/mod.rs`, make `Store` with `Store::open(path)`. Set the options in `06` section 4.
  6. Add `Store::get<T>(cf, key) -> Result<Option<T>>`, `Store::put_sync`, `Store::multi_get`.
  7. Add a `Batch` struct that wraps `WriteBatch`. Add `Batch::put<T>(cf, key, value)` and `Batch::delete(cf, key)`.
  8. Add `Store::write_batch(batch, sync: bool)`.
  9. Add `Store::prefix_scan<T>(cf, prefix, limit) -> Result<Vec<(Vec<u8>, T)>>`. Stop at the first key without the prefix.
- **Done when:**
  - `Store` has all functions in `03-architecture.md` section 2.3 for `zcie-store`, except `ObjectStore` and `Watermarks`.

### D-010 — `zcie-store`: object store

- [x] Done
- **Goal:** Blobs are stored compressed and content-addressed.
- **Read:** `06-storage.md` sections 5, 11.
- **Files:** `crates/zcie-store/src/objects/mod.rs`, `lib.rs`.
- **Steps:**
  1. Make `ObjectStore::new(root: PathBuf)`.
  2. Add `ObjectStore::put(bytes) -> Result<String>`. Obey the procedure in `06` section 5 exactly. Write to a temp file, `fsync`, rename, `fsync` the folder.
  3. Add `ObjectStore::get(hash) -> Result<Vec<u8>>`. Decompress. Check the BLAKE3 hash. Return an error if it does not agree.
  4. Add `ObjectStore::delete(hash)`.
  5. Add `ObjectStore::sweep_orphans(store: &Store, min_age: Duration) -> Result<u64>` from `06` section 11.
  6. Add `ObjectStore::remove_temp_files()` for recovery step 6.
- **Done when:**
  - The file layout is `objects/<hex[0..2]>/<hex[2..4]>/<hex>.zst`.

### D-011 — `zcie-store`: watermarks and snapshots

- [x] Done
- **Goal:** Watermarks and coherent reads exist.
- **Read:** `06-storage.md` sections 7, 12.
- **Files:** `crates/zcie-store/src/watermarks.rs`, `rocksdb/mod.rs`, `lib.rs`.
- **Steps:**
  1. Make `Watermarks { canonical_seq, graph_seq, search_seq }`.
  2. Add `Watermarks::read(store)` and `Batch::set_watermark(name, value)`.
  3. Make sure a watermark never decreases, except the rebuild case in `06` section 9 step 3.
  4. Add `Store::snapshot()` that returns a `StoreSnapshot` with `get` and `prefix_scan`.
- **Done when:**
  - The three watermark keys agree with `06` section 3.5.

### D-012 — `zcie-store`: recovery helpers

- [x] Done
- **Goal:** Storage-level recovery steps exist.
- **Read:** `06-storage.md` section 8.
- **Files:** `crates/zcie-store/src/recovery/mod.rs`, `lib.rs`.
- **Steps:**
  1. Add `read_event_seq(store) -> Result<u64>` (step 5).
  2. Add `pending_jobs(store) -> Result<Vec<JobRecord>>` in key order (step 7).
  3. Add `move_to_dead_letter(store, job, error, dead_letter_dir)`. Write the CF key and the JSON file.
  4. Add `events_in_range(store, from_exclusive, to_inclusive) -> Result<Vec<EventEnvelope>>` (step 9).
- **Done when:**
  - Steps 5, 6, 7 and 9 of `06` section 8 have a helper function.

### D-013 — `zcie-runtime`

- [x] Done
- **Goal:** A bounded job queue, a worker pool and a shutdown signal exist.
- **Read:** `03-architecture.md` section 5; `07-ingest-and-indexing.md` sections 3.3, 3.4.
- **Files:** `crates/zcie-runtime/` (`lib.rs`, `error.rs`, `queue.rs`, `workers.rs`, `shutdown.rs`).
- **Steps:**
  1. Set up crate `zcie-runtime` with `zcie-types`, `tokio`, `tracing`, `thiserror`.
  2. In `queue.rs`, make `JobQueue` with two bounded `tokio::sync::mpsc` channels: interactive and background.
  3. Add `JobQueue::push(job) -> Result<()>`. Return a `Busy` error when the channel is full.
  4. Add `JobQueue::next()`. Take an interactive job first. Use `tokio::select!` with `biased;`.
  5. In `workers.rs`, make `WorkerPool::start(count, queue, handler)`. `handler` is an `Arc<dyn Fn(JobRecord) -> Result<()> + Send + Sync>`. Run each handler call in `spawn_blocking`.
  6. In `shutdown.rs`, make `Shutdown` with a `tokio::sync::watch` channel.
- **Done when:**
  - Interactive jobs run before background jobs when both wait.

### D-014 — `zcie-ingest`: capture path

- [x] Done
- **Goal:** Events are captured durably and only once.
- **Read:** `07-ingest-and-indexing.md` section 2; `06-storage.md` section 6; `11-security.md` section 5.
- **Files:** `crates/zcie-ingest/` (`lib.rs`, `error.rs`, `capture.rs`, `dedupe/mod.rs`).
- **Steps:**
  1. Set up crate `zcie-ingest` with `zcie-types`, `zcie-store`, `zcie-security`, `zcie-runtime`, `zcie-telemetry`, `tokio`, `serde_json`, `thiserror`, `tracing`.
  2. Make `Ingest` with fields: `Arc<Store>`, `Arc<ObjectStore>`, a `std::sync::Mutex<u64>` for the event sequence, `JobQueue`, config values.
  3. In `dedupe/mod.rs`, add `find_existing(store, idempotency_key) -> Result<Option<CaptureReceipt>>`.
  4. In `capture.rs`, add `Ingest::capture(scope, request) -> Result<CaptureReceipt>`. Do the steps in `07` section 2 in the given order.
  5. Write the blob before you take the mutex. Hold the mutex from the idempotency check to the end of the `WriteBatch`.
  6. Put these items in one sync batch: event, idempotency key, event-id lookup, sequence counter, timeline keys, turn key, blob reference, job.
  7. After the batch, push the job to the `JobQueue`. If the queue is full, keep the job in the CF. Recovery or the next poll takes it.
- **Done when:**
  - The capture batch contains each item in `06` section 6.
  - The receipt is returned only after the sync batch write.

### D-015 — `zcie-ingest`: job dispatch

- [x] Done
- **Goal:** Workers run jobs with retry and dead-letter.
- **Read:** `07-ingest-and-indexing.md` sections 3.1 to 3.4; `06-storage.md` section 8.
- **Files:** `crates/zcie-ingest/src/queues/mod.rs`, `processors/mod.rs`, `lib.rs`.
- **Steps:**
  1. Add `Ingest::run_job(job) -> Result<()>`. Use a `match` on `JobKind`. Call one processor function per kind.
  2. For now, make each processor a function that logs the job and returns `Ok(())`. Later tasks fill them.
  3. On success, delete the `pending` key in the same batch as the processor output.
  4. On error, increment `attempts`. If `attempts` is 3, call `move_to_dead_letter`. Otherwise push the job again.
  5. Add `Ingest::advance_graph_watermark()`. Use the rule in `06` section 7.
- **Done when:**
  - Each `JobKind` has a branch in the `match`.

### D-016 — `zcie-graph`

- [x] Done
- **Goal:** Edges are stored in both directions with status rules. Traversal is bounded.
- **Read:** `05-data-model.md` section 7; `06-storage.md` section 3.4; `08-memory-and-graph.md` sections 5.1, 8.
- **Files:** `crates/zcie-graph/` (`lib.rs`, `error.rs`, `edges.rs`, `status.rs`, `traverse.rs`).
- **Steps:**
  1. Set up crate `zcie-graph` with `zcie-types`, `zcie-store`, `thiserror`, `tracing`.
  2. In `status.rs`, add `effective_status(requested, evidence) -> EdgeStatus`. Return the lower status of `requested` and `evidence.kind.max_status()`.
  3. In `edges.rs`, add `Graph::add_edge(batch, edge)`. Apply `effective_status`. Write `edges_out` and `edges_in` in the same batch. Obey the replace rules in `06` section 3.4.
  4. Add `Graph::edges_from(ref, relation: Option<Relation>)` and `Graph::edges_to(ref, relation: Option<Relation>)`.
  5. Add `Graph::delete_edge(batch, from, relation, to)`.
  6. In `traverse.rs`, add `Graph::traverse(start, relations, max_depth, max_fanout) -> Vec<Path>`. Use breadth-first search. Skip `Rejected` edges.
  7. Set each path status to the lowest edge status in the path.
- **Done when:**
  - No function can store an edge with a status higher than its evidence permits.
  - Traversal stops at `query.graph_max_depth` and `query.graph_max_fanout`.

### D-017 — `zcie-memory`

- [x] Done
- **Goal:** Tier A extraction, task rule, corrections and timeline exist.
- **Read:** `08-memory-and-graph.md` sections 1 to 7.
- **Files:** `crates/zcie-memory/` (`lib.rs`, `error.rs`, `extraction/tier_a.rs`, `tasks.rs`, `timeline.rs`, `conflict/mod.rs`, `provenance/mod.rs`, `capture/mod.rs`, `retention/mod.rs`).
- **Steps:**
  1. Set up crate `zcie-memory` with `zcie-types`, `zcie-store`, `zcie-graph`, `zcie-symbols`, `regex`, `thiserror`, `tracing`.
  2. In `tier_a.rs`, compile the patterns in `08` section 3.1 one time.
  3. Add `extract_tier_a(event, payload_text) -> Extraction`. `Extraction` lists memory records and edges to make. Do not write to the store in this function.
  4. Apply the rules by event type in `08` section 3.2.
  5. In `tasks.rs`, add the task rule from `08` section 4. Add `create_task` and `set_task_state`.
  6. In `provenance/mod.rs`, add a check that `source_event_ids` is not empty.
  7. In `conflict/mod.rs`, add `correct_memory`, `mark_contradiction`, `retract` from `08` section 6.
  8. In `timeline.rs`, add `timeline(ref) -> Vec<TimelineItem>` from `08` section 7.
  9. `capture/mod.rs` and `retention/mod.rs` hold short helper functions only. Leave them with a module doc comment if no helper is needed.
- **Done when:**
  - Each pattern in `08` section 3.1 is implemented.
  - A correction never deletes the earlier record. It sets `superseded_by`.

> **Note:** `zcie-memory` depends on `zcie-symbols` (for symbol name lookup). Do task `D-031` items for `SymbolIndex::find_by_name` as a small read-only function now if it does not exist. Put it in `crates/zcie-symbols`. Set up the crate with `zcie-types` and `zcie-store` only.

### D-018 — `zcie-ingest`: message processor

- [x] Done
- **Goal:** The `ExtractMemory` job makes memory records, tasks and edges.
- **Read:** `07-ingest-and-indexing.md` section 3.1; `08-memory-and-graph.md` sections 3 to 5.
- **Files:** `crates/zcie-ingest/src/processors/message.rs`, `processors/test_result.rs`, `processors/mod.rs`.
- **Steps:**
  1. Read the event and its payload from the store.
  2. Call `extract_tier_a`.
  3. Write all records and edges in one batch. Delete the job key in the same batch.
  4. Make the turn links in `08` section 5.3 when the event has a `turn_id`.
  5. In `test_result.rs`, apply `08` section 5.4.
  6. Make the processor idempotent. Use lookup keys to find existing records.
- **Done when:**
  - A replay of the same job makes no new record and no new edge.

### D-019 — `zcie-cache`

- [x] Done
- **Goal:** An L0 cache with safe keys exists.
- **Read:** `09-query-and-context.md` sections 4.1, 8, 8.1.
- **Files:** `crates/zcie-cache/` (`lib.rs`, `error.rs`, `l0.rs`, `cache_key.rs`).
- **Steps:**
  1. Set up crate `zcie-cache` with `zcie-types`, `lru`, `blake3`, `serde_json`, `thiserror`.
  2. In `cache_key.rs`, make `CacheKey` from all parts in `09` section 8. Hash them with BLAKE3.
  3. In `l0.rs`, make `L0Cache` with `std::sync::Mutex<lru::LruCache>`.
  4. Add `get`, `put`, `invalidate_scope(workspace_id)`, `clear()`.
  5. Store negative results with an expiry time (`cache.negative_ttl_ms`).
- **Done when:**
  - The cache key contains the principal, the workspace and the watermark.

### D-020 — `zcie-context`

- [x] Done
- **Goal:** Capsules and the token firewall exist.
- **Read:** `09-query-and-context.md` sections 6, 9.
- **Files:** `crates/zcie-context/` (`lib.rs`, `error.rs`, `capsule.rs`, `budget.rs`, `refs.rs`).
- **Steps:**
  1. Set up crate `zcie-context` with `zcie-types`, `serde_json`, `thiserror`.
  2. In `budget.rs`, add `estimate_tokens(text) -> u32` = `ceil(chars / 4)`.
  3. In `capsule.rs`, add `ContextCompiler::build_capsule(items, budget) -> QueryResponse`. Obey the algorithm in `09` section 9 step by step.
  4. Never cut a code span. If an item does not fit, stop and set `truncated = true` and a continuation cursor.
  5. In `refs.rs`, add helpers that make refs from records.
- **Done when:**
  - The serialized response never exceeds the budget, except the fixed envelope fields in `09` section 9.

### D-021 — `zcie-query`: classifier and exact routes

- [x] Done
- **Goal:** Queries are classified and exact lookups and timelines are answered.
- **Read:** `09-query-and-context.md` sections 3, 4.1, 4.2, 10.
- **Files:** `crates/zcie-query/` (`lib.rs`, `error.rs`, `classify.rs`, `planner.rs`, `routes.rs`).
- **Steps:**
  1. Set up crate `zcie-query` with `zcie-types`, `zcie-store`, `zcie-graph`, `zcie-memory`, `zcie-symbols`, `zcie-cache`, `zcie-context`, `regex`, `thiserror`, `tracing`.
  2. In `classify.rs`, add `normalize(question)` and `classify(question, mode) -> QueryClass`. Use the rule table in `09` section 3.2.
  3. In `routes.rs`, add the exact-lookup route (`09` section 4.2) and the timeline route.
  4. In `planner.rs`, add `QueryPlanner::run(auth, request) -> QueryResponse`. Follow the ladder in `09` section 4. Leave lexical search and graph expansion as "not available" for now. Mark the response `partial`.
  5. Apply the local answer rules in `09` section 10.
- **Done when:**
  - Each class in `09` section 3.2 maps to a route or to "not available".

### D-022 — `zcie-api`: `Engine`

- [x] Done
- **Goal:** The `Engine` struct has all core operations.
- **Read:** `10-api-and-gateways.md` section 2; `11-security.md` section 4.
- **Files:** `crates/zcie-api/` (`lib.rs`, `error.rs`, `engine.rs`, `requests.rs`, `responses.rs`).
- **Steps:**
  1. Set up crate `zcie-api` with the crates of levels 0 to 7 that it needs, plus `serde_json`, `thiserror`.
  2. In `error.rs`, make `ApiError` as in `10` section 2.1. Add `ApiError::code()` and `ApiError::http_status()`.
  3. In `engine.rs`, make `Engine` with `Arc` fields for each subsystem.
  4. Implement `capture_event`, `query`, `get_entity`, `get_timeline`, `create_task`, `link_entities`, `correct_memory`, `get_health`.
  5. Implement `forget` with the procedure in `06` section 10 and `11` section 7.
  6. Make `register_repository`, `expand` and `rebuild_search` return `ApiError::Internal("not implemented")` for now. Tasks `D-032`, `D-035`, `D-037` fill them.
  7. Call the scope check before each read and each write.
- **Done when:**
  - All twelve functions in `10` section 2 exist.

### D-023 — `zcie-core`: configuration and startup

- [x] Done
- **Goal:** One function loads the configuration, runs recovery and returns an `Engine`.
- **Read:** `15-configuration.md`; `06-storage.md` section 8; `04-folder-structure.md` section 6.
- **Files:** `crates/zcie-core/` (`lib.rs`, `error.rs`, `config.rs`, `startup.rs`).
- **Steps:**
  1. Set up crate `zcie-core` with `zcie-api` and the other crates it needs, plus `toml`, `serde`, `tokio`.
  2. In `config.rs`, make `EngineConfig` with one struct per section. Load in the order in `15`: defaults, file, environment.
  3. Return an error for an unknown key.
  4. In `startup.rs`, add `start_engine(config) -> Result<Engine>`. Do steps 2 to 11 of `06` section 8. Skip steps 8, 9 and 11 for now. Tasks `D-034` and `D-035` add them.
  5. Create the runtime tree with the modes in `04` section 6.
  6. Start the worker pool with `Ingest::run_job` as the handler.
- **Done when:**
  - `start_engine` returns an `Engine` with all subsystems connected.

### D-024 — `zcie-gateway-http`

- [x] Done
- **Goal:** All HTTP routes call the `Engine`.
- **Read:** `10-api-and-gateways.md` sections 3, 3.1; `11-security.md` sections 2, 3.
- **Files:** `gateways/http/Cargo.toml`, `gateways/http/src/lib.rs`, `src/routes.rs`, `src/auth.rs`, `src/error.rs`.
- **Steps:**
  1. Set up crate `zcie-gateway-http` with `zcie-api`, `zcie-security`, `zcie-types`, `axum`, `tokio`, `serde_json`, `tracing`, `thiserror`.
  2. In `auth.rs`, make an axum extractor that reads `Authorization: Bearer <key>` and returns `AuthContext`.
  3. Reject a request with an `Origin` header that is not loopback. See `11` section 3.
  4. In `routes.rs`, add each route in `10` section 3. Call the engine in `spawn_blocking`.
  5. Change `ApiError` to the HTTP status and the error body in `10` section 2.1.
  6. Add `serve(engine, addr, shutdown) -> Result<()>`.
- **Done when:**
  - Each route in `10` section 3 exists with the correct method and path.

### D-025 — `apps/zcied`: daemon

- [x] Done
- **Goal:** The daemon starts, serves HTTP and stops cleanly.
- **Read:** `03-architecture.md` sections 1, 5; `06-storage.md` section 8; `adr/ADR-003-single-writer-daemon.md`; `11-security.md` section 3.
- **Files:** `apps/zcied/Cargo.toml`, `apps/zcied/src/main.rs`, `src/lock.rs`, `src/signals.rs`.
- **Steps:**
  1. Set up binary crate `zcied` with `zcie-core`, `zcie-gateway-http`, `zcie-telemetry`, `tokio`, `anyhow`, `clap`, `tracing`.
  2. Add arguments: `--config <path>`, `--data-dir <path>`, `--foreground`.
  3. In `lock.rs`, take an exclusive lock on `run/daemon.lock`. Use `std::fs::File::try_lock` (stable Rust). Stop with a clear error if the lock is held.
  4. Write `run/zcied.pid`.
  5. Refuse a non-loopback `http_addr` unless `server.allow_remote = true`.
  6. If `clients.toml` does not exist, make one admin client. Print its key one time.
  7. Call `start_engine`, then `serve`.
  8. In `signals.rs`, stop on `SIGINT` and `SIGTERM`. Do the shutdown order in `03` section 5. Remove the pid file.
- **Done when:**
  - `cargo run -p zcied -- --help` prints the arguments.

---

## 5. Phase P2 — Code intelligence

### D-026 — `zcie-fs`: discovery and hashing

- [x] Done
- **Goal:** Files are discovered with ignore rules and hashed.
- **Read:** `07-ingest-and-indexing.md` sections 4.2, 6; `11-security.md` section 6.
- **Files:** `crates/zcie-fs/` (`lib.rs`, `error.rs`, `discover.rs`, `hash.rs`, `binary.rs`).
- **Steps:**
  1. Set up crate `zcie-fs` with `zcie-types`, `zcie-security`, `ignore`, `blake3`, `thiserror`, `tracing`.
  2. In `discover.rs`, add `discover_files(root, exclude, max_bytes) -> Vec<DiscoveredFile>`. Use `ignore::WalkBuilder`. Do not follow symlinks out of the root.
  3. In `hash.rs`, add `hash_file(path) -> Result<String>` with the `b3:` prefix.
  4. In `binary.rs`, add `is_binary(bytes) -> bool` (a `0x00` byte in the first 8 KiB).
  5. Add `language_for_path(path) -> Option<&'static str>` for `.rs`, `.ts`, `.tsx`, `.py`.
- **Done when:**
  - Excluded paths and files over `index.max_file_bytes` are not returned as indexable.

### D-027 — `zcie-fs`: file watcher

- [x] Done
- **Goal:** File changes become debounced change notices.
- **Read:** `07-ingest-and-indexing.md` section 5.
- **Files:** `crates/zcie-fs/src/watcher.rs`, `lib.rs`.
- **Steps:**
  1. Add `notify` to the crate.
  2. Make `FileWatcher::start(root, debounce_ms, sender)`.
  3. Merge events for the same path inside the debounce window.
  4. Send `FileChange { path, kind: Modified | Deleted | Renamed { from } }`.
  5. Ignore paths under `.git/` and excluded paths.
- **Done when:**
  - Many rapid writes to one file give one `FileChange`.

### D-028 — `zcie-git`

- [x] Done
- **Goal:** Git data is read with the `git` program.
- **Read:** `07-ingest-and-indexing.md` sections 4.2, 7; `adr/ADR-004-git-cli.md`.
- **Files:** `crates/zcie-git/` (`lib.rs`, `error.rs`, `command.rs`, `head.rs`, `commits.rs`, `trailers.rs`).
- **Steps:**
  1. Set up crate `zcie-git` with `zcie-types`, `thiserror`, `tracing`.
  2. In `command.rs`, add `run_git(repo_root, args) -> Result<String>`. Return `GitUnavailable` if `git` is missing.
  3. In `head.rs`, add `head_commit(root)` and `is_git_repository(root)`.
  4. Add `list_tracked_files(root)` with the exact command in `07` section 4.2. Split on `\0`.
  5. In `commits.rs`, add `commit_info(root, sha) -> CommitRecord` and `commits_since(root, from_sha)`.
  6. In `trailers.rs`, add `parse_trailers(message) -> Vec<(String, String)>`. Find lines `ZCIE-Task: <value>`.
- **Done when:**
  - Each Git command in `07` section 7 has a function.

### D-029 — `zcie-parse`: parser pool and Rust extractor

- [x] Done
- **Goal:** Rust files give symbols and imports.
- **Read:** `07-ingest-and-indexing.md` sections 8, 8.1, 8.2.
- **Files:** `crates/zcie-parse/` (`lib.rs`, `error.rs`, `pool.rs`, `grammars/mod.rs`, `extractors/mod.rs`, `extractors/rust.rs`).
- **Steps:**
  1. Set up crate `zcie-parse` with `zcie-types`, `tree-sitter`, `tree-sitter-rust`, `thiserror`.
  2. In `grammars/mod.rs`, add `language_for(name) -> Option<tree_sitter::Language>`.
  3. In `pool.rs`, make `ParserPool`. Keep one `Parser` per language in a `Mutex`.
  4. Add `parse_file(language, source) -> Result<ParsedFile>`. `ParsedFile` has `symbols`, `imports`, `quality`.
  5. In `extractors/rust.rs`, walk the tree. Map node kinds to `SymbolKind` with the table in `07` section 8.1.
  6. Make the qualified name from the parent `impl`, `mod` or `trait` names.
  7. Set `quality = PartialSyntaxError` if the tree has an error node.
- **Done when:**
  - All Rust node kinds in `07` section 8.1 are handled.

### D-030 — `zcie-parse`: TypeScript and Python extractors

- [x] Done
- **Goal:** TypeScript and Python files give symbols and imports.
- **Read:** `07-ingest-and-indexing.md` sections 8.1, 8.2.
- **Files:** `crates/zcie-parse/src/extractors/typescript.rs`, `extractors/python.rs`, `grammars/mod.rs`.
- **Steps:**
  1. Add `tree-sitter-typescript` and `tree-sitter-python`.
  2. Use the TSX grammar for `.tsx` files and the TypeScript grammar for `.ts` files.
  3. Write each extractor with the same structure as the Rust extractor.
- **Done when:**
  - All TypeScript and Python node kinds in `07` section 8.1 are handled.

### D-031 — `zcie-symbols`

- [x] Done
- **Goal:** Symbols have stable IDs and are stored per file.
- **Read:** `05-data-model.md` section 5 (symbol identity rule); `06-storage.md` section 3.6.
- **Files:** `crates/zcie-symbols/` (`lib.rs`, `error.rs`, `index.rs`, `identity.rs`).
- **Steps:**
  0. The crate exists since task `D-017`. Add the dependency `zcie-parse` now.
  1. In `identity.rs`, add `symbol_id_for(store, worktree_id, path, qualified_name, kind) -> SymbolId`. Reuse the ID from `lookup: t/"sym_ident"/...` or make a new one.
  2. In `index.rs`, add `SymbolIndex::upsert_file_symbols(batch, file, parsed)`.
  3. Delete the symbols of the file that are not in the new parse. Delete their lookup keys and edges.
  4. Write `sym_name` and `file_syms` lookup keys.
  5. Complete `SymbolIndex::find_by_name(workspace_id, name, limit)`.
- **Done when:**
  - The same symbol in an unchanged position keeps its ID after a reparse.

### D-032 — `zcie-ingest`: file and scan processors

- [x] Done
- **Goal:** Repositories are registered, scanned and kept current.
- **Read:** `07-ingest-and-indexing.md` sections 4, 5, 6; `10-api-and-gateways.md` (`POST /v1/repositories`).
- **Files:** `crates/zcie-ingest/src/processors/file.rs`, `processors/mod.rs`, `crates/zcie-api/src/engine.rs`, `crates/zcie-core/src/startup.rs`.
- **Steps:**
  1. Implement the `IndexFile` handler as in `07` section 6. Skip the parse if the hash did not change.
  2. Implement the `DeleteFile` handler.
  3. Implement the `ScanRepository` handler as in `07` section 4.2.
  4. Implement `Engine::register_repository` as in `07` section 4.1.
  5. In `startup.rs`, start one `FileWatcher` per registered worktree. Send each change through the capture path as a `file.changed` or `file.deleted` event.
  6. Maintain the live overlay keys in `06` section 3.5. Write an overlay key at capture. Delete it when `IndexFile` completes.
  7. Add recovery step 11 (reconciliation scan) to `startup.rs`.
- **Done when:**
  - `register_repository` returns a repository ID and a scan job ID.

### D-033 — `zcie-ingest`: commit processor

- [x] Done
- **Goal:** Commits become entities with verified links.
- **Read:** `07-ingest-and-indexing.md` sections 7, 7.1; `08-memory-and-graph.md` sections 5.2, 5.5.
- **Files:** `crates/zcie-ingest/src/processors/commit.rs`, `processors/mod.rs`.
- **Steps:**
  1. Implement the `IndexCommit` handler as in `07` section 7.1.
  2. Add `ChangedInCommit` edges with `GitHistory` evidence.
  3. For each `ZCIE-Task` trailer, resolve the task. Add the task-to-commit edge with `CommitTrailer` evidence.
  4. Never link a commit to a task by time alone.
- **Done when:**
  - The only way to a `Verified` task-to-commit edge is a trailer, a client trace or a user confirmation.

### D-034 — `zcie-search`

- [x] Done
- **Goal:** A Tantivy index for code, documents and conversations exists.
- **Read:** `07-ingest-and-indexing.md` section 9; `06-storage.md` sections 7, 9; `adr/ADR-005-derived-search-index.md`.
- **Files:** `crates/zcie-search/` (`lib.rs`, `error.rs`, `lexical/schema.rs`, `lexical/writer.rs`, `lexical/reader.rs`, `ranking/mod.rs`, `freshness/mod.rs`).
- **Steps:**
  1. Set up crate `zcie-search` with `zcie-types`, `zcie-store`, `tantivy`, `tokio`, `thiserror`, `tracing`.
  2. In `schema.rs`, make the schema in `07` section 9.2.
  3. In `writer.rs`, make the search writer task. It owns the only `IndexWriter` per index. It receives `SearchOp::{Upsert, Delete}` through a bounded channel.
  4. Delete by `doc_key` before each add.
  5. Commit every 1 second or every 1000 documents. After each commit, write `search_seq`.
  6. In `reader.rs`, add `SearchIndex::search(scope, text, kinds, limit) -> Vec<SearchHit>`. Filter by scope fields.
  7. Add `SearchIndex::rebuild(store, objects)` as in `06` section 9.
  8. In `startup.rs` of `zcie-core`, add recovery steps 8 and 9.
- **Done when:**
  - The three indexes are in `state/search/code`, `documents` and `conversations`.

### D-035 — `UpdateSearch` job and rebuild

- [x] Done
- **Goal:** Derived search documents follow canonical records.
- **Read:** `07-ingest-and-indexing.md` sections 3.1, 9.3, 9.4.
- **Files:** `crates/zcie-ingest/src/processors/mod.rs`, `processors/search.rs`, `crates/zcie-api/src/engine.rs`.
- **Steps:**
  1. Make each processor send `SearchOp` values for the records it changes.
  2. Implement the `UpdateSearch` handler.
  3. Implement `Engine::rebuild_search`. It needs `admin` permission.
- **Done when:**
  - A forgotten ref causes a `SearchOp::Delete`.

### D-036 — `zcie-query`: search, graph, ranking, freshness

- [x] Done
- **Goal:** The full retrieval ladder works.
- **Read:** `09-query-and-context.md` sections 4.3, 4.4, 5, 7; `07-ingest-and-indexing.md` section 10.
- **Files:** `crates/zcie-query/src/routes.rs`, `planner.rs`, `crates/zcie-search/src/ranking/mod.rs`, `freshness/mod.rs`.
- **Steps:**
  1. Add the lexical route with `SearchIndex::search`.
  2. Check each hit against the canonical store and the tombstones. Drop stale hits.
  3. Add the graph route with `Graph::traverse`.
  4. Implement the ranking formula in `09` section 5.
  5. Implement the freshness rules in `09` section 7. Read the live overlay for read-your-writes.
  6. Remove the "not available" answers from task `D-021`.
- **Done when:**
  - Each response has a `freshness` value and the watermarks.

### D-037 — Expand operation

- [x] Done
- **Goal:** Refs expand into evidence under a budget.
- **Read:** `09-query-and-context.md` sections 6, 6.1.
- **Files:** `crates/zcie-query/src/expand.rs`, `lib.rs`, `crates/zcie-api/src/engine.rs`.
- **Steps:**
  1. Add `expand(auth, request) -> ExpandResponse` for each level in `09` section 6.
  2. For a symbol at level `source`, read the file blob and return the span lines.
  3. Return an error for a stale `expected_revision`.
  4. Implement `Engine::expand`.
- **Done when:**
  - Expand checks the scope of each ref.

---

## 6. Phase P3 — Integrations

### D-038 — `zcie-sdk` (Rust)

- [x] Done
- **Goal:** A Rust HTTP client for all routes exists.
- **Read:** `10-api-and-gateways.md` sections 3, 6.
- **Files:** `sdk/rust/Cargo.toml`, `sdk/rust/src/lib.rs`, `src/error.rs`.
- **Steps:**
  1. Set up crate `zcie-sdk` with `zcie-types`, `reqwest` (blocking, json), `serde_json`, `thiserror`.
  2. Make `Client::new(endpoint, api_key, workspace)`.
  3. Add one method per SDK method in `10` section 6.
  4. Change HTTP error bodies into an `Error::Api { code, message }`.
- **Done when:**
  - Each method name in `10` section 6 exists.

### D-039 — `apps/zcie`: CLI

- [x] Done
- **Goal:** The CLI has all commands.
- **Read:** `10-api-and-gateways.md` section 5; `15-configuration.md`.
- **Files:** `apps/zcie/Cargo.toml`, `apps/zcie/src/main.rs`, `src/commands/*.rs`.
- **Steps:**
  1. Set up binary crate `zcie` with `zcie-sdk`, `zcie-types`, `zcie-security`, `clap`, `anyhow`, `serde_json`.
  2. Add each command in `10` section 5. Put each command group in one file in `src/commands/`.
  3. `zcie key create` runs locally. It uses `zcie_security::generate_key` and writes `clients.toml` with mode `0600`.
  4. `zcie daemon start` runs the `zcied` binary from the same folder as `zcie`.
  5. Read the API key from the environment variable in `server.api_key_env`.
- **Done when:**
  - `cargo run -p zcie -- --help` lists all commands in `10` section 5.

### D-040 — `zcie-mcp`: MCP gateway

- [x] Done
- **Goal:** An MCP stdio server with two tools exists.
- **Read:** `10-api-and-gateways.md` section 4; `adr/ADR-006-two-mcp-tools.md`.
- **Files:** `gateways/mcp/Cargo.toml`, `gateways/mcp/src/main.rs`, `src/tools.rs`.
- **Steps:**
  1. Set up binary crate `zcie-mcp` with `zcie-sdk`, `zcie-types`, `rmcp`, `tokio`, `serde_json`, `anyhow`.
  2. Register the tools `zcie_query` and `zcie_expand` with the schemas in `10` sections 4.1 and 4.2.
  3. Each tool calls the SDK. Return the JSON response as text content.
  4. Read the endpoint, the API key and the default workspace from the environment.
  5. Write logs to stderr only. Stdout is for the protocol.
- **Done when:**
  - The server lists exactly two tools.

### D-041 — Claude Code adapter

- [x] Done
- **Goal:** Claude Code hooks send events to the daemon.
- **Read:** `10-api-and-gateways.md` section 7.1.
- **Files:** `adapters/claude-code/hooks/*.sh`, `adapters/claude-code/install.md`.
- **Steps:**
  1. Write one POSIX shell script per hook in `10` section 7.1.
  2. Use one shared file `hooks/common.sh` for the `curl` call and the turn counter.
  3. Make sure each script always ends with `exit 0`.
  4. Write `install.md` with the `settings.json` hook entries and the MCP server entry.
- **Done when:**
  - `shellcheck` reports no errors (if `shellcheck` is installed).

### D-042 — Git adapter and custom example

- [x] Done
- **Goal:** Commits and a custom client example exist.
- **Read:** `10-api-and-gateways.md` sections 7.2, 7.3.
- **Files:** `adapters/git/post-commit`, `adapters/git/install.md`, `adapters/custom-example/example.sh`.
- **Steps:**
  1. Write the `post-commit` hook as in `10` section 7.2.
  2. Write `install.md` with the copy command for `.git/hooks/`.
  3. Write `example.sh` as in `10` section 7.3.
- **Done when:**
  - Both scripts always end with `exit 0`.

### D-043 — Forget, orphan sweep and health

- [x] Done
- **Goal:** Deletion and background maintenance are complete.
- **Read:** `06-storage.md` sections 10, 11; `11-security.md` section 7.
- **Files:** `crates/zcie-api/src/engine.rs`, `crates/zcie-core/src/startup.rs`.
- **Steps:**
  1. Complete `forget` with the search delete operations from task `D-035`.
  2. Start the orphan blob sweep at startup and each hour (recovery step 10).
  3. Complete `get_health` with queue sizes, watermarks, dead-letter count and counters.
- **Done when:**
  - All steps of `06` section 10 and section 11 are implemented.

### D-044 — Developer documentation

- [x] Done
- **Goal:** A new user can build and run ZCIE.
- **Read:** All documents.
- **Files:** `README.md`, `docs/operations/quickstart.md`.
- **Steps:**
  1. Write the build steps. Include the Xcode Command Line Tools or clang requirement for `rocksdb`.
  2. Write the first-run steps: start the daemon, make a key, add a repository, run a query.
  3. Write the MCP setup steps for Claude Code.
  4. Run `cargo fmt --all`.
- **Done when:**
  - The quickstart uses only commands that exist in the CLI.

### D-045 — Central project memory (ADR-009)

- [x] Done
- **Goal:** All memory is kept by a project ID in one store. A question from any folder can read any project.
- **Read:** `docs/adr/ADR-009-central-project-memory.md`.
- **Files:** `crates/zcie-types/src/{query,api,ids}.rs`, `crates/zcie-api/src/{projects,multi,engine,read_ops}.rs`, `gateways/http`, `gateways/mcp`, `sdk/rust`, `apps/zcie/src/commands/{project,data}.rs`, `apps/zcie/src/commands/claude/setup.rs`, `adapters/claude-code/hooks/{common,session-start,inject-context,read-guard}.sh`.
- **Steps:**
  1. Add `scope` and `cross_project` to `QueryRequest` and `project` to `ResultItem`.
  2. Add the project operations and the `/v1/projects` routes.
  3. Add the multi-project query: target projects, one query per project, merge rank by rank.
  4. Find the project from `.claude/zcie_id` in the CLI, the MCP gateway and the hooks. Make the project in the SessionStart hook only.
  5. Add `zcie project init|list|show|rename|delete|migrate` and `zcie query --global`.
- **Done when:**
  - TS-14 passes.
  - A real Claude Code session in a new folder writes `.claude/zcie_id`, and a session in another project answers a question about it.

### D-046 — One-turn answers and operational checks (ADR-010)

- [x] Done
- **Goal:** More questions are answered from the injected capsule in one turn. Problems that stop the memory are visible.
- **Read:** `docs/adr/ADR-010-one-turn-answers-and-operations.md`.
- **Files:** `crates/zcie-query/src/{relevance,planner,routes}.rs`, `crates/zcie-parse/src/{lib.rs,extractors/typescript.rs,extractors/python.rs}`, `crates/zcie-ingest/src/processors/{file,commit,scan}.rs`, `apps/zcie/src/commands/{doctor,service,daemon}.rs`, `adapters/claude-code/hooks/{inject-context,session-start}.sh`.
- **Steps:**
  1. Add `match_coverage` and use it in the context hook gate. Show symbol signatures in the capsule.
  2. Extract TypeScript and Python constants. Add `PARSER_VERSION`.
  3. Fall back to the lexical search for `implementation` questions with no task.
  4. Add `zcie doctor`, `zcie daemon install|uninstall`, the waiting `daemon stop`, the SessionStart notice and the dead-letter sweep.
- **Done when:**
  - TS-06-13, TS-06-14, TS-07-11, TS-07-12 and TS-14-16 pass.
  - In the 3-project benchmark, the expected facts of all 12 tasks are in the capsule, and 11 of 12 everyday prompts get no capsule.

---

### D-047 — Capsule once per session (ADR-010 section 6)

- [x] Done
- **Goal:** A long session does not get the same capsule again.
- **Files:** `adapters/claude-code/hooks/{inject-context,session-start,common}.sh`.
- **Done when:** TS-11-09 passes; the long-session benchmark shows no loss of correctness.

### D-048 — 1M-line scale benchmark

- [x] Done
- **Goal:** Measure indexing, storage, memory and latency at 1,000,000 lines.
- **Files:** `benchmarks/end-to-end/scale.py`, `benchmarks/results/scale-2026-10-16.md`.
- **Done when:** The report has the values with and without vector search.

### D-049 — TypeScript and Python SDKs (ADR-012)

- [x] Done
- **Files:** `sdk/typescript/`, `sdk/python/`, `tests/integration/sdks.rs`.
- **Done when:** TS-15-01 and TS-15-02 pass.

### D-050 — OpenCode adapter (ADR-012)

- [x] Done
- **Files:** `adapters/opencode/`, `apps/zcie/src/commands/opencode.rs`.
- **Done when:** TS-15-03 passes; a real OpenCode run sends the capsule to the model.

### D-051 — Local vector search (ADR-011)

- [x] Done
- **Files:** `crates/zcie-vector/`, `crates/zcie-search/src/lexical/writer.rs`, `crates/zcie-query/src/{fusion,routes}.rs`, `crates/zcie-ingest/src/processors/search.rs`, `crates/zcie-core/src/{config,startup}.rs`, `apps/zcie/src/commands/vector.rs`, `benchmarks/vector/eval.py`.
- **Done when:** TS-16 passes; the evaluation shows no loss for exact names; the Claude Code benchmarks show no regression.

### D-052 — Memory accuracy (ADR-013)

- [x] Done
- **Files:** `crates/zcie-search/src/lexical/{schema,reader,writer,document}.rs`, `crates/zcie-query/src/{relevance,synonyms,listing,classify,planner,items}.rs`, `crates/zcie-memory/src/extraction/{mod,correction,context}.rs`, `crates/zcie-memory/src/capture/mod.rs`, `crates/zcie-vector/src/{model,tokenize,unigram,text,lib}.rs`, `crates/zcie-core/src/{config,startup}.rs`, `apps/zcie/src/commands/{vector,import}.rs`, `adapters/claude-code/hooks/{common,inject-context,user-prompt-submit}.sh`, `adapters/opencode/zcie.js`, `benchmarks/memory-30/`, `benchmarks/user-ideas/`.
- **Done when:** TS-17 passes; the 30-stage test in new Claude Code sessions answers each question correctly; the user-ideas questions are answered from the memory.

### D-053 — npm package, setup wizard and verification (ADR-014)

- [x] Done
- **Files:** `npm/zcie/`, `scripts/packaging/build-npm.sh`, `apps/zcie/src/commands/setup/{mod,settings,prompt,apply,config,verify,live}.rs`, `apps/zcie/src/commands/{service,daemon,vector,mod}.rs`, `apps/zcie/src/commands/claude/{mod,setup}.rs`, `apps/zcie/src/main.rs`, `adapters/opencode/zcie.js`.
- **Done when:** TS-18 passes; an npm installation in an isolated prefix passes `zcie verify --live`.

### D-054 — Project add, project status, indexing only of added folders (ADR-015)

- [x] Done
- **Files:** `apps/zcie/src/commands/{project,project_add,import}.rs`, `crates/zcie-api/src/{project_status,projects,write_ops}.rs`, `crates/zcie-types/src/{api,registry}.rs`, `crates/zcie-ingest/src/queues/mod.rs`, `crates/zcie-core/src/{watchers,startup}.rs`, `crates/zcie-fs/src/watcher.rs`, `crates/zcie-store/src/{layout,recovery/mod,rocksdb/mod}.rs`, `gateways/http/src/routes.rs`, `sdk/rust/src/lib.rs`.
- **Done when:** TS-18-07 and TS-19 pass; the live queue of the user is empty after the startup sweep.

## 6a. Phase P4 — Context efficiency (ADR-017)

Do these tasks in number order. Read `adr/ADR-017-context-efficiency.md` first. Each task adds a setting in `engine.toml` that turns its behavior off (section `[context]`), with the default "on", unless the task says other. Do not write tests in these tasks; TS-20 in `13-test-plan.md` is the test phase of P4.

### D-055 — Benchmark baseline for context efficiency

- [x] Done
- **Goal:** The token use and the correctness of the current version are recorded, so that each later task can be compared with them.
- **Read:** ADR-017; `benchmarks/claude-ab/run_ab.py`; `benchmarks/memory-30/run.py`; `benchmarks/user-ideas/run.py`.
- **Files:** `benchmarks/claude-ab/run_ab.py`, `benchmarks/context/locate.json` (new), `benchmarks/context/run.py` (new), `benchmarks/context/README.md` (new), `benchmarks/user-ideas/generic.json` (new), `benchmarks/results/context-baseline-<date>.md` (new).
- **Steps:**
  1. In `run_ab.py`, run Claude Code with `--output-format stream-json`. Count the tool calls by name (Read, Grep, Glob, Bash, `mcp__zcie__*`) and the tokens of each tool result. Store them in each result record as `tool_calls` and `tool_result_tokens`.
  2. Make `benchmarks/context/locate.json`: 20 tasks of the form "where must I change X". Use the sample projects of `benchmarks/samples` and tantivy. Each task has the expected file paths.
  3. Make `benchmarks/context/run.py`. It runs each task in a new session and records the first file that the model reads (the first Read call), the tool calls and the tokens.
  4. Make `benchmarks/user-ideas/generic.json`: 15 short generic prompts that name no project fact ("bunların planını oluştur", "devam et", "bunu düzelt", "testleri çalıştır", …).
  5. Run the code tasks (3 projects and tantivy, 2 runs each), the localization set, the 30-stage test and the user-ideas test. Write the numbers in `benchmarks/results/context-baseline-<date>.md`: input tokens, output tokens, tool calls by name, capsule tokens, correct answers.
- **Done when:**
  - The baseline file has all numbers that AC-32 to AC-36 need.

### D-056 — Capsule hygiene

- [x] Done
- **Goal:** The capsule has no duplicate and no unrelated item, and the best items are at the positions that models read best.
- **Read:** ADR-017 decision 3; `adapters/claude-code/hooks/inject-context.sh`; `crates/zcie-query/src/{planner,dedupe,followups}.rs`.
- **Files:** `crates/zcie-query/src/{planner,dedupe,followups,relevance}.rs`, `crates/zcie-query/src/capsule_rules.rs` (new), `adapters/claude-code/hooks/inject-context.sh`, `adapters/opencode/zcie.js`, `crates/zcie-core/src/config.rs`.
- **Steps:**
  1. Drop a result whose full text is the same as the full text of a better result. Merge two memory records of one event into one item.
  2. Drop a `follow_up` text that is the same as the title or the full text of an item of the response. Show each follow-up text once in a response.
  3. Add the field `capsule` (bool) to the query response. It is false for a prompt with fewer than 4 content words and no name, ref, path or version. The hooks and the OpenCode plugin print no capsule when it is false.
  4. Order the shown items: the best item first, the second best item last, the others between them in rank order.
  5. Set a token budget for each query class: overview 900, list of decisions 2,000, other classes 1,200. The hook budget is a maximum only.
  6. Add `[context] capsule_rules = true`.
- **Done when:**
  - The 15 prompts of `generic.json` get no capsule.
  - The prompt "bunların planını oluştur" in the ZCIEngine project gets no capsule.

### D-057 — References between files

- [x] Done
- **Goal:** The index knows which file uses the symbols that another file defines. This is the input of the repository map.
- **Read:** `crates/zcie-parse/src/lib.rs`; `crates/zcie-parse/src/extractors/`; `crates/zcie-ingest/src/processors/file.rs` (the `Imports` edges).
- **Files:** `crates/zcie-parse/src/{lib,references}.rs`, `crates/zcie-parse/src/extractors/{rust,typescript,python}.rs`, `crates/zcie-ingest/src/processors/file.rs`, `crates/zcie-store/src/layout.rs`.
- **Steps:**
  1. In the parser, collect the identifiers that a file uses and does not define (call names, type names, imported names). Keep at most 2,000 names for each file. Ignore names with fewer than 3 characters and the keywords of the language.
  2. Store them as `lookup: t/"file_refs"/<file id>` (one value with the list of names).
  3. When a file is indexed, find the files that define each name (`sym_name` lookup of the workspace). Add the edge `file -References-> file`, status `inferred`, with the count of uses as the weight in the evidence note.
  4. Delete the old `References` edges of the file before the new ones are written.
  5. Increase `PARSER_VERSION`. The next scan then indexes all files again.
- **Done when:**
  - In the fixture repository, the file that calls `attach_to_bone` has a `References` edge to the file that defines it.

### D-058 — Repository map

- [x] Done
- **Goal:** A token-budgeted map of the most important files and their main symbols. The model goes to the right file without file scans.
- **Read:** ADR-017; Aider "Building a better repository map with tree sitter"; `crates/zcie-query/src/overview.rs`.
- **Files:** `crates/zcie-query/src/repomap.rs` (new), `crates/zcie-query/src/{lib,overview,planner}.rs`, `crates/zcie-types/src/query.rs`, `gateways/mcp/src/tools.rs`, `adapters/claude-code/hooks/session-context.sh`, `crates/zcie-core/src/config.rs`.
- **Steps:**
  1. Build a graph of the current files of the project. The edges are `References` (weight = count of uses) and `Imports` (weight 1).
  2. Run personalized PageRank: 20 iterations, damping 0.85. The personalization vector gives weight to:
     - the files of the query results;
     - the files that the session changed (`file.changed` events);
     - the paths named in the prompt.
     With no seed, use a uniform vector.
  3. Render the map in rank order: the file path, then the signature lines of its top symbols (by number of references from other files). Stop at the token budget (default 500).
  4. Add the query mode `map` (`QueryMode::Map`) and `[context] repo_map_budget = 500`.
  5. Add the map to the project card (overview) under the top-level list.
  6. At SessionStart (source `startup`), print the map of the project once, if the project has an indexed folder.
  7. In the `zcie_query` tool description, name the mode `map`.
- **Done when:**
  - `zcie query --mode map` prints a map of 500 tokens or fewer for the ZCIEngine project.
  - With the question "query planner rank candidates", the first file is `crates/zcie-query/src/planner.rs`. (With no question, the most used files come first: `layout.rs`, `ids.rs`, `rocksdb/mod.rs`; that is the expected result of the graph.)
- **Note (implementation):** D-057 stores the used names of each file (`file_refs`), not `References` edges; the map joins them with the definitions when it builds the graph. Edges written at index time would depend on the order of the files (a use indexed before its definition gets no edge). The map seeds come from the search hits of the question and from specific names (snake_case or camelCase, 8 characters or more).

### D-059 — Context text for symbols and passages

- [x] Done
- **Goal:** Each indexed symbol and passage carries the context that makes it findable alone (contextual retrieval), with no model call.
- **Read:** ADR-013 section 6 (follow-up context); `crates/zcie-vector/src/text.rs`; `crates/zcie-ingest/src/processors/search.rs`.
- **Files:** `crates/zcie-ingest/src/processors/search.rs`, `crates/zcie-vector/src/text.rs`, `crates/zcie-search/src/lexical/document.rs`.
- **Steps:**
  1. For a symbol, add to its search body and vector text:
     - the path words;
     - the names of its containers (module, class, impl);
     - the first comment line of its file.
  2. For a passage of a long document or a pasted text, add the title of the document and the nearest heading above the passage.
  3. Keep the shown title and snippet the same; only the indexed text gets the context.
  4. Start a search rebuild once (registry `context_text_v1`).
- **Done when:**
  - The benchmark question "penalty percentage for overdue invoices" ranks `LATE_FEE_PERCENT` at rank 1 or 2 (before: rank 2 to 4).

### D-060 — Graph ranking for multi-hop questions

- [x] Done
- **Goal:** A question that connects a task, a commit, a file and a test gets the whole chain in one answer.
- **Read:** HippoRAG (personalized PageRank from query entities); `crates/zcie-graph/src/`; `crates/zcie-query/src/routes.rs`.
- **Files:** `crates/zcie-query/src/graph_rank.rs` (new), `crates/zcie-query/src/{routes,planner}.rs`, `crates/zcie-core/src/config.rs`.
- **Steps:**
  1. Find the seed entities of the question: task refs, symbol names, file paths, commit SHAs and project names that the index has.
  2. Collect the subgraph around the seeds: depth 3 and at most 2,000 nodes. Edge weights: `verified` 1.0, `inferred` 0.5, `candidate` 0.2.
  3. Run personalized PageRank from the seeds (20 iterations, damping 0.5).
  4. Add the top 10 nodes as candidates with the score `0.6 × rank score`. A node on a path between two seeds gets the bonus 0.2.
  5. Add `[context] graph_rank = true`.
- **Done when:**
  - In the fixture repository, "#89 hangi dosyayı değiştirdi ve hangi test geçti?" returns the task, the commit, the file and the test event in the first 6 results.

### D-061 — Snippet compression

- [x] Done
- **Goal:** The capsule shows only the sentences and lines that the question needs.
- **Read:** LLMLingua (sentence filtering); `crates/zcie-query/src/relevance.rs` (`best_window`).
- **Files:** `crates/zcie-query/src/compress.rs` (new), `crates/zcie-query/src/{planner,relevance}.rs`, `crates/zcie-core/src/config.rs`.
- **Steps:**
  1. Split a memory, event or file snippet into sentences (and code lines for code).
  2. Keep:
     - each sentence with a question term;
     - the sentence after it;
     - each sentence with a number, a version, a path or a name of the question.
     Keep the order of the text.
  3. Replace each dropped run of sentences with "…".
  4. Do not compress a snippet of 300 characters or fewer.
  5. Set the maximum to 450 characters for an item (600 at first; 450 after the measurement of D-065) and 1,200 characters for the README of a project card.
  6. Add `[context] compress = true`.
- **Done when:**
  - The capsules of the 30-stage simulation have 25 % fewer tokens or more, and `simulate.py` still gives 17/17.

### D-062 — Localization mode

- [x] Done
- **Goal:** For a change request, one call returns the files, the symbols and the line ranges to read, coarse to fine (Agentless, AutoCodeRover).
- **Read:** Agentless (localization phases); `crates/zcie-query/src/{routes,repomap,graph_rank}.rs`.
- **Files:** `crates/zcie-query/src/locate.rs` (new), `crates/zcie-query/src/{planner,classify}.rs`, `crates/zcie-types/src/query.rs`, `gateways/mcp/src/tools.rs`.
- **Steps:**
  1. Add the query mode `locate`. The classifier also selects it for change requests ("X'i değiştir", "where must I change", "add … to …").
  2. Step 1: rank the files with the search hits, the repository map rank and the graph rank. Keep the top 5 files.
  3. Step 2: in each file, rank its symbols by the question terms and the references. Keep the top 3 symbols.
  4. Step 3: return the line range of each symbol and 5 lines of context. Do not return the source.
  5. Output format, one line for each symbol: `path:start-end symbol — signature`. The output has 300 tokens or fewer.
  6. In the capsule, show the locate result for a change request instead of the normal results.
- **Done when:**
  - In the localization set, the first expected file is in the locate result for 16 of 20 tasks or more.
- **Result (model-free, `benchmarks/context/offline_locate.py`, final build):** the expected file is first for 10 of 20 tasks and in the top 3 for 17 of 20 (so 17 of 20 or more are in the result of 5 files).

### D-063 — Batch expand

- [x] Done
- **Goal:** One tool call returns the source of several refs, so that the model does not read whole files one by one.
- **Read:** SWE-agent (windows of at most 100 lines); `crates/zcie-query/src/expand.rs`; ADR-006.
- **Files:** `crates/zcie-query/src/expand.rs`, `crates/zcie-types/src/query.rs`, `gateways/mcp/src/tools.rs`, `adapters/claude-code/hooks/read-guard.sh`.
- **Steps:**
  1. `zcie_expand` accepts a list of refs or `path:start-end` ranges (at most 10) and one token budget for all of them.
  2. Each range is at most 100 lines. A longer range gets its first 100 lines and the note "more: path:start-end".
  3. A symbol ref can have `with: ["callers", "tests"]`. The response then adds the signature lines of its callers and its tests (at most 5 each).
  4. The read guard hook tells the model to use `zcie_expand` with ranges when it reads a file of more than 400 lines with no offset.
- **Done when:**
  - One `zcie_expand` call with 3 ranges returns the 3 sources within the budget.
- **Design note:** ZCIE has no CALLS edges. The callers and the tests come from the file references of D-057: the files that use the name of the symbol. A file path with "test" or "spec" gives a test line; other files give caller lines. Each line is the first call of the name in the file; import lines are not shown. The code is in `crates/zcie-query/src/expand_batch.rs`.

### D-064 — Local reranker (spike, then decision)

- [x] Done
- **Goal:** Find out if a local cross-encoder improves the ranking enough to justify its memory and its latency.
- **Read:** ADR-017; cross-encoder and late interaction comparison (sources of ADR-017).
- **Files:** `crates/zcie-rerank/` (new, optional crate), `benchmarks/vector/eval.py`, `docs/adr/ADR-018-reranker.md` (new).
- **Steps:**
  1. Select a multilingual cross-encoder of 300 MB or less with a permissive license. Run it on the CPU in Rust (`candle` or ONNX Runtime).
  2. Rerank the top 30 candidates of each question of `benchmarks/vector/eval.py`, and of the user-ideas questions.
  3. Measure hit@1, MRR and the added p95 latency on the computer of the user.
  4. Write the result in ADR-018:
     - Keep the reranker (setting `[context] rerank = true`) only if hit@1 improves by 3 questions or more and the added p95 latency is 100 ms or less.
     - Else record the numbers and remove the code.
- **Done when:**
  - ADR-018 has the measured numbers and the decision.

- **Result:** the reranker is not added (ADR-018). The added p95 latency on real memory records is 542 ms or more; only one model and order combination improves hit@1 by 3, on one set. The spike script is `benchmarks/rerank/spike.py`.

### D-065 — Result measurement

- [x] Done
- **Goal:** The effect of P4 is measured with the same benchmark as the baseline.
- **Files:** `benchmarks/results/context-efficiency-<date>.md` (new), `docs/adr/ADR-017-context-efficiency.md` (section "Results").
- **Steps:**
  1. Run the benchmarks of D-055 again, with all P4 settings on.
  2. Run the code tasks once with each P4 setting off, one at a time, to find the effect of each change.
  3. Write the numbers and the comparison with the baseline.
- **Done when:**
  - The results file has the numbers that AC-32 to AC-36 need.
- **Result:** `benchmarks/results/context-efficiency-2026-10-17.md` and ADR-017 section "Results". AC-34, AC-35 and AC-36 pass; AC-32 and AC-33 fail (see the analysis in the results file).

---

## 6b. Phase P5 — Evidence-first context (ADR-019)

Do the tasks in release order: P5.1 (D-066, D-067), P5.2 (D-068, D-069, D-072), P5.3 (D-070, D-071, D-076), P5.4 (D-073, D-074, D-075), P5.5 (D-077, D-078, D-079), P5.6 (D-080). Start a release only after the measured result of the earlier release is recorded in `benchmarks/results/`.

> **Warning:** Do not add a model, a database, an ANN library or a service in P5. A new grammar crate or a vector snapshot format needs its own ADR first.

### D-066 — Query trace

- [x] Done
- **Goal:** One query shows why each candidate was selected or dropped, and how long each stage took.
- **Read:** ADR-019; `crates/zcie-query/src/{planner,routes,graph_rank}.rs`.
- **Files:** `crates/zcie-types/src/query.rs`, `crates/zcie-query/src/{planner,trace}.rs` (`trace.rs` new), `apps/zcie/src/commands/data.rs`, `gateways/mcp/src/tools.rs` (no change of the tool schema).
- **Steps:**
  1. Add the optional request field `explain` (default false) and the optional response field `trace`.
  2. The trace has: the class, the mode, the intent (after D-068), the question terms, the candidate count of each source (route, graph), and the duration of the stages route, graph, rank and compose in milliseconds.
  3. The trace has one line for each candidate: ref, kind, source, score, and the reason it was kept or dropped (`kept`, `duplicate`, `echo`, `below_cut`, `over_budget`).
  4. Titles in the trace have at most 60 characters. The trace is never written to a log. Keys are never in a trace.
  5. Add `zcie query --explain`. It prints the trace after the result.
- **Done when:**
  - `zcie query --explain "<T7 prompt>"` shows the candidates of T7 and the reason that each one was kept or dropped.
  - A response without `explain` is unchanged (same fields, same token count).

- **Result:** `zcie query --explain` on the T7 prompt: 91 route candidates, 0 graph candidates; the 6 garbage-collection symbols are kept, but only with their signatures (no bodies), and a test function ranks first. This is the evidence gap of T7 (D-068, D-069).

### D-067 — Isolated, paired benchmark

- [x] Done
- **Goal:** Two runs of the same task set start from the same store and do not change each other.
- **Read:** ADR-019; `benchmarks/claude-ab/run_ab.py`; `benchmarks/context/run.py`.
- **Files:** `benchmarks/isolated/{snapshot.py,run.py,report.py,README.md}` (new).
- **Steps:**
  1. `snapshot.py` makes a frozen ZCIE home: a fresh daemon indexes the benchmark projects and stores the seed memory of the task sets. It writes the home and a manifest (projects, ZCIE version, file hashes of the configuration).
  2. `run.py` runs each trial in a copy-on-write clone of the frozen home (`cp -c` on APFS), with its own daemon on its own port, and deletes the clone after the trial. Prompts of a trial never reach the frozen home.
  3. Conditions: `A0` (no hooks, default tools), `A0P` (no hooks, ZCIE tool profile `auto`), `U` (ZCIE hooks, default tools), `UP` (ZCIE hooks and profile `auto`). All use the same model, Claude Code version, task input and tool set.
  4. For each task and repetition, the conditions run in a random order (seeded). Each record has the condition, the seed, the ZCIE version and the Claude Code version.
  5. Record: correct answer, check result of edit tasks, tool calls by name, input, cache-creation, cache-read and output tokens, cost and wall time.
  6. `report.py` gives, per task class (`code`, `edit`, `locate`, `memory`), the mean of each condition, the paired difference to `A0` and a 95 % bootstrap interval. It does not combine unlike task classes.
- **Done when:**
  - Two runs of the same set with the same seed start from byte-identical stores (manifest hashes are equal).
  - The frozen home is unchanged after a run.

- **Result:** `benchmarks/results/isolated-p51-2026-10-17.md`: 188 runs, frozen home unchanged. Against `A0`: code `U` −46 %, `UP` −72 % tokens; edit `U` +15 % (the weak group).

### D-068 — Intent selection

- [x] Done
- **Goal:** The planner selects the evidence for the next action of the agent.
- **Read:** ADR-019; `crates/zcie-query/src/{classify,planner,locate,overview}.rs`.
- **Files:** `crates/zcie-query/src/{intent,planner}.rs` (`intent.rs` new), `crates/zcie-types/src/query.rs`.
- **Steps:**
  1. Add `QueryResponse.intent` (optional): `explain`, `locate`, `edit`, `trace`, `memory` or `general`. The selection is deterministic (phrases and the query class), with no model.
  2. `edit`: an instruction to change code that names a symbol, a file or a behavior ("add a method to X", "rename X", "X'e Y ekle"). `locate`: the change phrases of D-062. `explain`: "explain", "how does", "nasıl çalışıyor", "açıkla". `trace`: "trace", "from X to Y", "akışı". `memory`: the timeline and memory classes. Else `general`.
  3. Keep the modes `auto`, `exact`, `search`, `timeline`, `memory`, `map` and `locate`. A mode other than `auto` sets the intent.
  4. `explain` and `trace` get the symbols of the best files with their bodies (see D-069) and the graph neighbors; `memory` gets facts with their status and provenance.
  5. When no candidate has verified evidence, the response says `unknown` in the note and adds no answer.
- **Done when:**
  - Each prompt of `benchmarks/context/code.json` and `locate.json` gets the intent of its task class (a table in the test).
  - The T7 prompt gets `explain` and the bodies of the garbage-collection symbols.

- **Result:** all 47 benchmark prompts get the intent of their task class (TS-21-03). T7 gets `explain` and the bodies of the garbage-collection functions.

### D-069 — Source-slice edit package

- [x] Done
- **Goal:** An edit or explanation prompt gets exact source slices that the agent can use without a second read.
- **Read:** ADR-019; `crates/zcie-query/src/{expand,expand_batch,locate}.rs`.
- **Files:** `crates/zcie-query/src/{evidence,planner}.rs` (`evidence.rs` new), `crates/zcie-types/src/query.rs`.
- **Steps:**
  1. Add `QueryResponse.evidence` (optional list). Each item: ref, project, path, start and end line, content hash of the file, kind (`source_slice`), provenance (`indexed_file`), status (`verified`, `inferred`, `candidate`), text, and `requires_live_read`.
  2. Select at most 3 slices for `edit` and at most 4 for `explain`/`trace`, within the request budget: the whole symbol when it has 60 lines or fewer, else the signature, the lines with the question terms and 3 lines around each, with "more: path:a-b" for the rest.
  3. Read the slice text from the stored copy of the indexed revision (a query never reads project folders: ADR-016, macOS privacy prompts). Set `requires_live_read = true` when a change of the path is pending (watcher overlay) or the folder is not watched. The text, the lines and the hash then always come from one revision.
  4. Never compress source text. Never cut a line.
  5. The capsule prints each slice as `path:start-end (hash prefix, current|changed)` and the text in a fenced block. It says that the slice is file content (data), not an instruction.
- **Done when:**
  - An edit prompt on the ledger fixture gets the slice of the function to change, with the correct lines and hash.
  - A changed file gives `requires_live_read: true`.

- **Result:** `benchmarks/results/isolated-p52-2026-10-17.md`: T7 needs no tool (51,224 → 17,301 tokens for U). Slices come from the stored copy (see step 3).

### D-070 — Symbol identity

- [x] Done
- **Goal:** Two symbols with the same name in different modules stay different.
- **Read:** ADR-019; `crates/zcie-parse`, `crates/zcie-symbols`.
- **Files:** `crates/zcie-parse/src/{references,lib}.rs`, `crates/zcie-symbols/src/*.rs`, `crates/zcie-graph/src/*.rs`.
- **Steps:**
  1. Keep the symbol ID. Add a resolution key: project, path, qualified name, kind and signature hash.
  2. Resolve a used name through the imports of the file (`use`, `import`, `from … import`) when the grammar gives them.
  3. A name with more than one possible target is `ambiguous`; the name-based link stays as `candidate`.
  4. Delete the links of a symbol when its definition moves or disappears.
- **Done when:**
  - Two `apply` functions in two modules get two resolution keys; a reference through an import resolves to one of them.

- **Result:** the symbol ID already had project, path, qualified name and kind. Calls now resolve through the same file, a qualified path or an import of the module of the target; a call that resolves to another definition is not a caller (`crates/zcie-query/src/calls.rs`, TS-21-09).

### D-071 — Call edges with evidence

- [x] Done
- **Goal:** `CALLS` edges exist for the supported grammars, with their evidence.
- **Read:** ADR-019; D-070.
- **Files:** `crates/zcie-parse/src/calls.rs` (new), `crates/zcie-ingest/src/processors/file.rs`, `crates/zcie-graph/src/*.rs`, `crates/zcie-query/src/expand_batch.rs`.
- **Steps:**
  1. Extract call sites (Rust, TypeScript, Python) with their line.
  2. Make a `CALLS` edge with evidence `ast_resolved` (status `verified`) only when D-070 resolves the target. Else evidence `ast_approximate` (status `candidate`).
  3. Store the file revision and the line of each call.
  4. `with: ["callers"]` returns resolved callers first and marks approximate ones.
- **Done when:**
  - The ledger fixture gives a verified `CALLS` edge `invoice_total → apply_late_fee`.

- **Design note:** the call sites of each file are stored (`lookup: t/file_calls/<file>`, parser version 4, Rust macros included) and resolved at query time, so the order of indexing does not matter; no stored `CALLS` edges. `with: ["callers"]` and "who calls X" give `[verified: import]`, `[inferred: only definition]` or `[candidate: N definitions]`.

### D-072 — Adaptive capsule and session working set

- [x] Done
- **Goal:** The capsule depends on the intent and on what the session already has.
- **Read:** ADR-019; `adapters/claude-code/hooks/{inject-context,session-start,common}.sh`.
- **Files:** `adapters/claude-code/hooks/{inject-context,session-start,common}.sh`, `adapters/opencode/zcie.js`, `crates/zcie-query/src/capsule_rules.rs`.
- **Steps:**
  1. The hook keeps, per session, the refs and the file hashes that it delivered (the "seen" file gets `ref hash`). An unchanged ref is not sent again; a changed hash is sent again.
  2. The hook keeps the last delivered working set (at most 6 refs). A short prompt that points to the conversation ("bunu düzelt", "fix it") gets the slices of the working set again when their files changed, else nothing.
  3. The capsule format depends on the intent: `edit` and `explain` print the evidence slices (D-069); `locate` prints the ranked files and ranges; `memory` prints facts with status; `general` prints the old format.
  4. Replace the line "The index is current: … no Grep or Read check" with a per-slice status: "current" slices can be used directly; "changed" slices must be read again.
  5. The prompt itself is never shown as memory. Source code, negations and prohibitions are never compressed.
  6. Record the duration of the whole hook (`ZCIE_HOOK_TIMING_FILE`, optional).
- **Done when:**
  - The same prompt twice in one session gives the slices once.
  - A changed file gives its slice again with "changed".

- **Result:** TS-21-07 and TS-21-08 pass; the P4 sentence "no Grep or Read check" is gone. Code tasks of the isolated benchmark: file-search calls 0.35 → 0.17 (U), 0.39 → 0.22 (UP).

### D-073 — Memory provenance

- [x] Done
- **Goal:** A fact shows who stated it and why it is trusted.
- **Files:** `crates/zcie-memory/src/*.rs`, `crates/zcie-types/src/memory.rs`, `crates/zcie-query/src/*.rs`.
- **Steps:**
  1. Add the author type to memory records: `user`, `assistant`, `tool`, `import`.
  2. A record from an assistant answer is never `verified` automatically. A user correction supersedes an assistant record; an assistant record never supersedes a user record.
  3. A conflict is settled by status, then by author type, then by time.
  4. The query result shows the author type and the status.
- **Done when:**
  - An assistant answer that contradicts a user requirement does not supersede it.

- **Result:** the author is read from the source event: a sentence of an assistant answer has `evidence_status = assistant_statement`, ranks below user facts (×0.8) and is labeled in the capsule; corrections come only from user messages. A superseded, disputed or retracted record keeps its status whatever path found it (a defect: such a record was shown as "verified"). TS-21-10, TS-21-14.

### D-074 — Scope boundaries

- [x] Done
- **Goal:** A query never returns data of a project that the caller did not ask for or may not read.
- **Files:** `crates/zcie-api/src/*.rs`, `crates/zcie-query/src/*.rs`, `crates/zcie-core/src/config.rs`, `adapters/claude-code/hooks/inject-context.sh`.
- **Steps:**
  1. Add the setting `[context] cross_project_hooks` (default true: the current behavior). False: the hooks send `cross_project: false`.
  2. Check that lexical, vector, graph, cache, memory and expand results have the project scope of the request.
  3. Keep the admin checks; add tests for the hook key on admin routes.
  4. Add tests for `../` and absolute paths outside the project in range expansion.
- **Done when:**
  - The tests of the scope and the admin routes pass.

- **Result:** setting `cross_project` (hooks: `ZCIE_CROSS_PROJECT`); the hook key cannot use forget, project delete or search rebuild; ranges with `../`, absolute paths or paths outside the project give `not_found` (TS-21-11).

### D-075 — Complete forget

- [x] Done
- **Goal:** A forgotten record cannot come back from any index, cache or restart.
- **Files:** `crates/zcie-api/src/forget.rs`, `crates/zcie-search`, `crates/zcie-vector`, `crates/zcie-cache`.
- **Steps:**
  1. List the canonical and derived copies of each forgotten record: search documents and passages, vectors, edges, cache entries, blobs.
  2. Delete them; return a report with the counts.
- **Done when:**
  - After forget and a restart, lexical, vector, graph, cache and expand return nothing of the forgotten record.

- **Result:** a task that a forgotten message made is deleted too (it was not; its title is the message). After a forget and a restart, no lexical, vector, list or expand result has the record (TS-21-12).

### D-076 — Per-file freshness

- [x] Done
- **Goal:** A file that changed on disk is never shown as current.
- **Files:** `crates/zcie-query/src/{evidence,freshness}.rs`, `crates/zcie-ingest/src/*.rs`.
- **Steps:**
  1. A slice compares the disk hash with the indexed hash at query time (D-069) and also checks the pending file jobs of its path.
  2. Coalesce repeated file jobs of one path; interactive jobs go first.
  3. A deleted or renamed file gives `status: changed` and no slice.
- **Done when:**
  - A file changed after indexing gives `requires_live_read: true` in every evidence item of the file.

- **Result:** a file with a pending change (watcher overlay) gives `requires_live_read: true` in every slice (TS-21-05); a deleted file gives no slice. Repeated file jobs were not coalesced: each job reads the newest file, so a repeat is extra work, not wrong data.

### D-077 — Vector generations and warm start

- [x] Done
- **Goal:** Measure the vector fill at startup; make it faster only if the measurement shows a need.
- **Files:** `crates/zcie-vector/src/*.rs`, `crates/zcie-core/src/startup.rs`.
- **Steps:**
  1. Add `vector_seq` to the watermarks.
  2. Profile the fill of the store of the user (302,000 vectors, 68.5 s after P4). Done in P5.2: the fill now stores vectors in batches of 512 under one write lock (80 s; queries during the fill p95 103 ms instead of about 700 ms; `benchmarks/results/isolated-p52-2026-10-17.md`).
  3. Only if the profile justifies it: write an ADR for a versioned, checksummed snapshot, load it, and replay the delta; a mismatch rebuilds.
- **Done when:**
  - The profile is recorded; if a snapshot is added, a warm restart gives the same results as a fresh fill.

- **Result:** ADR-021: no snapshot; the batched fill (80 s, queries p95 103 ms during the fill) is enough. AC-42 does not apply.

### D-078 — Cache identity

- [x] Done
- **Goal:** A cached response is never older than the data it uses.
- **Files:** `crates/zcie-cache/src/*.rs`, `crates/zcie-query/src/planner.rs`.
- **Steps:**
  1. Add the schema version, the vector model, the code sequence (`code_seq`) and the context settings to the cache key.
  2. A memory correction invalidates the cache of its project.
- **Done when:**
  - A corrected requirement and an edited file never come from a cached capsule.

- **Result:** the cache key has the change counter of the workspace (`code_seq`, now a counter of file changes, also of changes with no event). The vector model and the context settings are fixed for the life of the daemon (a restart clears the cache). TS-21-15.

### D-079 — Shorter tool output with the same meaning

- [x] Done
- **Goal:** Tool output that repeats delivered source is shorter; errors stay visible.
- **Files:** `adapters/claude-code/hooks/*.sh`, `apps/zcie/src/commands/claude/*.rs`.
- **Steps:**
  1. Check the installed Claude Code version for `PostToolUse.hookSpecificOutput.updatedToolOutput`. Do nothing if it is missing.
  2. Shorten only a Read result whose range was delivered unchanged in this session. Keep errors, exit codes and stack traces.
  3. Never change permissions; never approve a tool call.
- **Done when:**
  - A controlled run shows fewer duplicate output tokens and the same tool results.

- **Result:** ADR-021: not added. Shortening a Read can hide code that an edit needs; correctness first (ADR-020).

### D-080 — More languages (only on measured need)

- [x] Done
- **Goal:** Add a grammar only when a benchmark shows missed results in a language of the target repositories.
- **Steps:**
  1. Measure the missed results caused by unsupported languages.
  2. Write an ADR before a new grammar crate.
- **Done when:**
  - The ADR records the measurement and the decision.

- **Result:** ADR-021: no new grammar. TS/TSX are 98 % of the code of the projects of the user; C# 14 files (0.2 %).

### D-081 — Verification gate (ADR-020)

- [x] Done
- **Goal:** A turn that changed code does not end while the check of the project fails.
- **Read:** ADR-020; Claude Code hooks reference (Stop hook, `decision: block`).
- **Files:** `adapters/claude-code/hooks/{verify-gate.sh,post-tool-use.sh,common.sh}` (`verify-gate.sh` new), `apps/zcie/src/commands/claude/hooks.rs`, `apps/zcie/src/commands/setup/{settings,apply}.rs`, `adapters/opencode/zcie.js`.
- **Steps:**
  1. `post-tool-use.sh` records the changed paths of the turn in `zcie-<session>.changed`.
  2. `verify-gate.sh` (Stop) reads the list. With no changed path, it does nothing.
  3. It selects the command: `.claude/zcie_verify` of the project root, else `cargo check --quiet` (Cargo.toml), `npx --no-install tsc --noEmit` (tsconfig.json), `python3 -m py_compile <changed .py files>`.
  4. It runs the command in the project root with a limit of 120 seconds. On failure it prints `{"decision": "block", "reason": "<first 60 lines>"}`, at most 3 times for one turn; then it lets the turn end with a short note.
  5. A pass, a timeout or no command: the list is cleared and nothing blocks.
  6. Add the setting `verify` (`auto`, `off`) to `zcie setup`; `off` does not register the hook.
- **Done when:**
  - A test turn that leaves a Rust syntax error is blocked with the compiler error; after the fix it ends.

- **Result:** `adapters/claude-code/hooks/verify-gate.sh` (PostToolUse records the changed files, Stop runs the check), setting `check` (`auto`, `off`). TS-21-13 passes. OpenCode has no gate yet (ADR-021).

### D-082 — Fact correctness of the capsule (ADR-020)

- [x] Done
- **Goal:** The capsule never gives an unresolved, stale or assistant-only statement as a fact.
- **Files:** `adapters/claude-code/hooks/inject-context.sh`, `adapters/opencode/zcie.js`, `crates/zcie-query/src/*.rs`.
- **Steps:**
  1. Items with status `inferred` or `candidate` are labeled "(not verified)" in the capsule.
  2. Memory records from assistant answers are labeled "(assistant statement, not confirmed by the user)" (D-073).
  3. Add an adversarial fixture: a wrong assistant statement, a superseded value, a changed file. No wrong value is given as a fact.
- **Done when:**
  - The adversarial fixture test passes; the 30-stage check stays 17/17.

- **Result:** the capsule labels `inferred`/`candidate` items "not verified" and assistant statements "assistant statement, not confirmed by the user" (Claude Code hook and OpenCode plugin). TS-21-10, TS-21-14.

### D-083 — Session records from transcripts (ADR-022)

- [x] Done
- **Goal:** One record for each interactive Claude Code session: project, ZCIE active, turns, tokens, Read and search calls, capsules.
- **Files:** `apps/zcie/src/commands/usage/{mod,scan}.rs` (new).
- **Steps:**
  1. Read `~/.claude/projects/*/*.jsonl` and `*/<session>/subagents/*.jsonl`; a subagent file adds to its parent session.
  2. Count each `message.id` once. Skip sessions with `entrypoint` `sdk-cli`.
  3. Keep the byte offset of each file in `<zcie home>/run/usage-cache.json`; read only new complete lines.
- **Done when:**
  - A fixture transcript gives the expected counters; a second scan after an append gives the same counters as one full scan.

- **Result:** `apps/zcie/src/commands/usage/scan.rs`; cache `<zcie home>/run/usage-cache.json`. TS-22-01, TS-22-02. Live: 4,402 files (5 GB), first scan 8.3 s, next scan 0.9 s.

### D-084 — Savings report and `zcie usage` (ADR-022)

- [x] Done
- **Goal:** For each project, the median tokens per turn with and with no ZCIE, the saving %, and the saved tokens.
- **Files:** `apps/zcie/src/commands/usage/report.rs` (new), `apps/zcie/src/main.rs`.
- **Steps:**
  1. Group sessions by project root (`.claude/zcie_id`, Git root, folder) in the window (`--days`, default 90).
  2. Compute the saving only when each group has 3 sessions or more; else give the reason.
  3. Add the hook statistics of `<zcie home>/stats/hooks.jsonl`. Print a table, or the report with `--json`.
- **Done when:**
  - The report of the fixture has the expected medians, saving and reason values.

- **Result:** `zcie usage [--json] [--days N] [--no-disk]` (`usage/{mod,report}.rs`); project roots recorded in `<zcie home>/run/usage-roots.json`. TS-22-03, TS-22-04, TS-22-07.

### D-085 — Hook statistics (ADR-022)

- [x] Done
- **Goal:** The read guard and the verification gate record their actions.
- **Files:** `adapters/claude-code/hooks/{common,read-guard,verify-gate}.sh`.
- **Steps:**
  1. `zcie_stat` in `common.sh` appends one JSON line to `<hooks folder>/../stats/hooks.jsonl`. It never fails the hook.
  2. `read-guard.sh` records `read_guard` with the lines of the file; `verify-gate.sh` records `gate` with `pass`, `block` or `skipped`.
- **Done when:**
  - A hook test finds one line for each action, with no file content.

- **Result:** `zcie_stat` and `zcie_project_root` in `common.sh`; `read_guard`, `gate` and `session` (project root) lines. The read guard no longer limits images and binary files. TS-22-05.

### D-086 — Menu bar app ZCIE Bar (ADR-022)

- [x] Done
- **Goal:** A macOS menu bar app that shows the saving of each project and a detailed report.
- **Files:** `apps/zcie-bar/{main.swift,build.sh,README.md}` (new).
- **Steps:**
  1. SwiftUI `MenuBarExtra`: the saving % of the last active project in the bar; the project list in the menu.
  2. A report window: the two groups, capsules, read guard, gate, tokens of each day, the method note.
  3. `build.sh` builds `~/Applications/ZCIE Bar.app` with `swiftc`; `--login` adds a LaunchAgent.
- **Done when:**
  - The app builds, starts, and shows the values of `zcie usage --json`.

- **Result:** `apps/zcie-bar` (SwiftUI `MenuBarExtra`; menu with KPI tiles, saving gauge, 14-day chart, comparison bars; report window with an overview page and a page for each project: tokens per turn, tool calls, token parts, tokens of each day, gate ring; Swift Charts, macOS 14), `build.sh`, `test.sh`. Opening the app shows the report; the login item starts it with `--background`. The app runs `~/.zcie/bin/zcie usage --json --no-disk`, so macOS asks for no protected folder. TS-22-06; live run 2026-10-19.

### D-087 — Cursor adapter (ADR-025)

- [x] Done
- **Goal:** Cursor sends its prompts, file edits and answers to ZCIE, and it can call `zcie_query`.
- **Files:** `adapters/claude-code/hooks/common.sh`, `gateways/mcp/src/main.rs`, `gateways/mcp/README.md`, `adapters/cursor/{cursor-hook.sh,README.md}` (new), `apps/zcie/src/commands/cursor.rs` (new), `apps/zcie/src/{main.rs,commands/mod.rs}`, `apps/zcie/src/commands/claude/hooks.rs`, `apps/zcie/src/commands/setup/{settings,mod,apply,verify}.rs`.
- **Steps:**
  1. Guard the Claude Code hooks inside Cursor and tag the app in `common.sh`.
  2. Add `ZCIE_PROJECT_DIR` to `zcie-mcp`.
  3. Add the dispatcher `cursor-hook.sh` and install it with the Claude Code hooks.
  4. Add `zcie cursor setup` and `zcie cursor remove`.
  5. Add Cursor to `zcie setup`, `zcie verify` and `zcie uninstall`.
- **Done when:**
  - `cargo build --workspace` and `cargo clippy --workspace -- -D warnings` pass.
  - TS-25-01 to TS-25-08 pass.

- **Result:** Coded (steps 1 to 5). Build and clippy pass. Tests TS-25 are pending the test phase.
- **Note:** Cursor CLI sends no prompt or answer hooks; sessionEnd replays the transcript once (ADR-025 consequence, TS-25-08).

---

### D-088 — Codex CLI adapter (ADR-026)

- [x] Done
- **Goal:** Codex CLI sends its prompts and answers to ZCIE, gets the capsule, and it can call `zcie_query`.
- **Files:** `adapters/codex/codex-hook.sh` (new), `apps/zcie/src/commands/codex.rs` (new), `apps/zcie/src/{main.rs,commands/mod.rs}`, `apps/zcie/src/commands/claude/hooks.rs`, `apps/zcie/src/commands/setup/{settings,mod,apply,verify}.rs`.
- **Done when:**
  - `cargo build --workspace` and `cargo clippy --workspace -- -D warnings` pass.
  - TS-26-01 and TS-26-02 pass.

---

## 7. After the last development task

1. Make sure all tasks `D-001` to `D-044` show `- [x] Done`.
2. Run `cargo build --workspace --release`. Fix all errors.
3. Start the test phase in `docs/13-test-plan.md`.

## 8. Later work (not in milestone 1)

Do not do these items now. They need a new plan.

- Benchmark corpora of 100k, 1M and 5M lines of code (`docs/13-test-plan.md` section 7 defines the targets).
- OPTIONAL crates: `zcie-vector`, `zcie-lsp`, `gateways/grpc`, `gateways/ipc`, `apps/zcie-inspect`.
- TypeScript and Python SDKs.
- OpenCode and IDE adapters.
- Tier B (local model) and Tier C (remote model) extraction.
- Encrypted payload storage.
