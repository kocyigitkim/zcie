# ADR-022 — Token savings for each project: `zcie usage` and the menu bar app

- Status: Accepted
- Date: 2026-10-19
- Tasks: D-083 to D-086 of `docs/12-development-plan.md`
- Requirement of the user: "A status bar app for ZCIE. It measures the token gain for each project. When I open the program, it gives a more detailed report."

## Context
The isolated benchmark (ADR-019) measures the gain of ZCIE on fixed task sets. The user wants the gain on the real work, for each project. Two sources have the data:

1. **Claude Code transcripts** (`~/.claude/projects/<folder>/<session>.jsonl`, and `<session>/subagents/*.jsonl`). Each assistant record has `message.usage` (`input_tokens`, `cache_creation_input_tokens`, `cache_read_input_tokens`, `output_tokens`). The records of one message repeat the same `message.id`; each message is counted once. Each ZCIE hook output is an `attachment` record whose `command` is in `.zcie/hooks/`; the capsule starts with "ZCIE context".
2. **The ZCIE hooks.** They know the actions of ZCIE that are not in the transcript: a read guard that limited a Read, and the result of the verification gate.

No source can give the tokens that a ZCIE session would have used without ZCIE. A real counterfactual needs the paired benchmark. For the real work, the report gives an observation and says so.

## Decision
1. **Session record.** For each interactive session (`entrypoint` is not `sdk-cli`; programs and benchmarks are excluded), `zcie usage` reads:
   - the folder (`cwd`) and the project: the folder with `.claude/zcie_id` above it, else the Git root, else the folder;
   - ZCIE active: from the first ZCIE hook output or `mcp__zcie__` tool call. When ZCIE joins a running (resumed) session, the part before is a session with no ZCIE; the typed message of the first capsule belongs to the ZCIE part;
   - turns: the messages that the user wrote (the rules of `zcie import claude`);
   - tokens: input + cache write + cache read + output, of the main session and its subagents (the definition of the benchmark);
   - Read calls, and search calls (Grep, Glob, and Bash commands that start with `grep`, `rg`, `find`, `fd` or `ls`);
   - capsules and their size in characters.
2. **Savings of a project.** Sessions in the window (default 90 days), with at least one turn:
   - `tokens_per_turn` of a session = tokens / turns. The project value is the median of its sessions.
   - Saving % = 1 − median(ZCIE sessions) / median(sessions with no ZCIE).
   - Saved tokens = turns of the ZCIE sessions × (median with no ZCIE − median with ZCIE). A negative value is shown as a loss.
   - No minimum number of sessions (decision of the user, 2026-10-19: "compute directly"). One session with ZCIE is enough. Each value says how many sessions it uses.
   - Basis `project`: the project has sessions with no ZCIE. The note says "observed: N with ZCIE against M with no ZCIE": the tasks of the two groups are not the same.
   - Basis `all projects`: the project has no session with no ZCIE. The baseline is the median of the sessions with no ZCIE of all projects. The value is labeled rough ("≈"), and `rough_saved_tokens` keeps its sum apart from `saved_tokens`. Reason: ZCIE is set up for the user, so a project that starts after the setup never has sessions with no ZCIE.
3. **Hook statistics.** The hooks append one JSON line for each action to `<zcie home>/stats/hooks.jsonl` (the folder above the hook scripts): `read_guard` (lines of the file, lines read) and `gate` (`pass`, `block`, `skipped`). One line has the time, the session and the folder; it has no file content and no key.
4. **Incremental scan.** The cache `<zcie home>/run/usage-cache.json` keeps, for each transcript, the byte offset of the last complete line and the partial session records. A scan reads only the new bytes. A file that became shorter is read again from the start.
5. **`zcie usage`.** Text table by default; `--json` gives the full report for programs; `--days N` sets the window.
6. **Menu bar app `ZCIE Bar`** (`apps/zcie-bar`, Swift, SwiftUI `MenuBarExtra` and Swift Charts, macOS 14 or later):
   - It runs `zcie usage --json --no-disk` at start, every 3 seconds while its menu or report window is open, every 30 seconds otherwise, and on "Refresh". It reads no transcript itself. A scan reads only the new lines (about 30 ms); the cache is written when it changed, at most once a minute. The view is drawn again only when a value changed.
   - The menu bar shows the saving % of the project with the last activity, or "ZCIE" with no value.
   - The menu lists the projects: saving %, saved tokens, and ZCIE sessions.
   - "Detailed report" opens a window: for each project the two groups (sessions, turns, tokens per turn, Read and search calls per turn), capsules, read guard, gate, the tokens of each day, and the method note.
   - `apps/zcie-bar/build.sh` builds `ZCIE Bar.app` into `~/Applications` with `swiftc` (no Xcode project). `--login` adds a LaunchAgent that starts it at login.

## Consequences
- The first scan of a large transcript folder takes time (5 GB: about one minute). Later scans read only the new bytes.
- The saving of a project with no sessions before ZCIE cannot be shown. The benchmark numbers (ADR-019) stay the controlled measurement.
- Transcripts stay on the computer. The cache keeps counters only, never message text.
