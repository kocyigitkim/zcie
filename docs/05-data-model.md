# 05 — Data Model

Status: Normative. All crates use the types in this file. Put the Rust types in `crates/zcie-types`.

## 1. ID format

Each entity ID has a prefix and a ULID. Use the `ulid` crate to make the ULID.

| Entity | Prefix | Example |
|--------|--------|---------|
| Event | `ev_` | `ev_01JAB3...` |
| Workspace | `ws_` | `ws_01JAB3...` |
| Project | `proj_` | `proj_01JAB3...` |
| Repository | `repo_` | `repo_01JAB3...` |
| Worktree | `wt_` | `wt_01JAB3...` |
| File | `file_` | `file_01JAB3...` |
| Symbol | `sym_` | `sym_01JAB3...` |
| Memory record | `mem_` | `mem_01JAB3...` |
| Task | `task_` | `task_01JAB3...` |
| Commit | `commit_` | `commit_<git sha>` (no ULID) |
| Edge | `edge_` | `edge_01JAB3...` |
| Job | `job_` | `job_01JAB3...` |
| Query | `q_` | `q_01JAB3...` |

Rules:

- Make a Rust newtype for each ID kind. Example: `pub struct EventId(pub String);`.
- Check the prefix when you parse an ID. Return an error if the prefix is wrong.
- A ref (glossary) uses the form `<kind>:<id>`. Example: `sym:sym_01JAB3...`.
- The workspace has a human name too. Example: `atheron`. The registry maps the name to the `ws_` ID.
- A project (ADR-009) is a workspace record. Its workspace ID is the project ID. New projects get a `proj_` ID; old `ws_` IDs stay valid as project IDs. The file `<project root>/.claude/zcie_id` holds the project ID (one line). All memory of a project is under its ID.

## 2. Scope

```rust
/// The isolation scope of a request or a record.
pub struct Scope {
    pub tenant_id: String,          // "local" in the first milestone
    pub principal_id: String,
    pub workspace_id: String,
    pub project_id: Option<String>,
    pub repository_id: Option<String>,
    pub worktree_id: Option<String>,
}
```

Each stored record has a `scope` field. A query reads a record only if the record scope is inside the request scope. See `docs/11-security.md`.

## 3. Event envelope (`zcie.event.v1`)

```rust
pub struct EventEnvelope {
    pub schema: String,               // always "zcie.event.v1"
    pub event_id: EventId,            // set by the daemon
    pub seq: u64,                     // set by the daemon
    pub idempotency_key: String,      // set by the client
    pub timestamp_utc: String,        // RFC 3339, set by the client or the daemon
    pub scope: Scope,
    pub source: EventSource,
    pub event_type: EventType,
    pub payload_ref: Option<String>,  // content hash "b3:..."; None if no payload
    pub payload_size: u64,
    pub git: Option<GitContext>,
    pub classification: Classification,
    pub redactions: u32,              // number of secrets that redaction replaced
}

pub struct EventSource { pub app: String, pub session_id: String, pub turn_id: Option<String> }
pub struct GitContext { pub worktree_id: String, pub head: String }
pub enum Classification { Public, ProjectPrivate, Personal, Secret }
```

### 3.1 Event types

| `EventType` | Serialized name | Payload |
|-------------|-----------------|---------|
| `UserMessage` | `user.message` | Message text. |
| `AssistantMessage` | `assistant.message` | Message text. Capture only if policy permits. |
| `ToolCall` | `tool.call` | JSON: tool name and arguments. |
| `ToolResult` | `tool.result` | JSON or text output. |
| `CommandRun` | `command.run` | JSON: command, exit code, output (truncated by policy). |
| `TestResult` | `test.result` | JSON: test name, status (`passed`/`failed`), output, OPTIONAL `refs` (list of refs that the test covers). |
| `FileChanged` | `file.changed` | JSON: path, old hash, new hash. |
| `FileDeleted` | `file.deleted` | JSON: path, old hash. |
| `GitCommit` | `git.commit` | JSON: SHA, message, changed paths. |
| `SessionStart` | `session.start` | None. |
| `SessionEnd` | `session.end` | None. |
| `UserConfirmation` | `user.confirmation` | JSON: a link or a memory record that the user confirmed. |

Serialize the enum with `#[serde(rename = "...")]` to the names in the table.

## 4. Memory records

```rust
pub struct MemoryRecord {
    pub memory_id: MemoryId,
    pub scope: Scope,
    pub kind: MemoryKind,
    pub text: String,                     // short statement, 500 characters or fewer
    pub status: MemoryStatus,
    pub confidence: f32,                  // 0.0 to 1.0
    pub source_event_ids: Vec<EventId>,   // MUST NOT be empty
    pub valid_from: String,               // RFC 3339
    pub valid_until: Option<String>,
    pub last_verified_revision: Option<String>,
    pub superseded_by: Option<MemoryId>,
    pub extractor: String,                // "tier_a.v1", "manual", "tier_b.<model>"
    pub generation: u64,                  // increments on each change
}

pub enum MemoryKind {
    Fact, UserRequirement, Constraint, DesignDecision, Task,
    Observation, Hypothesis, Correction, Preference,
}

pub enum MemoryStatus { Active, Verified, Disputed, Superseded, Retracted }
```

A `Task` memory record also has a task entity. See section 6.

## 5. Code entities

```rust
pub struct FileRecord {
    pub file_id: FileId,
    pub scope: Scope,                     // includes repository_id and worktree_id
    pub path: String,                     // relative to the worktree root, "/" separator
    pub language: Option<String>,         // "rust", "typescript", "python", or None
    pub content_hash: String,             // "b3:..."
    pub size: u64,
    pub is_binary: bool,
    pub git_revision: Option<String>,     // last commit SHA that changed the file
    pub indexed_seq: u64,                 // event seq that produced this state
    pub deleted: bool,
}

pub struct SymbolRecord {
    pub symbol_id: SymbolId,
    pub scope: Scope,
    pub file_id: FileId,
    pub path: String,
    pub language: String,
    pub name: String,                     // "attach_to_bone"
    pub qualified_name: String,           // "WeaponSocket::attach_to_bone"
    pub kind: SymbolKind,
    pub signature: String,                // first line of the declaration, 200 characters or fewer
    pub span: [u32; 2],                   // [start_line, end_line], 1-based
    pub file_hash: String,
    pub parser_quality: ParserQuality,
}

pub enum SymbolKind { Function, Method, Struct, Class, Enum, Trait, Interface, TypeAlias, Constant, Module }
pub enum ParserQuality { Complete, PartialSyntaxError }
```

Symbol identity rule: make the `symbol_id` stable for the same `(worktree_id, path, qualified_name, kind)`. Keep the map in the `lookup` CF. Reuse the old ID when a file is parsed again.

## 6. Task entity

```rust
pub struct TaskRecord {
    pub task_id: TaskId,
    pub scope: Scope,
    pub title: String,
    pub state: TaskState,
    pub external_ref: Option<String>,     // example: "#89" or "JIRA-12"
    pub created_from: EventId,
    pub generation: u64,
}

pub enum TaskState { Open, InProgress, Resolved, Failed, Abandoned }
```

## 6.1 Commit, registry and job records

These types are defined in other documents. Put them in `zcie-types` too.

| Type | Defined in |
|------|------------|
| `CommitRecord` | `docs/06-storage.md` section 3.3 |
| `WorkspaceRecord`, `ProjectRecord`, `RepositoryRecord`, `WorktreeRecord` | `docs/06-storage.md` section 3.1 |
| `JobRecord`, `JobKind`, `JobPriority`, `JobState` | `docs/07-ingest-and-indexing.md` section 3.1 |

## 7. Edges

```rust
pub struct Edge {
    pub edge_id: EdgeId,
    pub scope: Scope,
    pub from: String,                     // ref, example "task:task_01..."
    pub relation: Relation,
    pub to: String,                       // ref
    pub status: EdgeStatus,
    pub evidence: Evidence,
    pub confidence: f32,
    pub created_utc: String,
    pub generation: u64,
}

pub enum Relation {
    RequestedBy, Implements, AffectsFile, DefinesSymbol, Calls, Imports, References,
    TestedBy, FailedIn, FixedBy, ChangedInCommit, DerivedFrom, DecidedIn,
    Contradicts, Supersedes, BelongsTo, RelatedTo,
}

pub enum EdgeStatus { Verified, Inferred, Candidate, Rejected }

pub struct Evidence {
    pub kind: EvidenceKind,
    pub source_event_ids: Vec<EventId>,
    pub revision: Option<String>,
    pub note: Option<String>,             // 200 characters or fewer
}

pub enum EvidenceKind {
    ClientTrace,        // client sent an explicit trace ID that joins the two entities
    UserConfirmation,   // the user confirmed the link
    CommitTrailer,      // commit message has "ZCIE-Task: <task ref or external ref>"
    TestOutcome,        // a test.result event names the entity
    AstResolved,        // the parser found the relation in one file (DefinesSymbol)
    AstApproximate,     // name match across files, not resolved
    GitHistory,         // Git data (commit, diff-tree) states the relation
    Derivation,         // the engine made the record from the source event, or the user wrote an explicit ref
    TextMention,        // a message mentions a path or a symbol name
    Temporal,           // events occurred close in time
}
```

### 7.1 Edge status rules (mandatory)

| Evidence kind | Maximum status |
|---------------|----------------|
| `ClientTrace`, `UserConfirmation`, `CommitTrailer` | `Verified` |
| `AstResolved`, `TestOutcome`, `GitHistory`, `Derivation` | `Verified` |
| `TextMention`, `AstApproximate` | `Inferred` |
| `Temporal` | `Candidate` |

- A function that creates an edge MUST lower the status to the maximum in this table.
- A traversal MUST NOT raise the status of an edge.
- A path status is the lowest status of all edges in the path.

## 8. Serialization

- Serialize all stored values as JSON with `serde_json`. See `docs/adr/ADR-002-json-values.md`.
- Each stored value has a field `"v": 1` (schema version).
- Use `snake_case` for JSON field names and enum values, except event types (section 3.1) and relations.
- Serialize `Relation` as `SCREAMING_SNAKE_CASE`. Example: `IMPLEMENTS`, `CHANGED_IN_COMMIT`.

## 9. Key encoding

Build each RocksDB key from parts. Join the parts with the byte `0x00`. Use the helper `zcie_store::key(&[&str]) -> Vec<u8>`.

- Encode a `u64` sequence as a 20-digit zero-padded decimal string. Example: `00000000000000000042`. The sort order is then correct.
- A path can contain `/`. A path cannot contain `0x00`. So a path is a safe key part.
- See `docs/06-storage.md` for the key layout of each CF.
