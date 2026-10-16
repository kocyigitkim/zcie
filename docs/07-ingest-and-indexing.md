# 07 — Ingest and Indexing

Status: Normative. Crates: `zcie-ingest`, `zcie-fs`, `zcie-git`, `zcie-parse`, `zcie-symbols`, `zcie-search`.

> **Note:** Implementation decisions of milestone 1 are in `docs/adr/ADR-008-implementation-decisions.md`. Where they differ from this file, ADR-008 applies.

## 1. Overview

The write path has two parts:

1. The **capture path** stores an event durably and sends an acknowledgement. It is fast and does no parsing.
2. The **background path** executes jobs. Jobs parse files, extract memory, update the graph and update the search index.

```
Client event / file change
  -> auth + scope + validate + redact + idempotency
  -> blob write + fsync (if payload)
  -> RocksDB WriteBatch (event + idempotency entry + jobs)
  -> ACK to client
  -> workers: ExtractMemory / IndexFile / DeleteFile / IndexCommit / ScanRepository
  -> canonical derived records + graph_seq watermark
  -> UpdateSearch -> Tantivy commit -> search_seq watermark
  -> L0 cache invalidation
```

## 2. Capture path (mandatory steps)

The function is `zcie_ingest::capture_event(request) -> Result<CaptureReceipt>`.
Do these steps in this order:

1. Authenticate the client. Resolve the request scope. See `docs/11-security.md`.
2. Validate the request:
   - `schema` MUST be `zcie.event.v1`.
   - `idempotency_key` MUST NOT be empty. It MUST be 256 characters or fewer.
   - The payload size MUST be `ingest.max_payload_bytes` or less. Otherwise return error `payload_too_large`.
   - The scope in the event MUST be inside the client scope. Otherwise return error `forbidden`.
3. Apply the capture policy:
   - If the event type is `assistant.message` and `capture.assistant_messages` is `false`, return `{ "captured": false, "reason": "policy" }`. Do not store the event.
   - If the event type is `command.run` or `tool.result`, truncate the output to `capture.command_output_max_bytes`. Add the marker `[truncated]`.
4. If `security.redact_secrets` is `true`, apply the redaction rules in `docs/11-security.md` to the payload.
5. Read the key CF `lookup`, key `t / "idem" / <idempotency_key>` (see `docs/06-storage.md` section 3.6). If the key exists, return the stored receipt. Set `duplicate: true`. Stop here.
6. If a payload exists:
   1. Compute the BLAKE3 hash of the payload bytes.
   2. If the blob does not exist, compress it with zstd (level 3). Write it to a temporary file. Call `fsync`. Rename it to `objects/<aa>/<bb>/<hash>.zst`.
7. Get the next sequence number from the sequence counter (CF `registry`, key `t / "counter" / "event_seq"`).
8. Make one RocksDB `WriteBatch` with these items:
   - The envelope in CF `events`.
   - The idempotency entry in CF `lookup`.
   - The new sequence counter value in CF `registry`.
   - One job `ExtractMemory` in CF `jobs` (for message, test, command and tool events).
   - The new value of watermark `canonical_seq`.
9. Write the batch. Use `sync = true` if `storage.sync_writes` is `true`.
10. Send the job IDs to the in-memory worker queue.
11. Return the receipt.

```rust
pub struct CaptureReceipt {
    pub event_id: EventId,
    pub seq: u64,
    pub durable: bool,      // true after step 9
    pub indexed: bool,      // always false at return time
    pub duplicate: bool,
    pub canonical_seq: u64,
}
```

> **Caution:** Use one mutex for steps 5 to 9. Two concurrent captures with the same idempotency key MUST give one event. The daemon is the single writer, so one mutex is sufficient.

## 3. Jobs

### 3.1 Job record

```rust
pub struct JobRecord {
    pub job_id: JobId,
    pub kind: JobKind,
    pub scope: Scope,
    pub priority: JobPriority,        // Interactive or Background
    pub source_seq: u64,              // event seq that made the job
    pub target: String,               // event ID, file path, commit SHA or repository ID
    pub attempts: u32,
    pub state: JobState,
    pub last_error: Option<String>,
    pub created_utc: String,
    pub updated_utc: String,
}

pub enum JobKind { ExtractMemory, IndexFile, DeleteFile, ScanRepository, IndexCommit, UpdateSearch }
pub enum JobPriority { Interactive, Background }
pub enum JobState { Pending, Running, Done, DeadLetter }
```

| Job kind | Target | Work |
|----------|--------|------|
| `ExtractMemory` | event ID | Tier A extraction. See `docs/08-memory-and-graph.md`. |
| `IndexFile` | worktree ID + path | Hash, parse, store file and symbol records, edges. |
| `DeleteFile` | worktree ID + path | Mark the file deleted. Remove its symbols and edges from current state. |
| `ScanRepository` | repository ID + worktree ID | Full scan. Makes `IndexFile` and `IndexCommit` jobs. |
| `IndexCommit` | commit SHA | Store commit entity, `ChangedInCommit` edges, trailer links. |
| `UpdateSearch` | entity ref | Add, replace or delete the Tantivy document for the entity. |

### 3.2 Job rules

- A handler MUST be idempotent. A second execution of the same job MUST give the same stored state.
- Write the derived records and the job state `Done` in one `WriteBatch`.
- If a handler fails, increment `attempts`. Store `last_error`. Set state to `Pending`.
- After 3 failed attempts, set state to `DeadLetter`. Log the error. Do not try again.
- A handler that makes entities or edges MUST also make one `UpdateSearch` job for each changed entity.
- Delete `Done` job records after 24 hours. Keep `DeadLetter` records until an operator deletes them.

### 3.3 Worker loop

The daemon starts `ingest.worker_count` workers. Each worker does these steps:

1. Take the next job ID from the queue. Take `Interactive` jobs before `Background` jobs.
2. Read the job record. If the state is `Done` or `DeadLetter`, skip it.
3. Set the state to `Running`.
4. Call the handler for the job kind.
5. Write the result as in section 3.2.
6. Go to step 1.

Use two bounded `tokio::sync::mpsc` channels: one for `Interactive` and one for `Background`. The total capacity is `ingest.queue_capacity`. If the queue is full, the capture path still stores the job in RocksDB. The recovery scan (step 1 of `docs/06-storage.md` recovery) puts it in the queue later. Do not drop an accepted event.

### 3.4 Priority

- Jobs from a `user.message`, `file.changed` with a client source, and `test.result` are `Interactive`.
- Jobs from `ScanRepository` and the file watcher are `Background`.

## 4. Repository registration and full scan

### 4.1 Registration

`register_repository(path, scope, policy)` does these steps:

1. Make the path absolute. Make sure that the folder exists.
2. Run `git -C <path> rev-parse --show-toplevel`. If the command succeeds, the repository is a Git repository. Otherwise it is a plain folder. Set `git_available = false`.
3. Make a `repo_` ID and a `wt_` ID. Store them in CF `registry`.
4. Make one `ScanRepository` job. Return `{ repository_id, worktree_id, scan_job_id }`.
5. Start the file watcher for the worktree (section 5).

### 4.2 Full scan (`ScanRepository` handler)

1. List the files:
   - Git repository: run `git -C <root> ls-files -z --cached --others --exclude-standard`. Split the output at `0x00`.
   - Plain folder: use the `ignore` crate walker. It applies `.gitignore` rules.
2. Remove each path that matches a glob in `security.exclude_globs`.
3. Do not follow a symlink that points outside the worktree root.
4. For each remaining file:
   1. If the file size is more than `index.max_file_bytes`, store a `FileRecord` with metadata only. Do not parse.
   2. Read the first 8 KiB. If it contains a `0x00` byte, set `is_binary = true`. Store metadata and hash only.
   3. Otherwise make one `IndexFile` job.
5. For a Git repository, run `git -C <root> log --format=%H -n 200`. Make one `IndexCommit` job for each SHA that is not stored.
6. Store `revisions / <tenant> / <worktree> / head` = current HEAD SHA.

Process files in batches of 200 jobs per `WriteBatch`.

## 5. File watcher

- Use the `notify` crate. Watch the worktree root recursively.
- Ignore events in `.git/` and in paths that match `security.exclude_globs`.
- Collect events for each path for `ingest.debounce_ms`. Then process the path one time.
- For a create or modify event: make an `IndexFile` job (`Background`).
- For a remove event: make a `DeleteFile` job.
- For a rename event: make a `DeleteFile` job for the old path and an `IndexFile` job for the new path.
- When the daemon starts, run one reconciliation for each worktree: compare the stored file hashes with the files on disk. Make jobs for each difference. This finds changes that the watcher did not see.

## 6. `IndexFile` handler

1. Read the file. If the file does not exist, do the `DeleteFile` handler instead.
2. Compute the BLAKE3 hash. If the stored `FileRecord` has the same hash, set the job `Done`. Stop here.
3. Write the content to the object store (section 2, step 6).
4. Detect the language from the extension:

| Extension | Language |
|-----------|----------|
| `.rs` | `rust` |
| `.ts`, `.tsx` | `typescript` |
| `.py` | `python` |
| `.md`, `.txt` | none (document) |
| other | none (no parse) |

5. If the language is in `index.languages`, parse the file (section 8).
6. In one `WriteBatch`:
   - Store the new `FileRecord`.
   - Store the new `SymbolRecord`s. Delete the symbols of the old file version that are not in the new version.
   - Replace all `DefinesSymbol` and `Imports` edges from this file.
   - Store `revisions / <tenant> / <worktree> / <path> / <seq>` = content hash.
   - Make `UpdateSearch` jobs for the file and each changed symbol.
   - Update watermark `graph_seq`.
7. Remove the path from the live overlay (section 10).

## 7. Git integration

Use `std::process::Command` to run `git`. Set the working folder with `-C <root>`. Do not use a shell.

| Purpose | Command |
|---------|---------|
| Current HEAD | `git -C <root> rev-parse HEAD` |
| Current branch | `git -C <root> rev-parse --abbrev-ref HEAD` |
| Commit data | `git -C <root> show -s --format=%H%x00%an%x00%aI%x00%B <sha>` |
| Changed files of a commit | `git -C <root> diff-tree --no-commit-id -r --name-status -M <sha>` |
| Recent commits | `git -C <root> log --format=%H -n <count>` |
| Changes after a commit | `git -C <root> log --format=%H <old>..HEAD` |

### 7.1 `IndexCommit` handler

1. Read the commit data and the changed files.
2. Store a commit entity `commit_<sha>` in CF `entities` with: SHA, author, time, message, changed paths.
3. For each changed path, add edge `file:<file_id>` -`ChangedInCommit`-> `commit:<sha>`. Set status `Verified`. Set evidence `{ kind: GitHistory, note: "git diff-tree" }`. If no `FileRecord` exists for a path (for example, a deleted file), store the path in the commit entity only.
4. Parse the trailers in the commit message. A trailer line has the form `ZCIE-Task: <value>`. The value is a task ref (`task:task_01...`) or an external ref (`#89`). Resolve an external ref through CF `lookup`, key `t / "task_ext" / <workspace_id> / <value>`.
5. For each resolved task, add edge `task:<id>` -`Implements`-> `commit:<sha>`. Status `Verified`. Evidence kind `CommitTrailer`.
6. If the trailer value does not resolve, store it in the commit entity as `unresolved_trailers`. Do not make an edge.

> **Note:** The temporal match (task and commit close in time) MAY make an edge with status `Candidate` only. Milestone 1 does not make temporal edges.

### 7.2 Git hook

The `adapters/git` post-commit hook sends a `git.commit` event. The `ExtractMemory` handler for `git.commit` makes an `IndexCommit` job. See `docs/10-api-and-gateways.md`.

## 8. Parser (Tree-sitter)

- Keep one `tree_sitter::Parser` per language per worker. Do not share a parser between threads.
- Walk the syntax tree with a `TreeCursor`. Do not use Tree-sitter queries in milestone 1.
- If the tree has an error node, set `parser_quality = PartialSyntaxError`. Still store the symbols that were found.
- `span` = `[start_position.row + 1, end_position.row + 1]`.
- `signature` = first line of the node text, trimmed, 200 characters or fewer.
- `qualified_name` = names of enclosing `impl`/class/module nodes and the symbol name, joined with `::` (Rust) or `.` (TypeScript, Python).

### 8.1 Node kind map

| Language | Node kind | `SymbolKind` |
|----------|-----------|--------------|
| rust | `function_item` (outside `impl`) | `Function` |
| rust | `function_item` (inside `impl` or `trait`) | `Method` |
| rust | `struct_item` | `Struct` |
| rust | `enum_item` | `Enum` |
| rust | `trait_item` | `Trait` |
| rust | `type_item` | `TypeAlias` |
| rust | `const_item`, `static_item` | `Constant` |
| rust | `mod_item` | `Module` |
| typescript | `function_declaration` | `Function` |
| typescript | `method_definition` | `Method` |
| typescript | `class_declaration` | `Class` |
| typescript | `interface_declaration` | `Interface` |
| typescript | `enum_declaration` | `Enum` |
| typescript | `type_alias_declaration` | `TypeAlias` |
| typescript | `lexical_declaration` at top level with an `arrow_function` value | `Function` |
| python | `function_definition` (outside class) | `Function` |
| python | `function_definition` (inside class) | `Method` |
| python | `class_definition` | `Class` |

Get the name from the child field `name`. Skip a node with no `name` field.

### 8.2 Edges from the parser

| Edge | Status | Evidence kind |
|------|--------|---------------|
| `file:<id>` -`DefinesSymbol`-> `sym:<id>` | `Verified` | `AstResolved` |
| `file:<id>` -`Imports`-> `file:<id>` (if the import path resolves to a stored file) | `Inferred` | `AstApproximate` |

Import nodes: rust `use_declaration`, typescript `import_statement`, python `import_statement` and `import_from_statement`.
Resolve a relative import path (`./x`, `../x`, `.x`) against the file folder. Do not resolve package imports in milestone 1.
`Calls` and `References` edges are OPTIONAL. They are not in milestone 1.

## 9. Search index (Tantivy)

### 9.1 Indexes

Use three Tantivy indexes in `state/search/`:

| Index | Content |
|-------|---------|
| `code` | File records and symbol records of parsed languages. |
| `documents` | Markdown and text files. |
| `conversations` | Events of type message, test result, command, and memory records. |

### 9.2 Schema (same for all three)

| Field | Tantivy type | Options |
|-------|--------------|---------|
| `ref` | text | `STRING \| STORED` (not tokenized) |
| `kind` | text | `STRING \| STORED` |
| `tenant_id` | text | `STRING` |
| `workspace_id` | text | `STRING` |
| `repository_id` | text | `STRING` |
| `worktree_id` | text | `STRING` |
| `path` | text | `TEXT \| STORED` |
| `name` | text | `TEXT \| STORED` |
| `body` | text | `TEXT` (not stored) |
| `seq` | u64 | `INDEXED \| STORED \| FAST` |

- `body` for a symbol: the signature and the symbol source text. For a file: the full text. For an event: the payload text.
- Split identifiers for `body` and `name`: add `attach to bone` next to `attachToBone` and `attach_to_bone`. Do this with a simple function before indexing.

### 9.3 Updates

- The `UpdateSearch` handler does not commit. It sends the document to one search writer task.
- The search writer task deletes the old document with the same `ref` (`delete_term`), then adds the new document.
- The writer commits when 1 second passed since the last commit, or 500 documents are pending.
- After a commit, store watermark `search_seq` = highest `seq` in the commit. Reload the reader.
- For a deleted entity, only delete the term. Do not add a document.

### 9.4 Rebuild

`zcie rebuild-search` deletes the `search/` folder. Then it makes `UpdateSearch` jobs for all current entities from CF `entities`. Set `search_seq = 0` before the rebuild.

## 10. Live overlay (read-your-writes)

- The live overlay is an in-memory set of `(worktree_id, path)` with a pending `IndexFile` or `DeleteFile` job.
- Add the path when the job is made. Remove the path when the job is `Done`.
- A query that returns a result from a path in the live overlay MUST read the current file from disk. It MUST set `freshness = "partial"` if it cannot confirm the result. See `docs/09-query-and-context.md`.
- An event is readable by ID at once after the ACK, from CF `events`. It does not wait for the search index.
