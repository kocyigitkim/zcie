# ZCIE Documentation

ZeroContext Intelligence Engine (ZCIE) — Engineering Specification, Version 2.1
Status: Approved for implementation. Language: STE-100-based technical English.

## 1. What ZCIE is

ZCIE is a local, model-independent memory and code intelligence engine.
It captures authorized user events, indexes code repositories and relates both in a knowledge graph.
Any AI client can query ZCIE: Claude Code, OpenCode, custom agents, IDE plugins and local LLMs.
ZCIE does indexing, search and linking without a paid language model.

## 2. Reading order

Read the documents in this order before you start task `D-001`.

| # | Document | Content |
|---|----------|---------|
| 1 | [00-agent-rules.md](00-agent-rules.md) | Work order, code rules, writing rules, approved crates. |
| 2 | [01-overview.md](01-overview.md) | Goals, principles, scope, exclusions. |
| 3 | [02-glossary.md](02-glossary.md) | Terms and abbreviations. |
| 4 | [03-architecture.md](03-architecture.md) | Processes, crates, dependency direction, data flow. |
| 5 | [04-folder-structure.md](04-folder-structure.md) | Source tree and runtime tree. |
| 6 | [05-data-model.md](05-data-model.md) | IDs, events, memory, code entities, edges, keys. |
| 7 | [06-storage.md](06-storage.md) | RocksDB column families, object store, durability, recovery. |
| 8 | [07-ingest-and-indexing.md](07-ingest-and-indexing.md) | Write path, jobs, file scan, Git, parser, search index. |
| 9 | [08-memory-and-graph.md](08-memory-and-graph.md) | Raw memory, structured memory, extraction, linking. |
| 10 | [09-query-and-context.md](09-query-and-context.md) | Read path, query classes, cache, token budget, freshness. |
| 11 | [10-api-and-gateways.md](10-api-and-gateways.md) | Core API, HTTP routes, MCP tools, CLI, SDKs, adapters. |
| 12 | [11-security.md](11-security.md) | Scope checks, redaction, keys, deletion. |
| 13 | [15-configuration.md](15-configuration.md) | Configuration files and default values. |
| 14 | [12-development-plan.md](12-development-plan.md) | Development tasks `D-001` to the last task. |
| 15 | [13-test-plan.md](13-test-plan.md) | Tests. Start ONLY after all development tasks are complete. |
| 16 | [14-acceptance-criteria.md](14-acceptance-criteria.md) | Acceptance. Examine ONLY after all tests pass. |
| 17 | [adr/](adr/) | Architecture decision records. ADR-008 records the implementation decisions of milestone 1. ADR-009 defines the central project memory. |
| 18 | [operations/quickstart.md](operations/quickstart.md) | Build, run and connect clients. |

## 3. Project phases

```
Development phases (D)  ->  Test phase (T)  ->  Acceptance phase (A)
 D-001 ... D-last            write + run all tests    examine all criteria
 build + clippy only         fix code until green     record pass / fail
```

> **Warning:** Do not write tests in the development phases.
> Write and run all tests in the test phase only, after the last development task.
> Examine the acceptance criteria only after all tests pass.

## 4. Fixed decisions

These decisions apply to all documents. Do not change them without a new ADR.

| ID | Decision |
|----|----------|
| F-01 | The core engine is Rust. Other languages are used only in SDKs and adapters. |
| F-02 | One daemon process (`zcied`) owns the database. It is the single writer. A lock file prevents a second daemon. |
| F-03 | Clients never open RocksDB. Clients use the HTTP gateway (`127.0.0.1:47831`) or the MCP gateway. |
| F-04 | RocksDB is the canonical store. Tantivy and vector indexes are derived and can be rebuilt. |
| F-05 | Stored values are JSON (`serde_json`) with a version field `"v"`. |
| F-06 | Git data comes from the `git` command-line program. |
| F-07 | The first milestone parses Rust, TypeScript and Python. |
| F-08 | The MCP gateway exposes two tools only: `zcie_query` and `zcie_expand`. |
| F-09 | No language model is called for indexing, extraction or exact queries. Tier B and Tier C extraction are out of scope for the first milestone. |
| F-10 | The first milestone has one tenant: `local`. All keys still contain the tenant ID. |
| F-11 | Vector search, gRPC, LSP enrichment and the inspection UI are optional. They are not in the first milestone. |
| F-12 | HTTP requests carry the API key in the header `Authorization: Bearer <key>`. |

## 4.1 First milestone (definition)

The first milestone is complete when the engine can do all of these items without a language model:

1. Capture events durably and without duplicates.
2. Index one Git repository incrementally (files, symbols, commits).
3. Link one user request to a task, to code symbols and to a verified commit.
4. Answer an exact lookup through the HTTP API, the Rust SDK and the MCP gateway.
5. Recover after a crash and rebuild the search index from RocksDB.
