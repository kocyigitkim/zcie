# 04 — Folder Structure

Status: Normative.

## 1. Rule

Keep source code and runtime data separate.
Source code is in the repository. Runtime data is in `~/.zcie/`.
Do not commit runtime data, index files or build output to Git.

## 2. Source tree

The folders exist. Each folder has a `README.md` with its purpose. Add files only as the development plan tells you.

```
ZCIEngine/
├── Cargo.toml                 # workspace; members listed in section 3
├── Cargo.lock
├── rust-toolchain.toml        # channel = "stable"
├── README.md
├── .gitignore
├── config/
│   ├── default.toml           # default engine.toml (docs/15-configuration.md)
│   └── policies/
│       ├── capture.toml
│       ├── indexing.toml
│       ├── security.toml
│       └── context.toml
├── crates/                    # library crates (docs/03-architecture.md)
│   ├── zcie-types/
│   ├── zcie-telemetry/
│   ├── zcie-security/
│   ├── zcie-store/
│   ├── zcie-runtime/
│   ├── zcie-fs/
│   ├── zcie-git/
│   ├── zcie-parse/
│   ├── zcie-symbols/
│   ├── zcie-graph/
│   ├── zcie-memory/
│   ├── zcie-search/
│   ├── zcie-cache/
│   ├── zcie-ingest/
│   ├── zcie-query/
│   ├── zcie-context/
│   ├── zcie-api/
│   ├── zcie-core/
│   ├── zcie-lsp/              # OPTIONAL, folder only
│   └── zcie-vector/           # OPTIONAL, folder only
├── apps/
│   ├── zcied/                 # daemon binary
│   ├── zcie/                  # CLI binary
│   └── zcie-inspect/          # OPTIONAL, folder only
├── gateways/
│   ├── http/                  # crate zcie-gateway-http
│   ├── mcp/                   # binary crate zcie-mcp
│   ├── grpc/                  # OPTIONAL, folder only
│   └── ipc/                   # OPTIONAL, folder only
├── adapters/
│   ├── claude-code/
│   │   ├── hooks/             # hook scripts that call the CLI or HTTP
│   │   └── skills/            # OPTIONAL
│   ├── git/                   # post-commit hook script
│   ├── filesystem/            # notes only
│   ├── custom-example/        # example HTTP client script
│   ├── opencode/              # OPTIONAL
│   └── ide/{vscode,jetbrains} # OPTIONAL
├── sdk/
│   ├── rust/                  # crate zcie-sdk
│   ├── typescript/            # OPTIONAL
│   └── python/                # OPTIONAL
├── proto/zcie/v1/             # OPTIONAL (gRPC)
├── migrations/{kv,search}/    # schema migration notes and code
├── tests/                     # test phase only (docs/13-test-plan.md)
│   ├── unit/
│   ├── integration/
│   ├── crash-recovery/
│   ├── multitenancy/
│   ├── stale-index/
│   ├── token-regression/
│   └── fixtures/
├── benchmarks/                # after the test phase
│   ├── corpus-generator/
│   ├── ingestion/
│   ├── queries/
│   └── end-to-end/
├── scripts/{dev,packaging,migration}/
└── docs/
    ├── README.md, 00 ... 15
    └── adr/
```

## 3. Workspace members (milestone 1)

Put these paths in `[workspace] members` in the root `Cargo.toml`:

```
crates/zcie-types, crates/zcie-telemetry, crates/zcie-security, crates/zcie-store,
crates/zcie-runtime, crates/zcie-fs, crates/zcie-git, crates/zcie-parse,
crates/zcie-symbols, crates/zcie-graph, crates/zcie-memory, crates/zcie-search,
crates/zcie-cache, crates/zcie-ingest, crates/zcie-query, crates/zcie-context,
crates/zcie-api, crates/zcie-core, gateways/http, gateways/mcp, sdk/rust,
apps/zcied, apps/zcie
```

Add a crate to the list only when its development task creates its `Cargo.toml`.

## 4. Files inside a crate

Each library crate has this minimum layout:

```
crates/zcie-<name>/
├── Cargo.toml
├── README.md
└── src/
    ├── lib.rs        # module list and public re-exports only
    ├── error.rs      # one Error enum (thiserror) and `pub type Result<T>`
    └── limits.rs     # constants for this crate (rule C-17), if any
```

Expected modules per crate:

| Crate | Modules in `src/` |
|-------|-------------------|
| `zcie-types` | `ids.rs`, `scope.rs`, `event.rs`, `memory.rs`, `code.rs`, `task.rs`, `edge.rs`, `query.rs`, `error.rs` |
| `zcie-telemetry` | `logging.rs`, `timer.rs` |
| `zcie-security` | `auth.rs`, `scope_check.rs`, `redact.rs`, `exclude.rs` |
| `zcie-store` | `rocksdb/mod.rs`, `rocksdb/column_families.rs`, `keys.rs`, `objects/mod.rs`, `watermarks.rs`, `recovery/mod.rs` |
| `zcie-runtime` | `queue.rs`, `workers.rs`, `shutdown.rs` |
| `zcie-fs` | `discover.rs`, `hash.rs`, `binary.rs`, `watcher.rs` |
| `zcie-git` | `command.rs`, `head.rs`, `commits.rs`, `trailers.rs` |
| `zcie-parse` | `pool.rs`, `grammars/mod.rs`, `extractors/rust.rs`, `extractors/typescript.rs`, `extractors/python.rs` |
| `zcie-symbols` | `index.rs`, `identity.rs` |
| `zcie-graph` | `edges.rs`, `status.rs`, `traverse.rs` |
| `zcie-memory` | `capture/mod.rs`, `extraction/tier_a.rs`, `provenance/mod.rs`, `conflict/mod.rs`, `retention/mod.rs`, `tasks.rs`, `timeline.rs` |
| `zcie-search` | `lexical/schema.rs`, `lexical/writer.rs`, `lexical/reader.rs`, `ranking/mod.rs`, `freshness/mod.rs` |
| `zcie-cache` | `l0.rs`, `cache_key.rs` |
| `zcie-ingest` | `capture.rs`, `dedupe/mod.rs`, `queues/mod.rs`, `processors/{message.rs,file.rs,commit.rs,test_result.rs}` |
| `zcie-query` | `classify.rs`, `planner.rs`, `routes.rs` |
| `zcie-context` | `capsule.rs`, `budget.rs`, `refs.rs` |
| `zcie-api` | `engine.rs`, `requests.rs`, `responses.rs` |
| `zcie-core` | `config.rs`, `startup.rs` |

The folders `crates/zcie-store/src/{rocksdb,objects,recovery}` and other subfolders already exist. Use them.

## 5. `.gitignore`

The root `.gitignore` SHALL contain these lines:

```
/target/
**/*.rs.bk
.zcie/
*.sst
*.log
.env
.env.*
.DS_Store
```

## 6. Runtime tree

The daemon creates this tree at startup if it does not exist. The tree is never in Git.

```
~/.zcie/                               0700
├── config/                            0700
│   ├── engine.toml                    0600  (OPTIONAL; defaults apply)
│   ├── clients.toml                   0600
│   └── policies/                      0700
├── run/                               0700
│   ├── zcied.pid                      0600
│   └── daemon.lock                    0600
├── instances/
│   └── local/                         0700  (= storage.data_dir)
│       ├── manifest.json              0600  schema version, created time
│       ├── state/
│       │   ├── rocksdb/               canonical store
│       │   ├── search/                derived Tantivy indexes
│       │   │   ├── code/
│       │   │   ├── documents/
│       │   │   └── conversations/
│       │   ├── vectors/               OPTIONAL
│       │   ├── objects/ab/cd/<hex>.zst   blobs
│       │   └── checkpoints/           RocksDB checkpoints (backups)
│       ├── queue/dead-letter/         JSON files of failed jobs
│       ├── cache/                     OPTIONAL disk cache
│       ├── backups/
│       └── telemetry/logs/
└── exports/                           output of `zcie export`
```

Rules:

- Create all folders with mode `0700`. Create all files with mode `0600`.
- The IPC socket `run/zcied.sock` is OPTIONAL. It is not in milestone 1.
- Expand `~` with the `HOME` environment variable.
- `manifest.json` content: `{"v":1,"schema_version":1,"instance":"local","created_utc":"<RFC 3339>"}`.
- If `schema_version` in `manifest.json` is higher than the daemon supports, the daemon SHALL stop with an error.
