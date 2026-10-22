# ZeroContext Intelligence Engine (ZCIE)

ZCIE is a local, model-independent memory and code intelligence engine. It captures authorized AI-client events
(user messages, file changes, commits, test results), indexes code repositories (files, symbols, commits) and relates
both in a durable knowledge graph. Any client — Claude Code, Cursor, Codex CLI, OpenCode, custom agents, IDE plugins — can query it through
HTTP, the Rust SDK or MCP. Indexing, linking and exact queries use no language model.

```
user message "#89 fix attachToBone" ──REQUESTED_BY── task #89 ──IMPLEMENTS (commit trailer)── commit abc123
          │                                            │                                         │
     AFFECTS_FILE (same turn)                     TESTED_BY (test.result)                 CHANGED_IN_COMMIT
          ▼                                            ▼                                         ▼
  src/animation/weapon-socket.ts ──DEFINES_SYMBOL── WeaponSocket.attachToBone        test event (passed)
```

## Install

Supported systems: macOS on Apple Silicon and Windows 11 x64.

| | macOS | Windows 11 |
|---|---|---|
| Build tools | Xcode Command Line Tools (`xcode-select --install`) | Visual Studio Build Tools with "Desktop development with C++", and LLVM (`winget install Microsoft.VisualStudio.2022.BuildTools LLVM.LLVM`) |
| Rust | `curl https://sh.rustup.rs -sSf \| sh` | `winget install Rustlang.Rustup` |
| Node.js and npm | `brew install node` | `winget install OpenJS.NodeJS.LTS` |
| Hook tools | `brew install jq` (curl is built in) | `winget install Git.Git jqlang.jq` (Git Bash runs the hooks; curl is built in) |

Prebuilt packages: each CI run on GitHub uploads `zcie-macOS` and `zcie-Windows` (`dist/zcie-<version>.tgz`) as artifacts. With a package, skip the build step and run `npm install -g ./zcie-<version>.tgz`.

### 1. Build and install the engine

macOS (Terminal):

```sh
git clone https://github.com/kocyigitkim/zcie.git && cd zcie
scripts/packaging/build-npm.sh             # builds the release binaries and makes dist/zcie-<version>.tgz
npm install -g ./dist/zcie-0.1.0.tgz       # puts zcie on PATH (zcied and zcie-mcp are inside the package)
zcie setup                                 # wizard: settings, daemon, autostart, client integrations, end-to-end check
```

Windows 11 (Git Bash; open "Git Bash" from the Start menu):

```sh
git clone https://github.com/kocyigitkim/zcie.git && cd zcie
export LIBCLANG_PATH="/c/Program Files/LLVM/bin"   # RocksDB needs libclang
scripts/packaging/build-npm.sh
npm install -g ./dist/zcie-0.1.0.tgz
zcie setup
```

After the install, `zcie` works in PowerShell, cmd and Git Bash. If Git is not in `C:\Program Files\Git`, set `ZCIE_BASH` to the full path of `bash.exe`.

`zcie setup` asks for each client it finds on this computer. To install without questions, give the answers as flags, for example `zcie setup -y --claude user --cursor true --codex true`.

### 2. Index your projects

Only added folders are indexed. Run this once in each project:

```sh
cd /path/to/project
zcie project add                           # writes .claude/zcie_id, indexes the folder, then watches it
```

All clients share the memory of a project through `.claude/zcie_id`.

### 3. Connect your AI client

Each row is independent. "All projects" commands change user-level files; "one project" commands change files in the project folder.

The commands are the same on macOS and Windows.

| Client | All projects | One project | Then |
|---|---|---|---|
| **Claude Code** | `zcie config set claude user` | `zcie claude setup --scope project --dir <project>` | Add the MCP server (below). Start a new Claude Code session. |
| **Cursor** (app and `cursor-agent` CLI) | `zcie config set cursor true` | `zcie cursor setup --dir <project>` | Restart Cursor or open a new chat. In Settings → MCP, turn on `zcie` if it asks. |
| **Codex CLI** | `zcie config set codex true` | — | Start `codex` and trust the ZCIE hooks when it asks (or run `/hooks`). |
| **OpenCode** | `zcie config set opencode true` | `zcie opencode setup --dir <project>` | Restart OpenCode. |

Claude Code MCP server (once, for all projects):

```sh
claude mcp add --scope user zcie \
  -e ZCIE_URL=http://127.0.0.1:47831 \
  -e ZCIE_API_KEY_FILE="$HOME/.zcie/config/claude-code.key" \
  -- "$(npm root -g)/zcie/native/darwin-arm64/zcie-mcp"   # from source: $PWD/target/release/zcie-mcp
```

On Windows (PowerShell):

```powershell
claude mcp add --scope user zcie `
  -e ZCIE_URL=http://127.0.0.1:47831 `
  -e ZCIE_API_KEY_FILE="$env:USERPROFILE\.zcie\config\claude-code.key" `
  -- "$(npm root -g)\zcie\native\win32-x64\zcie-mcp.exe"
```

Cursor, Codex and OpenCode get the MCP server from the commands in the table.

What each integration installs:

| Client | Files | Context in each prompt | Memory capture | MCP tools |
|---|---|---|---|---|
| Claude Code | `~/.claude/settings.json` (hooks, env, tool profile) | yes | prompts, answers, edits, commands | `zcie_query`, `zcie_expand` |
| Cursor | `~/.cursor/hooks.json`, `~/.cursor/mcp.json` | yes (app); the CLI gets the session rule and MCP | prompts, answers, edits (CLI: replayed at session end) | same |
| Codex CLI | `~/.codex/hooks.json`, `codex mcp add zcie` | yes | prompts, answers | same |
| OpenCode | `~/.config/opencode/plugins/zcie.js`, `opencode.json` | yes | prompts, answers, edits | same |

### 4. Check, change, remove

```sh
zcie verify                                # checks the daemon, the index, the hooks of each enabled client
zcie verify --live                         # also runs two real Claude Code sessions
zcie config show                           # all settings
zcie config set budget 1500                # change a setting; it is applied at once
zcie cursor remove --global                # remove one client (also: zcie codex remove, zcie opencode remove --global)
zcie uninstall                             # remove all client entries and the autostart agent, stop the daemon; the data stays
```

`~` is `%USERPROFILE%` on Windows (for example `C:\Users\you\.cursor\hooks.json`). The Windows autostart is a script in the Startup folder of the user; it starts the daemon at login, but it does not restart it after a crash (run `zcie daemon start`).

Details: [npm/zcie/README.md](npm/zcie/README.md), [ADR-014](docs/adr/ADR-014-npm-install-and-setup.md), [ADR-025 (Cursor)](docs/adr/ADR-025-cursor-adapter.md), [ADR-026 (Codex)](docs/adr/ADR-026-codex-adapter.md), [ADR-027 (Windows)](docs/adr/ADR-027-windows-support.md).

## Quick start (from source)

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
| `adapters/` | Claude Code hooks, Cursor and Codex hook wrappers, OpenCode plugin, Git post-commit hook, example client. |
| `docs/` | Specification, development plan, test plan, acceptance criteria, ADRs. |

## Specification

Start at [docs/README.md](docs/README.md).
