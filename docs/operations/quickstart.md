# Quickstart

Status: Informative. All commands exist in the `zcie` CLI.

## 1. Requirements

- Rust stable (installed by `rustup`; the repository has `rust-toolchain.toml`).
- macOS: Xcode Command Line Tools (`xcode-select --install`). `rocksdb` compiles C++ code and `bindgen` needs `libclang`.
  The repository file `.cargo/config.toml` points `LIBCLANG_PATH` to `/Library/Developer/CommandLineTools/usr/lib`.
- Linux: `clang` and `libclang-dev`. Set `LIBCLANG_PATH` (for example `/usr/lib/llvm-18/lib`).
- `git` (for Git features), `curl` and `jq` (for the Claude Code hooks).

## 2. Build

```sh
cargo build --workspace --release
```

The first build compiles RocksDB. It takes several minutes. The binaries are in `target/release/`:
`zcied` (daemon), `zcie` (CLI), `zcie-mcp` (MCP stdio gateway).

Put them on your `PATH`:

```sh
export PATH="$PWD/target/release:$PATH"
```

## 3. First run

1. Start the daemon. It creates `~/.zcie/` with mode 0700.

   ```sh
   zcie daemon start
   ```

   On the first start the daemon makes an admin API key. It prints the key once in its log and stores it in
   `~/.zcie/config/admin.key` (mode 0600). The CLI uses this file when `--key` and `ZCIE_API_KEY` are not set.

2. Check the daemon:

   ```sh
   zcie health
   ```

3. Register a repository in a workspace:

   ```sh
   zcie --workspace myproject repo add /path/to/repo
   ```

   The daemon scans the repository, parses Rust, TypeScript and Python files, indexes the last 200 commits and
   starts a file watcher.

4. Ask questions:

   ```sh
   zcie --workspace myproject query "where is parse_config defined"
   zcie --workspace myproject query "which commit implemented #42"
   zcie --workspace myproject expand sym:sym_01... --level source
   ```

5. Stop the daemon:

   ```sh
   zcie daemon stop
   ```

## 4. API keys for clients

Make one key per client. The CLI prints the key one time and stores only its hash in `~/.zcie/config/clients.toml`.

```sh
zcie key create --name claude-code --workspaces myproject --permissions read,write
kill -HUP "$(cat ~/.zcie/run/zcied.pid)"   # the daemon reloads clients.toml
```

## 5. Claude Code

The fastest way is the automatic setup. It configures the lean tool profile and the ZCIE hooks:

```sh
cd /path/to/project && zcie claude setup
```

See `adapters/claude-code/install.md`. The manual steps below are an alternative.

1. Add the MCP gateway (two tools: `zcie_query`, `zcie_expand`):

   ```sh
   claude mcp add zcie -e ZCIE_API_KEY=<key> -e ZCIE_WORKSPACE=myproject -- zcie-mcp
   ```

2. Add the capture hooks. See `adapters/claude-code/install.md`. The hooks never block Claude Code: if the daemon
   does not run, they exit with code 0.

3. Link commits to tasks: add a trailer line to the commit message, for example `ZCIE-Task: #42`.
   Install the Git hook from `adapters/git/install.md` so the daemon receives each commit at once.

## 6. Files

| Path | Content |
|------|---------|
| `~/.zcie/config/engine.toml` | OPTIONAL configuration. See `config/default.toml`. |
| `~/.zcie/config/clients.toml` | API key hashes and permissions. |
| `~/.zcie/instances/local/state/` | RocksDB, Tantivy indexes, blobs. |
| `~/.zcie/instances/local/telemetry/logs/` | Daemon logs. |
| `~/.zcie/run/` | PID file and the single-writer lock. |

## 7. Troubleshooting

| Problem | Action |
|---------|--------|
| `Library not loaded: @rpath/libclang.dylib` during the build | Install Xcode Command Line Tools, or set `LIBCLANG_PATH` to the folder with `libclang.dylib`. |
| `another zcied holds .../daemon.lock` | A daemon runs. Use `zcie daemon stop`. |
| Search results lag behind | `zcie health` shows the watermarks. `zcie rebuild-search` rebuilds the search index. |
| A job failed three times | See `~/.zcie/instances/local/queue/dead-letter/`. |
