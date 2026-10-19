# ADR-010 — One-turn answers and operational checks

- Status: Accepted
- Date: 2026-10-16

## Context
Measurements of Claude Code with ZCIE (`benchmarks/results/three-projects-2026-10-16.md`, round 24) showed:

1. A question that the model answers in 1 turn costs about half of a 2-turn answer. Each extra turn reads the cached context again (about 9,000 tokens).
2. The model needed a second turn for 7 of 12 tasks. Two causes:
   - The context hook injected no capsule. Its gate accepted only exact answers, task and history classes, and symbol names in the prompt. "What is the late fee rate?" got nothing.
   - The capsule had the path of a symbol but not its value. TypeScript `const` declarations and Python module constants were not symbols at all.
3. "Which commit changed the fee rate?" is classified `implementation`. With no task in the store, the route returned nothing.
4. Operations: the hooks do nothing when the daemon does not run, and nobody sees the gap. Installed hook scripts get old after an update of `zcie`. `zcie daemon stop` returned before the process ended, so a `start` right after it failed on the lock. Failed jobs of deleted projects stayed in the dead-letter list.

## Decision

### 1. Match coverage
`QueryResponse.match_coverage` (0.0 to 1.0) is the best share of the question terms that one result contains (`crates/zcie-query/src/relevance.rs`).

- Terms: lowercase stems of 6 characters after a plural or verb ending is removed ("applies" → "apply"). camelCase, snake_case and paths are split.
- Question words, meta words ("function", "file", "commit", "value") and command verbs ("run", "fix", "add") are not terms.
- Only facts count: symbols, files, commits, tasks and decision records. Conversation events and stored requests do not count, because they repeat chat words.
- The value is 0.0 if the best result has fewer than 2 terms.
- The planner sets the field before the capsule is built, so the token budget includes it.

### 2. Context hook
The hook injects a capsule also when `match_coverage >= 0.6`. Each symbol line shows the signature line (for a constant: the value). A dry run with 12 everyday prompts ("run the tests", "thanks, looks good", "devam et", …) injected 1 capsule, for the one prompt that asked about code.

### 3. Constants are symbols (parser version 2)
- TypeScript: top-level `const` declarations that are not functions are `Constant` symbols. `let` and `var` are not symbols.
- Python: module-level assignments to UPPER_CASE names (PEP 8) are `Constant` symbols.
- `zcie_parse::PARSER_VERSION` is stored in each file record. A file with an older version is parsed again, also if its content is the same.

### 4. Commit questions
If the `implementation` route finds no task, it runs the lexical search. Commits get a lexical bonus of 0.3 in the `implementation` class.

### 5. Operations
| Item | Decision |
|------|----------|
| `zcie doctor [--fix]` | Checks the daemon, the queues and dead letters, the index lag, the key file mode, the installed hooks, the project of the folder, the Claude Code settings and the autostart agent. `--fix` reinstalls old hooks and starts a stopped daemon. Exit code 1 if a check fails. |
| `zcie daemon install \| uninstall` | macOS launchd agent `dev.zcie.zcied`: start at login, restart after a crash (`KeepAlive.SuccessfulExit = false`), no restart after a clean stop. `--dry-run` prints the agent file. |
| `zcie daemon stop` | Waits up to 30 s until the process ends. |
| SessionStart hook | If the daemon does not answer in 1 s, prints one line for the model: the session is not recorded; tell the user to start the daemon. |
| Dead letters | Deleting a project, and the hourly maintenance sweep, remove dead-letter jobs of projects that no longer exist. |

### 6. Each ref once per session
The context hook writes the refs that it injected to `${TMPDIR}/zcie-<session_id>.seen` and drops them from later capsules of the same session; with no new result it injects nothing. SessionStart with `source` `compact` or `clear` removes the file. Measured in long sessions (11 questions, round 27): Z costs 0.486 USD per session against 0.670 for A (−27 %; round 21: −19 %), 1.79 turns per question (before: 2.00), 33/33 correct.

## Consequences
- In the 3-project benchmark, Z needed 1.4 turns per task instead of 1.7. API time per run fell from 4.9 s to 4.4 s. On the tantivy tasks, Z needed 2.0 turns instead of 2.2 and API time fell from 7.7 s to 6.4 s.
- The cost did not fall on the 3-project tasks (0.247 → 0.250 USD for 24 runs). The capsule is written to the prompt cache, and this costs about as much as the turn that it saves. The gain is speed and fewer tool calls, not cost.
- No regression: the tantivy tasks stay 30/30 correct, and Z costs 53 % less than A (before: 49 %).
- The first start after an update parses all files again (parser version 2).
