# ADR-012 — TypeScript and Python SDKs, OpenCode adapter

- Status: Accepted
- Date: 2026-10-16

## Context
ZCIE is model-independent (`docs/01-overview.md`), but only Claude Code, the Rust SDK, the CLI and MCP clients used it. `docs/12-development-plan.md` section 8 names TypeScript and Python SDKs and OpenCode and IDE adapters as later work.

## Decision

### 1. SDKs
| SDK | Folder | Runtime | Dependencies | Form |
|-----|--------|---------|--------------|------|
| TypeScript / JavaScript | `sdk/typescript` (`@zcie/client`) | Node 18+, Bun, Deno | none (global `fetch`) | ESM JavaScript `src/index.js` + types `src/index.d.ts`; no build step |
| Python | `sdk/python` (`zcie`) | Python 3.9+ | none (standard library) | responses are dicts with the JSON fields of the API |

- Both have the same operations as the Rust SDK: health, capture, repositories, query (with `scope` and `cross_project`), expand, entity, timeline, tasks, links, memory correction, forget, search rebuild and projects (ensure, list, get, rename, delete).
- `Client.fromEnv()` / `Client.from_env()`: `ZCIE_URL`, `ZCIE_API_KEY` or `ZCIE_API_KEY_FILE` (default `~/.zcie/config/admin.key`), and the project from `ZCIE_WORKSPACE` or `.claude/zcie_id` of the current folder or a parent folder (ADR-009).
- Errors: `ZcieError` with `status` (0 for a transport error) and the API error `code`.

### 2. OpenCode adapter
- One plugin file `adapters/opencode/zcie.js` (OpenCode plugin API, `@opencode-ai/plugin` 1.18): no dependencies, one export (OpenCode calls each exported function as a plugin).
- It does the work of the Claude Code hooks with the same rules:
  - project: `.claude/zcie_id` (the same file, so Claude Code and OpenCode share one memory); the plugin makes the project in a folder with no ID file (not in the home folder);
  - capture: `chat.message` → `user.message`; `tool.execute.after` of `edit`, `write`, `patch`, `multiedit` → `file.changed`;
  - context: `chat.message` adds the capsule as a `synthetic` text part of the user message, with the same gate (answer, history class, symbol name, other project, `match_coverage ≥ 0.6`) and the same filters; each ref once per session; `session.compacted` resets the list.
  - Each call has a 2 s timeout and ignores errors: the plugin never blocks or breaks a session.
- `zcie opencode setup [--dir] [--global] [--name]` installs the plugin (`.opencode/plugins/zcie.js` or `~/.config/opencode/plugins/`), adds `mcp.zcie` (local `zcie-mcp`) to `opencode.json`, and makes the project. A config with comments is not changed; the command prints the entry. `zcie opencode remove` removes both.

### 3. Verification
- `cargo test -p zcie-tests --test sdks` starts a daemon and runs the 4 TypeScript tests, the 4 Python tests and the 3 plugin tests against it. A missing `node` or `python3` skips that part with a printed reason.
- A real OpenCode 1.18.34 run (isolated `XDG_*` folders, a local fake OpenAI-compatible model that logs each request) showed: OpenCode loads the plugin and `mcp.zcie`; the request to the model has the ZCIE capsule in the user message and the tools `zcie_zcie_query` and `zcie_zcie_expand`.

## Consequences
- Any Node, Bun, Deno or Python program, and OpenCode, can use the central project memory.
- The capsule rules exist in two places (the shell hook and the plugin). A change of the rules must change both; TS-15 and TS-11-09 test both.
