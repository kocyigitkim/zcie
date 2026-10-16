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
