# 06 — Storage

Status: Normative. Implement this file in `crates/zcie-store`.

> **Note:** Implementation decisions of milestone 1 are in `docs/adr/ADR-008-implementation-decisions.md`. Where they differ from this file, ADR-008 applies.

## 1. Storage parts

| Part | Location | Canonical? | Content |
|------|----------|------------|---------|
| RocksDB | `state/rocksdb/` | Yes | Events, entities, edges, lookups, jobs, watermarks. |
| Object store | `state/objects/` | Yes | Payload blobs and file content blobs. |
| Tantivy | `state/search/` | No (derived) | Full-text indexes. Can be rebuilt. |
| Vector index | `state/vectors/` | No (derived) | OPTIONAL. Not in milestone 1. |

Rule: If a derived index and the canonical store do not agree, the canonical store is correct.

## 2. Key helper

Put these functions in `crates/zcie-store/src/keys.rs`:

```rust
/// Joins key parts with the byte 0x00.
pub fn key(parts: &[&str]) -> Vec<u8>;
/// Encodes a sequence number as a 20-digit zero-padded string.
pub fn seq_part(seq: u64) -> String;
/// Makes a prefix for a prefix scan. The prefix ends with 0x00.
pub fn prefix(parts: &[&str]) -> Vec<u8>;
```

In the tables below, `/` shows a 0x00 separator. `t` is the tenant ID. Literal parts are in quotes.

## 3. Column families

Open the database with all nine CFs. Create missing CFs at open.

### 3.1 `registry`

| Key | Value |
|-----|-------|
| `t / "counter" / "event_seq"` | `{"v":1,"value":<u64>}` last assigned event seq |
| `t / "workspace" / <workspace_id>` | `WorkspaceRecord` |
| `t / "project" / <project_id>` | `ProjectRecord` |
| `t / "repository" / <repository_id>` | `RepositoryRecord` |
| `t / "worktree" / <worktree_id>` | `WorktreeRecord` |

Registry record types (add them to `zcie-types`):

```rust
pub struct WorkspaceRecord { pub workspace_id: String, pub name: String, pub principal_id: String, pub created_utc: String }
pub struct ProjectRecord { pub project_id: String, pub workspace_id: String, pub name: String }
pub struct RepositoryRecord { pub repository_id: String, pub workspace_id: String, pub project_id: Option<String>, pub name: String, pub has_git: bool }
pub struct WorktreeRecord { pub worktree_id: String, pub repository_id: String, pub root_path: String, pub last_scanned_head: Option<String>, pub last_scan_utc: Option<String> }
```

### 3.2 `events`

| Key | Value |
|-----|-------|
| `t / seq_part(seq)` | `EventEnvelope` |

Events are immutable. The only exception is `forget` (section 10).

### 3.3 `entities`

| Key | Value |
|-----|-------|
| `t / "file" / <file_id>` | `FileRecord` |
| `t / "sym" / <symbol_id>` | `SymbolRecord` |
| `t / "mem" / <memory_id>` | `MemoryRecord` |
| `t / "task" / <task_id>` | `TaskRecord` |
| `t / "commit" / <commit_id>` | `CommitRecord` |

```rust
pub struct CommitRecord { pub commit_id: String, pub scope: Scope, pub sha: String, pub message: String, pub author_utc: String, pub changed_paths: Vec<String>, pub trailers: Vec<(String, String)> }
```

### 3.4 `edges_out` and `edges_in`

| CF | Key | Value |
|----|-----|-------|
| `edges_out` | `t / <from ref> / <RELATION> / <to ref>` | `Edge` |
| `edges_in` | `t / <to ref> / <RELATION> / <from ref>` | `Edge` (same value) |

Rules:

- Write both keys in the same `WriteBatch`. Never write only one.
- One edge exists for each `(from, relation, to)`. A new write for the same triple replaces the old value. Keep the old `edge_id`. Increment `generation`.
- Do not lower an edge from `Verified` to a weaker status by a new write. Exception: `Rejected` by a user correction.
- To read all edges from a ref, scan the prefix `t / <ref> /` in `edges_out`.

### 3.5 `revisions`

| Key | Value |
|-----|-------|
| `t / "watermark" / "canonical_seq"` | `{"v":1,"value":<u64>}` |
| `t / "watermark" / "graph_seq"` | `{"v":1,"value":<u64>}` |
| `t / "watermark" / "search_seq"` | `{"v":1,"value":<u64>}` |
| `t / "file_state" / <worktree_id> / <path>` | `{"v":1,"file_id":..,"content_hash":..,"seq":..}` |
| `t / "overlay" / <worktree_id> / <path>` | `{"v":1,"seq":<u64>,"content_hash":..}` live overlay entry |

### 3.6 `lookup`

| Key | Value |
|-----|-------|
| `t / "idem" / <idempotency_key>` | `{"v":1,"event_id":..,"seq":..}` |
| `t / "event_id" / <event_id>` | `{"v":1,"seq":..}` |
| `t / "ws_name" / <name>` | `{"v":1,"workspace_id":..}` |
| `t / "path" / <worktree_id> / <path>` | `{"v":1,"file_id":..}` |
| `t / "sym_ident" / <worktree_id> / <path> / <qualified_name> / <kind>` | `{"v":1,"symbol_id":..}` |
| `t / "sym_name" / <workspace_id> / <lowercase name> / <symbol_id>` | `{"v":1}` |
| `t / "file_syms" / <file_id> / <symbol_id>` | `{"v":1}` |
| `t / "task_ext" / <workspace_id> / <external_ref>` | `{"v":1,"task_id":..}` |
| `t / "commit_sha" / <repository_id> / <sha>` | `{"v":1,"commit_id":..}` |
| `t / "blob" / <content hash>` | `{"v":1,"size":..,"ref_count":..}` number of events that reference the blob |
| `t / "turn" / <session_id> / <turn_id> / <event_id>` | `{"v":1,"event_type":..}` events of one client turn (for `ClientTrace`) |
| `t / "timeline" / <ref> / seq_part(seq) / <event_id>` | `{"v":1}` event touches the ref |

### 3.7 `jobs`

| Key | Value |
|-----|-------|
| `t / "pending" / seq_part(seq) / <job_id>` | `JobRecord` with state `Pending` or `Running` |
| `t / "dead" / <job_id>` | `JobRecord` with state `DeadLetter` and `last_error` |

`JobRecord`, `JobKind`, `JobPriority` and `JobState` are defined in `docs/07-ingest-and-indexing.md` section 3.1. When a job is `Done`, delete its `pending` key in the same `WriteBatch` as the job's output records.

### 3.8 `policies`

| Key | Value |
|-----|-------|
| `t / "tombstone" / <ref or event_id>` | `{"v":1,"forgotten_utc":..,"reason":..}` |
| `t / "capture" / <workspace_id>` | OPTIONAL per-workspace capture override |

## 4. RocksDB options

- Set `create_if_missing = true` and `create_missing_column_families = true`.
- Use `WriteOptions::set_sync(config.storage.sync_writes)` for capture batches. The default is `true`.
- Use non-sync writes for derived records (entities and edges from jobs). The jobs CF makes them recoverable.
- Use `multi_get_cf` to read many keys. Use `prefix_iterator_cf` with a check that each key starts with the prefix. Stop the scan at the first key without the prefix.
- Do not tune block cache, bloom filters or compaction in milestone 1 (rule C-11).

## 5. Object store

Layout: `state/objects/<hex[0..2]>/<hex[2..4]>/<hex>.zst`. `<hex>` is the 64-character BLAKE3 hex of the uncompressed content.

`ObjectStore::put(bytes) -> content_hash` procedure:

1. Calculate the BLAKE3 hash of `bytes`.
2. If the final file exists, return the hash. Do not write again.
3. Compress `bytes` with zstd, level 3.
4. Write the compressed bytes to a temporary file in the same folder: `<hex>.zst.tmp-<ulid>`.
5. Call `sync_all()` on the temporary file.
6. Rename the temporary file to the final name.
7. Call `sync_all()` on the folder.
8. Return `"b3:" + hex`.

`ObjectStore::get(hash)`: read the file, decompress, calculate the hash again, and compare. Return an error if the hash is different.

> **Caution:** Write and sync the blob BEFORE the `WriteBatch` that refers to it. A crash can then leave an orphan blob, but never an event without its blob.

## 6. Capture batch

`Ingest::capture` writes one `WriteBatch` per event. Hold the capture lock (one `tokio::sync::Mutex`) from step 2 to step 6. Write the blob before the lock.

1. Look up `lookup: t/"idem"/<key>`. If it exists, return the stored receipt with `duplicate: true`. Do not write.
2. Take the capture lock. Look up the idempotency key again.
3. Set `seq = event_seq + 1`. Make a new `event_id`.
4. Add these items to one `WriteBatch`:
   - `events: t/seq_part(seq)` → envelope
   - `lookup: t/"idem"/<key>` → event_id and seq
   - `lookup: t/"event_id"/<event_id>` → seq
   - `lookup: t/"blob"/<hash>` → size (if a payload exists)
   - `registry: t/"counter"/"event_seq"` → seq
   - `revisions: t/"watermark"/"canonical_seq"` → seq
   - `jobs: t/"pending"/seq_part(seq)/<job_id>` → job (if the event type needs a job)
5. Write the batch with sync write options.
6. Store `seq` in memory as the new `event_seq`. Release the lock.
7. Push the job to the in-memory `JobQueue`. If the queue is full, do not fail. The job stays in the jobs CF and recovery picks it up.
8. Return `{ event_id, seq, durable: true, indexed: false, duplicate: false }`.

## 7. Watermarks

| Watermark | Meaning | Updated by |
|-----------|---------|------------|
| `canonical_seq` | Highest durable event seq. | Capture batch. |
| `graph_seq` | All jobs with seq ≤ this value are complete. | Worker, after a job completes. |
| `search_seq` | Tantivy has committed documents for all seq ≤ this value. | Search writer, after a commit. |

Rules:

- Calculate `graph_seq` as: (lowest seq in `jobs: t/"pending"`) − 1. If no pending job exists, `graph_seq = canonical_seq`.
- A worker deletes the pending job key in the same `WriteBatch` as the derived records. Then it updates `graph_seq`.
- The search writer reads `graph_seq` (value G), drains its channel, commits Tantivy, then writes `search_seq = G`.
- Watermarks only increase. Never write a lower value.
- `search_seq ≤ graph_seq ≤ canonical_seq` is always true.

## 8. Startup and crash recovery

Do these steps at daemon startup, before the HTTP server starts:

1. Take the lock on `run/daemon.lock`. Stop if another daemon holds it.
2. Create the runtime tree if it does not exist (`docs/04-folder-structure.md` section 6).
3. Read `manifest.json`. Make it if it does not exist. Stop if the schema version is too high.
4. Open RocksDB. RocksDB replays its WAL automatically.
5. Read `event_seq` from the registry. Store it in memory.
6. Delete files that end with `.tmp-<ulid>` in `state/objects/`.
7. Scan `jobs: t/"pending"` in key order. Push each job to the `JobQueue`.
8. Open Tantivy. If it fails to open, delete `state/search/` and run a full rebuild (section 9).
9. If `search_seq < graph_seq`, re-send search documents for each event with seq in `(search_seq, graph_seq]`.
10. Start the background orphan blob sweep (section 11).
11. Start a reconciliation scan for each registered worktree (`docs/07-ingest-and-indexing.md`).
12. Start the HTTP server.

Each job handler MUST be idempotent. A replay of a job SHALL NOT create duplicate entities or edges. Use the lookup keys to find existing IDs.

A job that fails three times goes to `jobs: t/"dead"/<job_id>`. Write the job as JSON to `queue/dead-letter/<job_id>.json` too. Log an error.

## 9. Search index rebuild

Tantivy is derived. Rebuild it with `zcie rebuild-search` or at step 8 of recovery.

1. Stop the search writer.
2. Delete `state/search/` and create the empty indexes.
3. Write `search_seq = 0` directly. (This is the only permitted decrease.)
4. Scan `entities` for `file` records that are not deleted. Index their content from the object store.
5. Scan `events` for message events. Index their payload. Skip forgotten events.
6. Scan `entities` for `mem` and `task` records. Index their text.
7. Commit. Write `search_seq = graph_seq`.
8. Start the search writer.

Each Tantivy document has a unique key field `doc_key` (the ref). Before you add a document, delete the old documents with the same `doc_key`. So a replay does not make duplicates.

## 10. Forget (deletion)

`forget(scope, selector)` procedure:

1. Find the target refs and event IDs.
2. In one sync `WriteBatch`:
   - Write a tombstone for each target in `policies`.
   - Replace the payload reference of each target event with `None`. Set a field `"forgotten": true`.
   - Delete memory records, tasks and edges that come only from the target events.
   - Delete the `lookup: t/"blob"/<hash>` key if no other event refers to the blob.
3. Invalidate the L0 cache for the scope.
4. Send delete operations for the `doc_key` values to the search writer.
5. The orphan sweep deletes the unreferenced blobs.
6. Return an audit receipt: `{ forgotten_refs, forgotten_events, utc }`.

Queries SHALL check tombstones. A ref with a tombstone SHALL NOT be returned, even if Tantivy still has the document.

## 11. Orphan blob sweep

Run the sweep at startup in the background, and then each hour.

1. List all `.zst` files in `state/objects/`.
2. For each file, look up `lookup: t/"blob"/b3:<hex>`.
3. If the key does not exist and the file is older than 10 minutes, delete the file.

The 10-minute age protects a blob that a capture writes at this moment.

## 12. Coherent reads

- A query that reads more than one key uses one RocksDB snapshot (`Store::snapshot()`).
- Each response includes the watermarks that were valid for its snapshot.
- Backups: `zcie backup` makes a RocksDB checkpoint in `state/checkpoints/<utc>/` and copies `objects/`. OPTIONAL in milestone 1.
