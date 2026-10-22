# ADR-025 — Cursor adapter

- Status: Accepted
- Date: 2026-10-22
- Tasks: D-087 of `docs/12-development-plan.md`

## Context
Only Claude Code and OpenCode use ZCIE. Cursor 3.x runs hooks and can return `additional_context`. Cursor also reads an MCP server config. Cursor runs the Claude Code hooks of a project by default. ZCIE must not run its Claude Code hooks inside Cursor. This decision is pinned against Cursor 3.25.6.

## Decision
ZCIE SHALL connect Cursor with one dispatcher hook and the `zcie-mcp` server.

1. Project id. Cursor reuses `.claude/zcie_id`. It does not create `.cursor/zcie_id`.
2. Dispatcher. `adapters/cursor/cursor-hook.sh` gives the Cursor input the fields of the Claude Code input. It runs the Claude Code hook scripts of the event. It prints Cursor JSON.
3. Guard. `zcie_read_input` exits 0 when the input has `cursor_version` and `ZCIE_APP` is not `cursor`.
4. App tag. `zcie_send` tags each event with `ZCIE_APP`. The default is `claude-code`.
5. MCP project. `zcie-mcp` reads `ZCIE_PROJECT_DIR`. Without it, the server uses the working folder. A project install writes the absolute path. A global install writes `${workspaceFolder}`.
6. Rules. No `.mdc` rule file. The `sessionStart` output carries the rule line for `zcie_query`.
7. Removal. There is no manifest. Removal matches the hook command path and the `mcpServers.zcie` entry.
8. Settings. The dispatcher reads the key from `~/.zcie/config/claude-code.key`. It reads the URL and budget from `~/.zcie/config/client.json`. It adds the Homebrew folders to `PATH`.

## Consequences
- Cursor and Claude Code share one project memory.
- The capsule rules stay in one place, the Claude Code hook scripts.
- The ZCIE Claude Code hooks do nothing inside Cursor.
- Cursor has no tool profile, no guards, no verification gate, no subagent context and no usage report.
- A global MCP entry depends on Cursor expanding `${workspaceFolder}` in `env`. This is not yet confirmed.
- Cursor CLI (cursor-agent 2026.10.19) fires no beforeSubmitPrompt or afterAgentResponse. At sessionEnd the dispatcher replays transcript_path (user_query text as the prompt, the last assistant text of each turn as the answer) when the session has no turn file. A GUI session or a second sessionEnd replays nothing.
