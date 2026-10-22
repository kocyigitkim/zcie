# Cursor adapter

`cursor-hook.sh` is one dispatcher hook for all Cursor events (ADR-025). It maps the Cursor input to the Claude Code input shape, runs the matching Claude Code hook scripts from `<home>/hooks/`, and prints Cursor JSON. `sessionStart` returns the ZCIE rule and the repo map as `additional_context`; `beforeSubmitPrompt` returns the context capsule; `afterFileEdit`, `afterAgentResponse` and `sessionEnd` send events to the daemon.

Event map: `sessionStart`, `beforeSubmitPrompt`, `afterFileEdit`, `afterAgentResponse`, `sessionEnd`.

Cursor CLI sends no prompt or answer hooks, so `sessionEnd` replays its `transcript_path` once when the session has no turn file (TS-25-08).

Install: `zcie cursor setup [--dir <project>] [--global]`. Remove: `zcie cursor remove`.
