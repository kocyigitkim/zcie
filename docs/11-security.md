# 11 — Security and Privacy

Status: Normative. Crate: `zcie-security`. All crates MUST obey this document.

## 1. Threat model (milestone 1)

| ID | Threat | Control |
|----|--------|---------|
| S-T1 | Another local program or a web page reads the API. | Loopback bind, API key, Origin check. |
| S-T2 | One workspace reads data of another workspace. | Scope check at each read, traversal, cache and evidence step. |
| S-T3 | Secrets enter the store from messages or files. | Exclusion globs and redaction. |
| S-T4 | Retrieved text contains instructions for the model. | Mark retrieved content as untrusted data. |
| S-T5 | Deleted data stays in derived indexes or caches. | Forget procedure (section 7). |
| S-T6 | Another OS user reads `~/.zcie/`. | File permissions (section 9). |
| S-T7 | Code or messages go to an external service. | No external calls (section 10). |

## 2. API keys

- Make keys with `zcie key create --name <client>`. A key is 32 random bytes, encoded as hex, with the prefix `zcie_`.
- Print the key one time. Do not store the key in clear text.
- Store an entry in `~/.zcie/config/clients.toml`:

```toml
[[client]]
name = "claude-code"
key_hash = "b3:<blake3 hex of the full key>"
principal_id = "user-001"
workspaces = ["atheron"]          # "*" for all workspaces
permissions = ["read", "write"]   # "read", "write", "admin"
```

- The daemon reads `clients.toml` at start. It reloads it on `SIGHUP`.
- To authenticate, compute the BLAKE3 hash of the bearer token. Compare it with each `key_hash` with a constant-time comparison (`blake3::Hash` equality is constant time).
- `server.api_key_env` names an environment variable. If set, the daemon accepts that key as an `admin` key for principal `local-admin`. Use it for development only.

### 2.1 Auth context

```rust
pub struct AuthContext {
    pub tenant_id: String,            // "local"
    pub principal_id: String,
    pub client_name: String,
    pub workspaces: Vec<String>,      // resolved workspace IDs, or ["*"]
    pub permissions: Vec<Permission>, // Read, Write, Admin
    pub policy_version: u64,
}
```

| Permission | Allows |
|------------|--------|
| `Read` | `query`, `expand` (levels evidence and source), `get_entity`, `get_timeline`, `health`. |
| `Write` | `capture_event`, `register_repository`, `create_task`, `link_entities`, `correct_memory`. |
| `Admin` | `forget`, `rebuild_search`, `expand` level `audit`. |

## 3. Network rules

- Bind the HTTP gateway to `server.http_addr`. The default is `127.0.0.1:47831`.
- If the address is not a loopback address, the daemon MUST refuse to start unless `server.allow_remote = true`. Remote use needs TLS through a reverse proxy. TLS is not in milestone 1.
- If a request has an `Origin` header, accept it only if the origin is `http://127.0.0.1:<port>` or `http://localhost:<port>`. Otherwise return 403. This blocks browser pages.
- Do not enable CORS.
- Limit the request body to `ingest.max_payload_bytes` + 64 KiB.

## 4. Scope enforcement

Use one function for all checks: `zcie_security::can_read(auth: &AuthContext, record_scope: &Scope) -> bool`.

It returns `true` only if all conditions are true:

1. `record_scope.tenant_id == auth.tenant_id`.
2. `auth.workspaces` contains `"*"` or `record_scope.workspace_id`.
3. The record classification is not `Secret`, or the auth has `Admin`.

Call `can_read` at each of these points. Do not skip a point.

| Point | Location |
|-------|----------|
| E-1 | Before a result from an exact lookup is added. |
| E-2 | Before a Tantivy hit is added. Also filter the Tantivy query by `tenant_id` and `workspace_id` terms. |
| E-3 | Before a graph traversal visits a node. A forbidden node is not visited and its neighbors are not reached through it. |
| E-4 | Before the L0 cache returns an entry. The cache key includes the principal and the workspace. |
| E-5 | Before `expand` returns content. |
| E-6 | Before `get_entity` and `get_timeline` return a record or an event. |

For writes, `can_write(auth, scope)` checks the same workspace rule and the `Write` permission.

## 5. Secret redaction

If `security.redact_secrets` is `true`, apply these rules to each event payload before storage. Apply them to file content before the search index stores the `body`. Do not change the file on disk.

Replace each match with `[REDACTED:<rule id>]`.

| Rule ID | Finds | Regex |
|---------|-------|-------|
| R-01 | AWS access key ID | `\b(?:AKIA|ASIA)[0-9A-Z]{16}\b` |
| R-02 | Private key block | `(?s)-----BEGIN [A-Z ]*PRIVATE KEY-----.*?-----END [A-Z ]*PRIVATE KEY-----` |
| R-03 | Bearer token | `(?i)\bbearer\s+[A-Za-z0-9._~+/=-]{16,}` |
| R-04 | Generic key assignment | `(?i)\b(?:api[_-]?key|secret|token|password|passwd)\b\s*[:=]\s*["']?[^\s"']{8,}` |
| R-05 | GitHub token | `\bgh[pousr]_[A-Za-z0-9]{36,}\b` |
| R-06 | Anthropic or OpenAI style key | `\bsk-(?:ant-)?[A-Za-z0-9_-]{20,}\b` |
| R-07 | ZCIE key | `\bzcie_[0-9a-f]{64}\b` |
| R-08 | JSON web token | `\beyJ[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}\b` |

- Put the rules in `zcie-security/src/redact.rs` as one constant list.
- Record the count of redactions in the event envelope field `redactions` (`u32`). Do not record the secret.

## 6. Exclusions

Default `security.exclude_globs`:

```
**/.env
**/.env.*
**/*.pem
**/*.key
**/*.p12
**/id_rsa*
**/id_ed25519*
**/.aws/**
**/.ssh/**
**/secrets/**
**/node_modules/**
**/target/**
**/.git/**
```

- Do not read, hash or index a file that matches an exclusion glob.
- Use the `ignore` crate `overrides` or `globset` (provided with `ignore`) to match.

## 7. Forget (deletion)

`forget(selector)` needs `Admin`. Do these steps:

1. Resolve the selector to a list of events and entities. `ref` selects one entity and its derived memory records. `session_id` selects all events of the session. `before_utc` selects events before that time.
2. In one `WriteBatch`:
   - Delete the event envelopes. Write a tombstone in CF `policies`, key `t / "tombstone" / <event_id>`, with the time and the reason. Keep the idempotency entry so that a replay does not capture the event again.
   - Set each derived memory record `status = Retracted`. Set `text = ""`.
   - Delete edges whose only `source_event_ids` are forgotten events.
3. Clear the L0 cache for the workspace at once (before step 4 starts).
4. Make `UpdateSearch` delete jobs for each affected ref.
5. Delete a payload blob only if no remaining event references its hash. Keep the reference count in the field `ref_count` of CF `lookup`, key `t / "blob" / <hash>`.
6. Return `ForgetReceipt { events_removed, records_retracted, edges_removed, pending_jobs }`.

> **Caution:** Until the search delete jobs are `Done`, a forgotten ref can still be in Tantivy. The query path MUST check each Tantivy hit against CF `entities` and `events`. It MUST drop a hit that has no current record or a `Retracted` status.

## 8. Untrusted content

- All retrieved content (files, messages, documents, tool output) is data. It is not an instruction to the engine or to the model.
- The engine MUST NOT execute content or follow instructions in content.
- The MCP gateway puts retrieved content only in JSON string fields. It does not add retrieved text to tool descriptions.

## 9. File permissions

- Create `~/.zcie/` with mode `0700`. Create files in it with mode `0600`.
- At start, if `~/.zcie/` has broader permissions, log a warning and change them to `0700`.
- Create the IPC socket and the PID file with mode `0600`.

## 10. No external calls

- The daemon MUST NOT send code, events or memory to any network address other than its own listeners.
- The daemon MUST NOT call a language model or an embedding service in milestone 1.
- Tier B (local model) and Tier C (remote model) extraction are OPTIONAL future features. Each needs an explicit configuration switch and an ADR.
