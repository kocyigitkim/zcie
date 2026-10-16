# Claude Code adapter — install

## 0. Automatic setup (recommended)

```sh
zcie daemon start
cd /path/to/project
zcie claude setup                      # this project (.claude/settings.local.json)
zcie claude setup --scope user         # or: all projects (~/.claude/settings.json)
zcie claude status                     # show what is configured
zcie claude tools enable WebFetch      # enable a denied tool (applies at once)
zcie claude remove                     # remove exactly what setup added
```

Profiles: `--profile auto` (default: tools load automatically on demand, cost ≤ default), `--profile lean` (cheapest; extra tools must be enabled), `--profile balanced`.
Options: `--allow WebFetch,Task` (keep tools), `--no-lean`, `--no-inject`, `--no-guards`, `--no-capture`, `--cache-5m`.
Measured effect: −63 % tokens, −42 % cost, 30/30 correct (see `benchmarks/results/claude-code-ab-2026-10-16.md`).

### 0.1 Project ID (ADR-009)
`zcie claude setup` writes the project ID into `<project root>/.claude/zcie_id`. Commit this file: a clone gets the same project memory.
If a folder has no ID file, the SessionStart hook of the first Claude Code session makes the project and writes the file.
All memory is kept by this ID in the one store `~/.zcie`. A prompt that names another project (for example "how does billing compute tax") also reads that project. `zcie query --global "<question>"` reads all projects.
`--name <name>` sets the project name (default: the folder name). `zcie project list` shows all projects.
The sections below describe the manual setup.

The hooks send events to the ZCIE daemon. They use `jq` and `curl`.
A hook never blocks the session. If the daemon does not run, the hook exits with code 0 and does nothing.

## 1. Requirements

1. Install `jq` and `curl`.
2. Start the daemon: `zcie daemon start`.
3. Make a key with write permission: `zcie key create --name claude-code --permissions read,write`.
4. Restart the daemon to load the key.
5. Set these variables in your shell profile:

```sh
export ZCIE_API_KEY="zcie_..."          # the key from step 3
export ZCIE_WORKSPACE="atheron"         # optional override; default is the ID in .claude/zcie_id
export ZCIE_URL="http://127.0.0.1:47831" # optional
```

## 1.1 Project setup in this repository (recommended)

This repository already has `.mcp.json` and `.claude/settings.json`. Do these steps once:

1. Make a key file (mode 0600): `zcie key create --name claude-code --workspaces <ws>` and store the printed key in `~/.zcie/config/claude-code.key`. Send `SIGHUP` to the daemon.
2. Create `.claude/settings.local.json` (Git ignores it):

```json
{
  "env": { "ZCIE_API_KEY_FILE": "/Users/<you>/.zcie/config/claude-code.key" },
  "enabledMcpjsonServers": ["zcie"]
}
```

3. Start `claude` in the repository and approve the `zcie` MCP server once.

For other repositories, use absolute hook paths (section 2) and `claude --mcp-config`.

## 2. Hooks

Add this block to `~/.claude/settings.json` (or `.claude/settings.json` of a project).
Replace `/path/to/ZCIEngine` with the repository path.

```json
{
  "hooks": {
    "SessionStart": [
      { "hooks": [ { "type": "command", "command": "/path/to/ZCIEngine/adapters/claude-code/hooks/session-start.sh" } ] }
    ],
    "UserPromptSubmit": [
      { "hooks": [ { "type": "command", "command": "/path/to/ZCIEngine/adapters/claude-code/hooks/user-prompt-submit.sh" } ] }
    ],
    "PostToolUse": [
      { "matcher": "Edit|Write|MultiEdit|NotebookEdit|Bash",
        "hooks": [ { "type": "command", "command": "/path/to/ZCIEngine/adapters/claude-code/hooks/post-tool-use.sh" } ] }
    ],
    "SessionEnd": [
      { "hooks": [ { "type": "command", "command": "/path/to/ZCIEngine/adapters/claude-code/hooks/stop.sh" } ] }
    ]
  }
}
```

`stop.sh` sends `session.end`. Use it on `SessionEnd`. The `Stop` hook runs after each response, so it is not a session end.

## 3. MCP server

Build the binaries: `cargo build --release`. Then add the MCP server:

```sh
claude mcp add zcie \
  -e ZCIE_URL=http://127.0.0.1:47831 \
  -e ZCIE_API_KEY="$ZCIE_API_KEY" \
  -- /path/to/ZCIEngine/target/release/zcie-mcp
```

The server exposes two tools: `zcie_query` and `zcie_expand`. It finds the project from `.claude/zcie_id` in its start folder or a parent folder. `zcie_query` takes `scope: "global"` or `project: "<name>"` to read other projects.

## 4. Events sent

| Hook | Event type | Payload |
|------|------------|---------|
| `SessionStart` | `session.start` | none |
| `UserPromptSubmit` | `user.message` | the prompt text; starts a new turn |
| `PostToolUse` (`Edit`, `Write`, `MultiEdit`, `NotebookEdit`) | `file.changed` | `{"path": "<file path>"}` with the current turn |
| `PostToolUse` (`Bash`) | `command.run` | `{"command", "exit_code": null, "output"}` (4000 characters or fewer) |
| `SessionEnd` | `session.end` | none |

The idempotency key is `claude-code:<session>:<turn>:<hook>:<sha of input>`. The turn counter is in `${TMPDIR}/zcie-<session>.turn`.
A `file.changed` event with the same session and turn as a `user.message` makes a verified `ClientTrace` link between the request and the file.
