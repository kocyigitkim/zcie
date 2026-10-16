# 10 — API and Gateways

Status: Normative. Crates and folders: `zcie-api`, `gateways/http`, `gateways/mcp`, `apps/zcie`, `sdk/`, `adapters/`.

> **Note:** Implementation decisions of milestone 1 are in `docs/adr/ADR-008-implementation-decisions.md`. Where they differ from this file, ADR-008 applies.

## 1. Layers

```
SDK / CLI / hooks / MCP gateway  --HTTP JSON-->  gateways/http (axum, in zcied)  -->  zcie-api  -->  engine crates
```

- `zcie-api` contains the operations. It has no HTTP code.
- `gateways/http` changes HTTP requests into `zcie-api` calls. It runs inside the daemon.
- `gateways/mcp` is a separate binary `zcie-mcp`. It talks to the daemon over HTTP. It does not open RocksDB.
- `gateways/ipc` and `gateways/grpc` are OPTIONAL. They are not in milestone 1.

## 2. Core API (`zcie-api`)

Put all operations on one struct `Engine`. Each function takes an `AuthContext` (from `docs/11-security.md`).

```rust
impl Engine {
    pub fn capture_event(&self, auth: &AuthContext, req: CaptureRequest) -> Result<CaptureReceipt, ApiError>;
    pub fn register_repository(&self, auth: &AuthContext, req: RegisterRepositoryRequest) -> Result<RegisterRepositoryResponse, ApiError>;
    pub fn query(&self, auth: &AuthContext, req: QueryRequest) -> Result<QueryResponse, ApiError>;
    pub fn expand(&self, auth: &AuthContext, req: ExpandRequest) -> Result<ExpandResponse, ApiError>;
    pub fn get_entity(&self, auth: &AuthContext, entity_ref: &str) -> Result<serde_json::Value, ApiError>;
    pub fn get_timeline(&self, auth: &AuthContext, entity_ref: &str) -> Result<Vec<TimelineItem>, ApiError>;
    pub fn create_task(&self, auth: &AuthContext, req: CreateTaskRequest) -> Result<TaskRecord, ApiError>;
    pub fn link_entities(&self, auth: &AuthContext, req: LinkRequest) -> Result<EdgeId, ApiError>;
    pub fn correct_memory(&self, auth: &AuthContext, memory_id: &str, req: CorrectRequest) -> Result<MemoryRecord, ApiError>;
    pub fn forget(&self, auth: &AuthContext, req: ForgetRequest) -> Result<ForgetReceipt, ApiError>;
    pub fn get_health(&self, auth: &AuthContext) -> Result<HealthReport, ApiError>;
    pub fn rebuild_search(&self, auth: &AuthContext) -> Result<JobId, ApiError>;
}
```

The functions are synchronous. The HTTP gateway calls them with `tokio::task::spawn_blocking`.

### 2.1 Error type

```rust
pub enum ApiError {
    Unauthorized,              // 401 "unauthorized"
    Forbidden,                 // 403 "forbidden"
    NotFound(String),          // 404 "not_found"
    InvalidRequest(String),    // 400 "invalid_request"
    PayloadTooLarge,           // 413 "payload_too_large"
    Busy,                      // 503 "busy" (queue or disk saturated)
    Internal(String),          // 500 "internal"
}
```

HTTP error body:

```json
{ "error": { "code": "invalid_request", "message": "idempotency_key is empty" } }
```

Do not put internal paths, keys or stack traces in `message`.

## 3. HTTP routes

Base address: `server.http_addr` (default `127.0.0.1:47831`). All bodies are JSON. All routes except `GET /v1/health` need `Authorization: Bearer <key>`.

| Method | Path | Body | Response |
|--------|------|------|----------|
| `POST` | `/v1/events` | `CaptureRequest` | `CaptureReceipt` |
| `POST` | `/v1/repositories` | `RegisterRepositoryRequest` | `RegisterRepositoryResponse` |
| `POST` | `/v1/query` | `QueryRequest` | `QueryResponse` |
| `POST` | `/v1/expand` | `ExpandRequest` | `ExpandResponse` |
| `GET` | `/v1/entities/{ref}` | — | Entity JSON |
| `GET` | `/v1/timeline/{ref}` | — | `{ "items": [TimelineItem] }` |
| `POST` | `/v1/tasks` | `CreateTaskRequest` | `TaskRecord` |
| `POST` | `/v1/links` | `LinkRequest` | `{ "edge_id": "edge_..." }` |
| `POST` | `/v1/memory/{id}/correct` | `CorrectRequest` | `MemoryRecord` |
| `POST` | `/v1/forget` | `ForgetRequest` | `ForgetReceipt` |
| `GET` | `/v1/health` | — | `HealthReport` (liveness only without a key) |
| `POST` | `/v1/admin/rebuild-search` | — | `{ "job_id": "job_..." }` |
| `POST` | `/v1/projects` | `EnsureProjectRequest` | `ProjectInfo` (ADR-009) |
| `GET` | `/v1/projects` | — | `ProjectList` |
| `GET` | `/v1/projects/{id or name}` | — | `ProjectInfo` |
| `POST` | `/v1/projects/{id or name}/rename` | `RenameProjectRequest` | `ProjectInfo` |
| `DELETE` | `/v1/projects/{id or name}` | — | `ProjectInfo` (admin key) |

URL-encode a ref in a path (`:` becomes `%3A`).

### 3.1 Request bodies

`CaptureRequest` — the client fields of the envelope plus the payload:

```json
{
  "schema": "zcie.event.v1",
  "idempotency_key": "claude-code:s-18:u-421",
  "timestamp_utc": "2026-10-16T07:00:00Z",
  "workspace": "atheron",
  "repository_id": "repo_01JAB3...",
  "source": { "app": "claude-code", "session_id": "s-18", "turn_id": "u-421" },
  "type": "user.message",
  "payload": "Weapon attachment moves during animation #89.",
  "git": { "worktree_id": "wt_01JAB3...", "head": "abc123" },
  "classification": "project_private"
}
```

- `payload` is a string. For JSON payloads, the client sends the JSON as a string.
- `workspace` is the workspace name or the `ws_` ID. The daemon resolves it.
- The daemon sets `event_id`, `seq`, `tenant_id` and `principal_id`.

`RegisterRepositoryRequest`:

```json
{ "workspace": "atheron", "path": "/Users/me/src/atheron", "project": "atheron-client" }
```

`RegisterRepositoryResponse`: `{ "repository_id", "worktree_id", "scan_job_id", "git_available" }`.

`QueryRequest` (HTTP form):

```json
{ "workspace": "atheron", "question": "Which code handles weapon attachment?", "mode": "auto", "token_budget": 250, "max_results": 8 }
```

`ExpandRequest` (HTTP form):

```json
{ "workspace": "atheron", "refs": ["sym:sym_01JAB3..."], "level": "source", "token_budget": 1500 }
```

`CreateTaskRequest`: `{ "workspace", "title", "external_ref"?, "source_event_id" }`.

`LinkRequest`:

```json
{ "workspace": "atheron", "from": "task:task_01...", "relation": "IMPLEMENTS", "to": "commit:commit_abc123",
  "evidence": "user_confirmation", "source_event_id": "ev_01...", "note": "confirmed in review" }
```

`evidence` accepts `user_confirmation` or `client_trace` only. Other evidence kinds come from the engine.

`CorrectRequest`: `{ "text": "...", "source_event_id": "ev_01..." }`.

`ForgetRequest`: `{ "workspace", "selector": { "ref"?, "session_id"?, "before_utc"? }, "reason" }`. See `docs/11-security.md`.

`HealthReport`: `{ "status": "ok", "version", "watermarks", "queue": { "interactive", "background", "dead_letter" }, "storage_bytes", "uptime_s" }`.

## 4. MCP gateway (`zcie-mcp`)

- Binary in `gateways/mcp`. Transport: stdio. Use the `rmcp` crate.
- Configuration from environment: `ZCIE_URL` (default `http://127.0.0.1:47831`), `ZCIE_API_KEY` or `ZCIE_API_KEY_FILE`, `ZCIE_WORKSPACE` (optional override).
- Project: `ZCIE_WORKSPACE` if set. Else the first `.claude/zcie_id` in the current folder or a parent folder (ADR-009). Else `default`.
- Expose exactly two tools. Do not expose capture, link or forget tools.
- Streamable HTTP transport is OPTIONAL. It is not in milestone 1.

### 4.1 Tool `zcie_query`

Description: the constant `QUERY_DESCRIPTION` in `gateways/mcp/src/tools.rs`. It tells the model that the index holds all projects on this computer and that `scope=global` or `project=<name>` reads other projects.

```json
{
  "type": "object",
  "properties": {
    "question": { "type": "string", "description": "Question, symbol name or ref." },
    "mode": { "type": "string", "enum": ["auto", "exact", "search", "timeline", "memory"] },
    "token_budget": { "type": "integer", "minimum": 50, "maximum": 4000 },
    "include_source": { "type": "boolean" },
    "scope": { "type": "string", "enum": ["project", "global"] },
    "project": { "type": "string", "description": "Another project by name or ID." }
  },
  "required": ["question"]
}
```

### 4.2 Tool `zcie_expand`

Description: `Expand ZCIE refs into evidence, source code or original records under a token budget.`

```json
{
  "type": "object",
  "properties": {
    "refs": { "type": "array", "items": { "type": "string" }, "minItems": 1, "maxItems": 20 },
    "level": { "type": "string", "enum": ["evidence", "source", "audit"] },
    "token_budget": { "type": "integer", "minimum": 50, "maximum": 4000 }
  },
  "required": ["refs"]
}
```

The tool result is the response JSON as one text content item. On an error, return `isError: true` with the error message.

## 5. CLI (`zcie`)

Use `clap` with derive. All commands except `daemon start` call the HTTP API.

| Command | Action |
|---------|--------|
| `zcie daemon start [--foreground]` | Start `zcied`. Without `--foreground`, start it as a detached child process. |
| `zcie daemon stop` | Read `run/zcied.pid`. Send `SIGTERM`. |
| `zcie daemon status` | Call `GET /v1/health`. |
| `zcie repo add <path> --workspace <name>` | `POST /v1/repositories`. |
| `zcie query "<question>" [--mode <m>] [--budget <n>] [--global]` | `POST /v1/query` with `cross_project: true`. `--global` reads all projects. Print JSON. |
| `zcie project init [--dir <d>] [--name <n>]` | `POST /v1/projects`. Write `<root>/.claude/zcie_id`. |
| `zcie project list \| show [p] \| rename <p> <name> \| delete <p>` | Project management (ADR-009). |
| `zcie project migrate` | Write `.claude/zcie_id` into each registered root that has none. |
| `zcie expand <ref>... [--level <l>]` | `POST /v1/expand`. |
| `zcie capture --type <t> --session <s> [--turn <t>] <text or ->` | `POST /v1/events`. `-` reads stdin. |
| `zcie link <from> <relation> <to> --confirm` | `POST /v1/links` with `user_confirmation`. |
| `zcie forget --ref <ref> \| --session <s>` | `POST /v1/forget`. |
| `zcie health` | `GET /v1/health` with the key. Print a short table. |
| `zcie rebuild-search` | `POST /v1/admin/rebuild-search`. |
| `zcie key create --name <client>` | Make a new API key. Print it one time. Store the hash in `clients.toml`. Runs locally, no daemon needed. |

Global options: `--url`, `--project` (alias `--workspace`, or `ZCIE_WORKSPACE`), `--key` (or `ZCIE_API_KEY`). Without `--project`, the CLI reads `.claude/zcie_id` in the current folder or a parent folder.

## 6. SDKs

All SDKs are thin HTTP wrappers. They contain no engine logic.

| SDK | Milestone 1 | Package |
|-----|-------------|---------|
| Rust | Yes | `sdk/rust` crate `zcie-sdk` (uses `reqwest` blocking) |
| TypeScript | OPTIONAL | `sdk/typescript` package `@zcie/client` (uses `fetch`) |
| Python | OPTIONAL | `sdk/python` package `zcie` (uses `urllib.request`) |

Methods (same names in all SDKs, in the language's case style):

`capture`, `register_repository`, `query`, `expand`, `get_entity`, `get_timeline`, `create_task`, `link`, `correct_memory`, `forget`, `health`.

Rust example:

```rust
let client = zcie_sdk::Client::new("http://127.0.0.1:47831", &api_key, "atheron");
let response = client.query("Which code handles weapon attachment?", 250)?;
```

## 7. Adapters

### 7.1 Claude Code adapter (`adapters/claude-code`)

Use Claude Code hooks. Each hook is a small POSIX shell script that calls `curl`.

| Hook | Event type sent |
|------|-----------------|
| `SessionStart` | `session.start` |
| `UserPromptSubmit` | `user.message` (the prompt text) |
| `PostToolUse` (tools `Edit`, `Write`) | `file.changed` (the file path, with the current `turn_id`) |
| `PostToolUse` (tool `Bash`) | `command.run` |
| `Stop` | `session.end` |

Rules:

- Read the hook input JSON from stdin. Use `jq` to get `session_id`, `prompt`, `tool_name` and `tool_input`.
- Set `turn_id` = a counter stored in `${TMPDIR}/zcie-<session_id>.turn`. Increment it on `UserPromptSubmit`.
- Set `idempotency_key` = `claude-code:<session_id>:<turn_id>:<hook>:<sha of input>`.
- Use `curl --max-time 2 --silent`. Always `exit 0`.

> **Warning:** A hook MUST NOT block or fail the user session. If the daemon is not available, the hook exits with code 0 and does nothing.

- Provide `adapters/claude-code/install.md` with the `settings.json` hook entries and the MCP server entry for `zcie-mcp`.

### 7.2 Git adapter (`adapters/git`)

- `post-commit` hook script. It sends `git.commit` with payload `{ "sha": "<HEAD>" }` and `source.app = "git"`.
- `idempotency_key` = `git:<repository_id>:<sha>`.
- Same rule: `--max-time 2`, always `exit 0`.

### 7.3 Custom example (`adapters/custom-example`)

- One short script that registers a repository, captures one message and runs one query with `curl`.
