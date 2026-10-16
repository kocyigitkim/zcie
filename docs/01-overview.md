# 01 — Overview

Status: Normative.

## 1. Purpose

ZCIE is a local engine for AI memory and code intelligence.
It keeps a durable record of authorized user events and the code of indexed repositories.
It relates user requests, tasks, code symbols, commits and test results in a knowledge graph.
Any AI client can query this graph through a stable API.
The model does not have to read the same project again in each session.

Main success criterion: A client can find the relation between an earlier user request and the current code. The result shows the source and the validity of the relation. This MUST work from each supported client.

## 2. Goals

| ID | Goal | Meaning |
|----|------|---------|
| G-1 | Universal AI memory | Claude Code, OpenCode, custom agents and custom applications use the same memory. |
| G-2 | Complete project indexing | ZCIE indexes source files, symbols, dependencies, documents, Git history and authorized conversations. |
| G-3 | Relational intelligence | ZCIE keeps durable relations between requirements, code, tests, failures and decisions. |
| G-4 | Fast retrieval | Repeated queries use the L0 cache. Other queries use local indexes. |
| G-5 | Minimal model tokens | Search, indexing, memory update and linking run without a language model where possible. |

## 3. Principles

| ID | Principle |
|----|-----------|
| P-001 | Engine independence. The core SHALL NOT depend on Claude Code, an Anthropic API, a specific model, MCP or one IDE. |
| P-002 | Full authorized capture. Store each authorized event without change in raw memory. Structured memory is a separate, derived record. Never replace a source record with a summary. |
| P-003 | Local execution. Do discovery, hashing, parsing, indexing, graph traversal, ranking, caching and exact lookups locally. These operations SHALL NOT need a paid model. |
| P-004 | Evidence. Each memory record and each edge SHALL keep a source ID and a revision. Do not change a hypothesis into a verified fact without verification. |
| P-005 | Minimal context. Return refs and short facts first. Return source code only when necessary. Let the client expand refs step by step. |
| P-006 | Fast durable writes. Acknowledge a captured event only after the durability barrier. Derived indexes are asynchronous and recoverable. |
| P-007 | Freshness. Detect changes. Keep index watermarks. Do not return stale content as current. Read the live source if necessary. |
| P-008 | Security. Give each record a scope. A retrieval or a cache hit SHALL NOT bypass the scope check. |
| P-009 | User control. Support capture opt-in, exclusions, private workspaces, inspection, correction, export and deletion. Exclude or redact secrets. |
| P-010 | Truthful performance. All latency, recall and token numbers are targets until a benchmark measures them. |

## 4. Clients

- Claude Code, OpenCode, custom agents, IDE plugins, CLI programs, desktop applications, local LLM runtimes.
- First milestone protocols: HTTP/JSON (loopback), MCP stdio, Rust SDK.
- Later protocols (OPTIONAL): MCP Streamable HTTP, gRPC, Unix socket IPC, TypeScript SDK, Python SDK.
- Each client SHALL send an API key. The key identifies the principal and the permitted workspaces.
- ZCIE cannot read earlier conversations of a client that had no capture adapter. Only captured or imported events are indexed.

## 5. Source types

| # | Source type | First milestone |
|---|-------------|-----------------|
| 1 | Git tracked files, untracked files that are not ignored, commits, changed paths. | Yes |
| 2 | AST symbols (Rust, TypeScript, Python), imports. | Yes |
| 3 | Markdown, plain text and configuration files (full-text only). | Yes |
| 4 | User messages, tool calls, tool results, command runs, test results. | Yes |
| 5 | Assistant messages. | Yes, if `capture.assistant_messages = true` |
| 6 | Decisions, requirements, tasks, corrections (Tier A extraction). | Yes |
| 7 | Branches, tags, blame, worktree overlays. | OPTIONAL |
| 8 | PDF, Office documents, logs, traces, issue trackers. | OPTIONAL |
| 9 | Binary files. | Metadata and hash only |

## 6. First milestone scope

The first milestone is defined in `docs/README.md` section 4.1.
These items are NOT in the first milestone:

- Vector search (`zcie-vector`).
- LSP enrichment (`zcie-lsp`).
- gRPC gateway, IPC gateway and Streamable HTTP for MCP.
- Inspection UI (`apps/zcie-inspect`).
- Tier B (local model) and Tier C (remote model) extraction.
- TypeScript SDK, Python SDK, OpenCode adapter and IDE plugins.
- Multi-tenant deployment. The tenant is always `local`.
- Encryption at rest.

## 7. Limits and caveats

- ZCIE cannot answer all questions immediately. Exact lookups are local and fast. New reasoning still needs a model.
- "Full indexing" applies only to authorized, supported sources. Binary files and remote applications need extractors or adapters.
- The index size increases with source volume and retention. Use retention policies.
- Durable writes cost write latency. ZCIE does not give an atomic transaction across RocksDB and Tantivy.
- A symbol ID is stable only on a best-effort basis across edits and moves.
- A local model for extraction (OPTIONAL) uses CPU or GPU. It can make the engine slower.
- Token estimate rule for all documents: `tokens = ceil(chars / 4)`.
