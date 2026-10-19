# 15 — Configuration

Status: Normative. Implement this file in `crates/zcie-core/src/config.rs`.

> **Note:** Implementation decisions of milestone 1 are in `docs/adr/ADR-008-implementation-decisions.md`. Where they differ from this file, ADR-008 applies.

## 1. Load order

Load the configuration in this order. A later source replaces an earlier value.

1. Built-in defaults (the values in section 2).
2. The file `~/.zcie/config/engine.toml`, if it exists.
3. Environment variables `ZCIE_<SECTION>_<KEY>`. Example: `ZCIE_SERVER_HTTP_ADDR=127.0.0.1:50000`.

Rules:

- Use `serde` with `#[serde(default)]` on each section struct. A missing key gets its default.
- An unknown key is an error. Use `#[serde(deny_unknown_fields)]`.
- Expand `~` in paths with the `HOME` environment variable.
- Copy the file in section 2 to `config/default.toml` in the repository.

`zcie setup` and `zcie config set` (ADR-014) keep their settings in `~/.zcie/config/setup.toml` and write these `engine.toml` keys: `server.http_addr`, `ingest.worker_count`, `vector.enabled`, `vector.model`. Other keys in `engine.toml` stay. They also write `~/.zcie/config/client.json` (`url`, `budget`) for the OpenCode plugin.

## 2. `engine.toml` (defaults)

```toml
[server]
http_addr = "127.0.0.1:47831"
allow_remote = false
api_key_env = "ZCIE_API_KEY"

[storage]
data_dir = "~/.zcie/instances/local"
sync_writes = true

[ingest]
max_payload_bytes = 1048576
queue_capacity = 10000
worker_count = 4
debounce_ms = 200

[index]
max_file_bytes = 1048576
languages = ["rust", "typescript", "python"]

[query]
default_token_budget = 250
max_token_budget = 4000
max_results = 8
graph_max_depth = 3
graph_max_fanout = 50

[cache]
l0_capacity_entries = 10000
negative_ttl_ms = 2000

[security]
redact_secrets = true
exclude_globs = [".env", ".env.*", "*.pem", "*.key", "id_rsa*"]

[capture]
assistant_messages = false
command_output_max_bytes = 65536

[vector]
enabled = true                 # use local vector search if the model folder exists (ADR-011)
model = "potion-multilingual-128M"  # folder under <home>/models/ (zcie vector install); if this default is
                                    # missing, the first installed known model is used (ADR-013)
# min_similarity = 0.20         # lowest cosine similarity of a vector hit; default 0.20 for the
                                # multilingual model, 0.30 for potion-base-8M
```

## 3. Key reference

| Key | Type | Default | Meaning |
|-----|------|---------|---------|
| `server.http_addr` | string | `127.0.0.1:47831` | HTTP listen address. A non-loopback address needs `server.allow_remote = true`. |
| `server.allow_remote` | bool | `false` | Permit a non-loopback `http_addr`. See `docs/11-security.md`. |
| `server.api_key_env` | string | `ZCIE_API_KEY` | Name of the environment variable that clients use for their key. The daemon does not read it. |
| `storage.data_dir` | path | `~/.zcie/instances/local` | Instance folder. |
| `storage.sync_writes` | bool | `true` | Sync write for capture batches. Set `false` only for benchmarks. |
| `ingest.max_payload_bytes` | integer | `1048576` | Largest accepted payload. A larger payload gets HTTP 413. |
| `ingest.queue_capacity` | integer | `10000` | Size of the in-memory job queue. |
| `ingest.worker_count` | integer | `4` | Number of ingest workers. |
| `ingest.debounce_ms` | integer | `200` | Wait time to merge rapid file changes for one path. |
| `index.max_file_bytes` | integer | `1048576` | Larger files get metadata and hash only. |
| `index.languages` | string list | `rust, typescript, python` | Languages for symbol extraction. |
| `query.default_token_budget` | integer | `250` | Budget when the request has no budget. |
| `query.max_token_budget` | integer | `4000` | Highest budget a request can ask for. |
| `query.max_results` | integer | `8` | Maximum results in one capsule. |
| `query.graph_max_depth` | integer | `3` | Maximum traversal depth. |
| `query.graph_max_fanout` | integer | `50` | Maximum edges read from one node. |
| `cache.l0_capacity_entries` | integer | `10000` | L0 cache size in entries. |
| `cache.negative_ttl_ms` | integer | `2000` | Lifetime of a cached "not found" result. |
| `security.redact_secrets` | bool | `true` | Apply secret redaction to payloads before storage. |
| `security.exclude_globs` | string list | see above | Files that are never read or indexed. |
| `capture.assistant_messages` | bool | `false` | Store `assistant.message` events. If `false`, reject them with HTTP 403. |
| `capture.command_output_max_bytes` | integer | `65536` | Truncate command and test output to this size. Mark it `"truncated": true`. |

Token estimate rule: `tokens = ceil(chars / 4)`.

## 4. `clients.toml`

This file lists the API keys. The daemon reads it at startup. Make it with `zcie key create --name <client>`.

```toml
[[client]]
name = "claude-code"
principal_id = "user-001"
key_hash = "b3:<64 hex>"           # BLAKE3 hash of the API key
workspaces = ["atheron"]           # workspace names; "*" means all
permissions = ["read", "write"]    # "read", "write", "admin"
```

Rules:

- Never store the plain API key. Store only the BLAKE3 hash.
- `zcie key create` makes a random key with 32 bytes, prints it once in hex, and stores the hash.
- A request is authorized only if its key hash matches a `key_hash`.
- `read` permits query operations. `write` permits capture. `admin` permits registration, forget and rebuild.
- If `clients.toml` does not exist, the daemon makes one admin client and prints its key once at startup.

## 5. Policy files

Policy files are in `~/.zcie/config/policies/`. Copies of the defaults are in `config/policies/`. All are OPTIONAL in milestone 1. If a file does not exist, the values in `engine.toml` apply.

| File | Content |
|------|---------|
| `capture.toml` | Per-workspace capture opt-in and event types to reject. |
| `indexing.toml` | Extra exclude globs and generated-file patterns per repository. |
| `security.toml` | Extra secret patterns (regex) for redaction. |
| `context.toml` | Per-client token budget overrides. |
