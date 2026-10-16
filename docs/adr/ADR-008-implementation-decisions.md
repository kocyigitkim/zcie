# ADR-008 — Implementation decisions of milestone 1

- Status: Accepted
- Date: 2026-10-16

## Context
The development tasks `D-001` to `D-044` were implemented. Some documents left a choice open. Some details changed when the code ran for the first time. This ADR records each decision. Where this ADR and an earlier text disagree, this ADR applies.

## Decisions

### Build and toolchain
- I-01: The workspace uses Rust edition 2024 and `resolver = "3"`. The toolchain is Rust stable.
- I-02: `.cargo/config.toml` sets `LIBCLANG_PATH` and an rpath to the Command Line Tools library folder. The `librocksdb-sys` build script loads `libclang.dylib` at run time. A user value of `LIBCLANG_PATH` wins.
- I-03: `rocksdb` uses `default-features = false` with `lz4`. `reqwest` has no TLS features, because clients use loopback HTTP only.
- I-04: The crate `getrandom` makes API keys. The crate `globset` matches exclusion globs.

### Runtime
- I-05: `JobQueue` uses one `Mutex` with two `VecDeque` lists and a `Condvar`. Workers are OS threads, because all job handlers are blocking. The queue ignores a job ID that is already queued. A `running` set prevents a second run of the same job.
- I-06: A maintenance thread puts stored pending jobs into the queue every 2 seconds (`refill_queue`) and raises `graph_seq`. It runs the orphan blob sweep 30 seconds after start and then each hour.
- I-07: Job handlers build their batch and write it under one write lock. `IndexFile` reads, hashes and parses the file before it takes the lock.

### Write path
- I-08: An `assistant.message` with `capture.assistant_messages = false` gets HTTP 403 (`forbidden`). The event is not stored.
- I-09: `command.run`, `tool.result` and `test.result` payloads are truncated. For a JSON payload only the `output` field is truncated and `"truncated": true` is added.
- I-10: The `ExtractMemory` handler of a `file.changed` or `file.deleted` event resolves the path to a worktree, allocates the file ID, makes the `ClientTrace` links of the turn, and makes an `IndexFile` or `DeleteFile` job.
- I-11: The file watcher makes `IndexFile` and `DeleteFile` jobs directly. It does not capture events. Client hooks send `file.changed` events through the capture path.
- I-12: The live overlay is in memory only. The capture of a `file.changed` event, a watcher change and a scan add paths. The completion of the file job removes the path. The `revisions: overlay` key of `docs/06-storage.md` is not used.
- I-13: Extra lookup keys: `t/"session"/<session>/<event id>` (forget by session) and `t/"mem_of_event"/<event id>/<memory id>` (records of an event). `revisions: t/"file_state"/<worktree>/<path>` stores `{content_hash, seq}`.

### Search
- I-14: Processors send `SearchOp` values directly to the search writer channel after their batch write and before `graph_seq` rises. The writer commits every second or after 500 operations, and only if an operation was applied. After each commit cycle it writes `search_seq = G` (G read before the channel drain).
- I-15: The job kind `UpdateSearch` is the full rebuild. At startup, if the search index does not open or `search_seq < graph_seq`, the daemon starts a full rebuild. A clean shutdown commits the index, so a rebuild is not necessary after it.
- I-16: The writer stops on an explicit `SearchOp::Stop`, because other senders can still exist at shutdown.

### Memory and graph
- I-17: X-05 (Git SHA) accepts only values with a letter and a digit, to reduce false matches.
- I-18: X-07 (decision marker) has no trailing word boundary, so `decision:` matches.
- I-19: Edges without source events (Git and AST edges) keep the revision in `evidence.revision`.

### Read path
- I-20: The cache key contains `canonical_seq` and `graph_seq`. A new event or a completed job makes a new key, so a cached answer never hides newer data. Forget, links and corrections also clear the cache of the workspace.
- I-21: A cursor has the form `<query_id>:<offset>`. Result pages stay in the L0 cache for 5 minutes.
- I-22: Classifier keywords: `defined` and `tanımı` (definition), `what did we do` and `ne yaptık` (timeline), `implements` (implementation).
- I-23: `QueryResponse` has the extra fields `class` and `note`. `ExpandItem` has the extra fields `path` and `range`.
- I-30: In the `Timeline` and `Task` classes the kind weight is 1.0 for task, memory, commit and event results (history kinds), and 0.5 for code. A history question then shows the commits and test results, not only code (TS-12-01).
- I-31: The `Timeline` class searches the `conversations` index first and normalizes its scores alone. Hits from `code` and `documents` are normalized separately and their lexical score is multiplied by 0.5 (`docs/09-query-and-context.md` section 4.3, "conversations then code").
- I-24: Search hits and graph neighbors are loaded from the canonical store before they are returned. A hit with a tombstone, a deleted file, a retracted record or a forgotten event is dropped.

### Daemon, CLI and adapters
- I-25: `zcied` takes the lock with `std::fs::File::try_lock`. The lock file stays; the lock is the OS file lock.
- I-26: On the first start the daemon prints the admin key once and also writes it to `~/.zcie/config/admin.key` (mode 0600). The CLI uses this file if `--key` and `ZCIE_API_KEY` are not set.
- I-27: `zcied` accepts `--home`. The default `storage.data_dir` follows the home folder.
- I-28: The Claude Code hook `stop.sh` is attached to the `SessionEnd` event, because `Stop` runs after each response.
- I-29: `forget` with a ref that is not an event (`mem`, `task`, `file`, `sym`, `commit`) writes a tombstone for the ref and removes its edges. A `mem` ref is also retracted.

### Claude Code integration (session test, 2026-10-16)
- I-32: A `git.commit` event without a repository ID finds its worktree by `git cat-file -e <sha>^{commit}` in each worktree of the workspace. Before this fix, a workspace with two repositories dropped the commit link.
- I-33: `zcie-mcp`, the Claude Code hooks and the Git hook read the key from `ZCIE_API_KEY`, else from the file in `ZCIE_API_KEY_FILE` (Git hook: `git config zcie.apiKeyFile`). No key is stored in a project file.
- I-34: The repository has project-level Claude Code files: `.mcp.json` (MCP server `zcie`) and `.claude/settings.json` (hooks). User values (workspace, key file) are in `.claude/settings.local.json`, which Git ignores.

### Token cost in Claude Code (benchmark, 2026-10-16)
- I-35: The hook `inject-context.sh` adds a ZCIE capsule to the prompt only for an exact answer, a task, implementation, timeline or exact-ref question, or a symbol named in the prompt. Generic chat gets no capsule, so no extra cache write.
- I-36: The lean Claude Code profile `adapters/claude-code/lean-settings.json` denies 21 unused built-in tools. It cuts the context of each turn from about 21,000 to 6,600 tokens. See `benchmarks/results/claude-code-ab-2026-10-16.md`.
- I-37: `zcie_query` (MCP) returns the source of the best symbols in the same call and removes null fields. MCP stays available for explicit deep lookups; it is not the default path.

### Automatic Claude Code setup (2026-10-16)
- I-38: `zcie claude setup [--scope project|user]` installs the hook scripts (embedded in the binary) to `~/.zcie/hooks/`, makes or extends the API key file `~/.zcie/config/claude-code.key`, registers the project, and merges the lean deny list, the env values and 10 hooks into the Claude Code settings file. It records each added entry in a manifest; `zcie claude remove` removes exactly these entries. A second run adds nothing.
- I-39: Hooks of the profile: capture (session start/end, prompts, file changes), `inject-context.sh` (selective capsule), `session-context.sh` (short working rules once per session), `session-guard.sh` (suggests `/clear` to the user when a session grows), `read-guard.sh` (limits a full Read of a file over 600 lines to 250 lines and adds the ZCIE symbol outline), `tool-unlock.sh` and `tool-relock.sh` (enable a denied tool when the prompt needs it, deny it again at session end).
- I-40: A tool that `tool-unlock.sh` enables is available from the next message of the session. Claude Code fixes the tool list when the session starts; in the same message the model uses the tools it has (for example `curl` through Bash).
- I-41: Query results drop memory and event items that only repeat the question, before the search limit; items with the same text keep one copy (the memory record before its event). In memory-first classes, decision, preference, constraint and correction records rank above plain requests.
- I-42: Tier A sentence splitting ends a sentence only at `.`, `!` or `?` followed by white space, so `0.75` or `a.rs` does not cut a decision.
- I-43: `--cache-5m` sets `FORCE_PROMPT_CACHING_5M=1`. It is not the default: it was 12 % cheaper in back-to-back runs, but a pause over 5 minutes makes the next turn write the whole context again.

- I-44: Decision, preference, constraint and correction records are also kept in a decision index (`lookup: t/"decision"/<workspace>/<memory id>`). Timeline/decision questions read it directly and score it by word overlap, so a few decisions are not hidden by many stored messages. A one-time startup migration fills the index for older records.
- I-45: `zcie claude setup --profile lean|balanced`. `lean` (default) denies 21 tools; `balanced` denies 15 and keeps web, subagent, skill, LSP and notebook tools. A profile change removes the deny rules of the old profile. `session-context.sh` lists the denied tools (name and one line, about 150 tokens) once at session start (`startup` matcher only), so the model knows them. The model cannot enable a tool itself: Claude Code's auto-mode classifier blocks that permission change, which is correct.
- I-46: Measured alternatives that did not lower the cost: `ENABLE_TOOL_SEARCH=true` (all tools deferred; −12 % tokens, +5 % cost in long sessions) and the `balanced` profile (−31 % tokens, +41 % cost). Both send a large block after the varying prompt, so each session writes it to the cache again.

- I-47: `zcie claude setup` has the default profile `auto`: `ENABLE_TOOL_SEARCH=true` plus a deny list of only the 6 tools that Claude Code cannot defer (`Skill`, `Agent`, `Workflow`, `ScheduleWakeup`, `ReportFindings`, `ListAgents`). The other tools stay available by name and the model loads them itself through ToolSearch, as in the default Claude Code. Measured against the default: −26 % cost for single questions, −46 % for code edits, the same cost in long sessions (+2 %, ±9 %), −44 % to −60 % tokens, 30/30 and 11/11 correct. `lean` stays available as the cheapest profile (−15 % to −46 % cost) when automatic use of the extra tools is not needed. A profile change also replaces the env values that the setup added.

## Consequences
- The behavior of milestone 1 is defined by the documents plus this ADR.
- The test phase (`docs/13-test-plan.md`) uses these decisions as the expected behavior.
