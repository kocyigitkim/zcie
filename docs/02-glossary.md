# 02 — Glossary

Status: Normative. Use these terms with these meanings only (rule W-06).

## 1. Terms

| Term | Meaning |
|------|---------|
| ZCIE | ZeroContext Intelligence Engine. The full system in this repository. |
| Daemon | The `zcied` process. It is the only process that opens the database. |
| Client | A program that sends requests to the daemon. Examples: Claude Code hook, CLI, SDK, MCP gateway. |
| Gateway | A transport adapter in `gateways/`. It changes a transport request into a call to `zcie-api`. |
| Adapter | A client-side program in `adapters/`. It captures events from a tool and sends them to the daemon. |
| Tenant | The top isolation unit. The first milestone uses one tenant: `local`. |
| Principal | A user or a service identity that sends requests. |
| Workspace | A named group of projects for one principal. Example: `atheron`. |
| Project | A logical product inside a workspace. |
| Repository | One Git repository or one plain folder that ZCIE indexes. |
| Worktree | One checked-out folder of a repository. A repository has one or more worktrees. |
| Scope | The set (tenant, principal, workspace, project, repository, worktree) that limits a request. |
| Event | One immutable record of something that occurred. Example: a user message. |
| Envelope | The metadata part of an event. The payload is stored separately. |
| Payload | The original content of an event. Stored in the object store. |
| Idempotency key | A client-supplied key. Two events with the same key are one event. |
| Stream | An ordered sequence of events from one source session. |
| Sequence number (seq) | A monotonic counter. Each event has one per tenant. |
| Object store | Folder of compressed, content-addressed blobs. |
| Blob | One immutable byte sequence in the object store. |
| Content hash | The BLAKE3 hash of a blob before compression. Format: `b3:<64 hex chars>`. |
| Raw memory | All captured events and payloads, without change. |
| Structured memory | Records derived from raw memory: facts, tasks, decisions and others. |
| Memory record | One structured memory item. |
| Entity | Any stored record with an ID: file, symbol, task, memory record, event. |
| Edge | One typed, directed relation between two entities. |
| Edge status | The trust level of an edge: `verified`, `inferred`, `candidate`, `rejected`. |
| Evidence | The source that supports an edge or a memory record. |
| Provenance | The list of source event IDs and revisions for a record. |
| Symbol | A named code declaration. Examples: function, struct, class, method. |
| Span | A line range `[start_line, end_line]` in a file. Lines start at 1. |
| Revision | A Git commit SHA, or a content hash for a file with no Git data. |
| Job | One durable unit of background work. |
| Worker | A background task that executes jobs. |
| Watermark | The highest sequence number that an index has fully processed. |
| Live overlay | The set of changed files that the indexes did not process yet. |
| Freshness | A query result label: `current`, `stale`, `partial`. |
| Capsule | A compact, cited answer block for a model. |
| Token budget | The maximum number of model tokens in a response. |
| Token firewall | The component that applies the token budget to model-facing responses. |
| Ref | A short stable handle to an entity. Example: `sym:01J...`. |
| L0 cache | The in-process memory cache of the daemon. |

## 2. Abbreviations

Only these abbreviations are permitted in names and text.

| Abbreviation | Meaning |
|--------------|---------|
| ID | Identifier |
| CF | RocksDB column family |
| KV | Key-value |
| AST | Abstract syntax tree |
| LSP | Language Server Protocol |
| MCP | Model Context Protocol |
| API | Application programming interface |
| SDK | Software development kit |
| IPC | Inter-process communication |
| ACL | Access control list |
| ADR | Architecture decision record |
| DTO | Data transfer object |
| UTC | Coordinated Universal Time |
| ULID | Universally unique lexicographically sortable identifier |
| p95 | 95th percentile |
