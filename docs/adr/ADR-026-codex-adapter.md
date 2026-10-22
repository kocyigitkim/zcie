# ADR-026 — Codex CLI adapter

- Status: Accepted
- Date: 2026-10-22
- Tasks: D-088 of `docs/12-development-plan.md`

## Context
Codex CLI 0.163 runs hooks from `~/.codex/hooks.json`. The hook events, the hook input and the context output have the form of Claude Code hooks (`SessionStart`, `UserPromptSubmit`, `Stop`, `SessionEnd`; `hookSpecificOutput.additionalContext`). Codex runs a new hook only after the user trusts it. Codex does not run the Claude Code hooks of `~/.claude`.

## Decision
ZCIE SHALL connect Codex CLI with the Claude Code hook scripts and the `zcie-mcp` server.

1. Wrapper. `adapters/codex/codex-hook.sh <script>` sets `ZCIE_APP=codex`, the key file, the URL and the budget. It runs the Claude Code script. It gives plain context text the Codex JSON form.
2. Hooks. `zcie codex setup` writes one matcher group per script to `~/.codex/hooks.json`: session start and repository map, prompt capture and capsule, answer capture at `Stop`, session end. Other groups stay.
3. MCP. `zcie codex setup` runs `codex mcp add zcie`. The server finds the project from its working folder.
4. Trust. ZCIE does not write the trust state of Codex. The user trusts the hooks once in Codex (`/hooks`).
5. Settings. `zcie setup --codex true` and `zcie config set codex true` install the hooks; `zcie uninstall` removes them; `zcie verify` checks them.

## Consequences
- Codex, Cursor and Claude Code share one project memory through `.claude/zcie_id`.
- Codex file edits are not captured by a hook. The file watcher of the daemon indexes them.
- A change of a hook command needs a new trust in Codex.
