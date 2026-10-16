# ZeroContext Intelligence Engine (ZCIE)

ZCIE is a local, model-independent memory and code intelligence engine. It captures authorized AI-client events
(user messages, file changes, commits, test results), indexes code repositories (files, symbols, commits) and relates
both in a durable knowledge graph. Any client — Claude Code, OpenCode, custom agents, IDE plugins — can query it through
HTTP, the Rust SDK or MCP. Indexing, linking and exact queries use no language model.

```
user message "#89 fix attachToBone" ──REQUESTED_BY── task #89 ──IMPLEMENTS (commit trailer)── commit abc123
          │                                            │                                         │
     AFFECTS_FILE (same turn)                     TESTED_BY (test.result)                 CHANGED_IN_COMMIT
          ▼                                            ▼                                         ▼
  src/animation/weapon-socket.ts ──DEFINES_SYMBOL── WeaponSocket.attachToBone        test event (passed)
```

## Quick start

```sh
cargo build --workspace --release          # needs Xcode CLT / clang (RocksDB)
export PATH="$PWD/target/release:$PATH"
zcie daemon start
zcie --workspace demo repo add .
zcie --workspace demo query "where is start_engine defined"
```

Full steps: [docs/operations/quickstart.md](docs/operations/quickstart.md).

## Layout

| Folder | Content |
|--------|---------|
| `crates/` | Engine library crates (types, store, ingest, graph, memory, search, query, api, core, …). |
| `apps/zcied` | The daemon. The only process that opens the database. |
| `apps/zcie` | The CLI. |
| `gateways/http`, `gateways/mcp` | HTTP/JSON gateway (inside the daemon) and the MCP stdio gateway. |
| `sdk/rust` | Rust client SDK. |
| `adapters/` | Claude Code hooks, Git post-commit hook, example client. |
| `docs/` | Specification, development plan, test plan, acceptance criteria, ADRs. |

## Specification

Start at [docs/README.md](docs/README.md).
