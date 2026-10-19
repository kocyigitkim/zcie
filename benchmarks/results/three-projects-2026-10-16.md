# Multi-project benchmark — three sample projects (ADR-009)

Date: 2026-10-16. Hardware: Apple M4 Pro (arm64). Claude Code binary pinned, `DISABLE_AUTOUPDATER=1`.
Model: the default model of Claude Code. Daemon: release build, one store `~/.zcie` with 7 projects
(the 3 sample projects, ZCIEngine, tantivy (144k lines), and 2 small projects).

## 1. Corpus

`benchmarks/samples/generate.py <dir>` makes three Git repositories (fixed seed, fixed dates):

| Project | Language | Lines | Target facts |
|---------|----------|-------|--------------|
| ledger-rs | Rust | 3,017 | `apply_late_fee`, `LATE_FEE_PERCENT` (1.5 → 1.75 in a later commit), `MAX_DB_CONNECTIONS = 24`, `normalize_currency` |
| atlas-ts | TypeScript | 2,784 | `resolveRoute`, `CACHE_TTL_SECONDS = 420` (Redis), `normalizeCurrency` |
| orbit-py | Python | 2,726 | `schedule_orbit_burn`, `MAX_BURN_RETRIES = 7` (RQ) |

Each project has 45 filler modules. `benchmarks/samples/prepare.py <dir> <tasks.json>` runs `zcie claude setup`
in each project (this writes `.claude/zcie_id`), waits for the index, and captures decisions of an earlier session.
These decisions are only in the ZCIE memory, not in the files:

- ledger-rs: Postgres 16; MySQL was rejected because it has no transactional DDL.
- atlas-ts: Fastify 4 instead of Express; Redis 7 is only a route cache.
- orbit-py: Celery was replaced with RQ because the Celery workers leaked memory.

## 2. Tasks (12) and conditions

| Group | Tasks | Asked from | Example |
|-------|-------|------------|---------|
| local | L1–L4 | the same project | "What is the late fee rate and which function applies it?" ; history: "Which commit changed the late fee rate?" |
| cross | X1–X3 | another project, by name (X1, X3) or by project ID (X2) | "In project proj_…, which function schedules an orbit burn?" |
| memory | M1–M3 | another project, by name or ID | "Which database did we decide to use for ledger-rs, which did we reject, and why?" |
| global | G1–G2 | any project, "which of my projects …" | "Which of my projects uses Redis, and for what?" |

- **A**: default Claude Code (Read, Grep, Glob, LS, Bash), no ZCIE.
- **Z**: the settings of `zcie claude setup` (auto profile, ZCIE hooks). The project comes from `.claude/zcie_id`.
- Both conditions get the same system prompt line: "The user's other projects are sibling folders of the current folder: ../ledger-rs, ../atlas-ts and ../orbit-py." The hooks do not see it.
- 2 repetitions, 48 runs. A run is correct if the answer contains all expected strings (M1 expects the reason "transactional DDL", so an answer that only says Postgres is not correct).

## 3. Result (round 24, `claude-ab/round24-three-projects-final.jsonl`)

| Group | A correct | Z correct | A cost | Z cost | Δ cost | A tokens/run | Z tokens/run | Δ tokens | A turns | Z turns |
|---|---|---|---|---|---|---|---|---|---|---|
| local | 8/8 | 8/8 | 0.185 | 0.062 | -66 % | 40,652 | 15,746 | -61 % | 2.0 | 1.8 |
| cross | 6/6 | 6/6 | 0.186 | 0.075 | -60 % | 58,882 | 17,425 | -70 % | 3.2 | 1.8 |
| memory | 2/6 | 6/6 | 0.264 | 0.052 | -80 % | 67,428 | 12,598 | -81 % | 4.0 | 1.3 |
| global | 4/4 | 4/4 | 0.126 | 0.057 | -55 % | 57,118 | 19,458 | -66 % | 3.0 | 2.0 |
| ALL | 20/24 | 24/24 | 0.761 | 0.247 | -68 % | 54,648 | 15,997 | -71 % | 3.0 | 1.7 |

Cost in USD for all runs of the group. Tokens = input + cache write + cache read + output per run.

- Z answered all 24 runs correctly. A failed 4 runs: M1 and M2 ask for reasons that exist only in the memory of an earlier session.
- A found the code of other projects by searching sibling folders (3.2 turns). Z got the file path, the project root and the line range in the injected capsule (1.8 turns).
- A question by project ID (X2, M2) works the same as a question by name.

## 4. Defects found and fixed during this benchmark

Round 23 (`claude-ab/round23-three-projects.jsonl`) was the first pass. It found these defects. All are fixed and tested (TS-14-12 to TS-14-15):

1. A question that contains a project ID did not read that project. Fix: `cross_project` also matches project IDs.
2. "Which of my projects …" read only the current project. Fix: questions about all projects read all permitted projects.
3. Scores of different projects are not comparable, so global results were noise from large projects. Fix: term coverage ranking (`crates/zcie-api/src/relevance.rs`).
4. Result paths of another project are relative to its root. Fix: the merged note and the capsule give the project roots.
5. A decision record is one sentence; the reason in the next sentence was lost. Fix: the result snippet is the full source message.
6. Stored prompts (questions) were shown as memory facts. Fix: the capsule drops memory and event items that contain "?".
7. The CLI default idempotency key `cli:<session>:<turn>` collided between projects, and after a project delete a new capture was dropped as a duplicate. Fix: idempotency keys are unique per project.
8. Jobs of a deleted project failed 3 times. Fix: they end with no work.

The substring check of round 23 counted 2 wrong A answers and 1 wrong Z answer as correct ("MySQL" appeared in "I found no MySQL"). Round 24 uses a stricter expected string.

## 5. Rounds 25 and 26 — one-turn answers (ADR-010)

Changes: the context hook injects a capsule when `match_coverage >= 0.6`; symbol lines show the signature (constants show the value); TypeScript and Python constants are symbols; "which commit …" questions fall back to the lexical search. Fresh sample projects, same tasks and conditions.

Round 25 (`claude-ab/round25-three-projects-coverage.jsonl`):

| Group | A correct | Z correct | A cost | Z cost | Δ cost | A tokens/run | Z tokens/run | Δ tokens | A turns | Z turns |
|---|---|---|---|---|---|---|---|---|---|---|
| local | 8/8 | 8/8 | 0.183 | 0.064 | -65 % | 40,604 | 9,160 | -77 % | 2.0 | 1.0 |
| cross | 6/6 | 6/6 | 0.176 | 0.075 | -58 % | 51,878 | 14,438 | -72 % | 2.8 | 1.5 |
| memory | 2/6 | 6/6 | 0.286 | 0.057 | -80 % | 89,726 | 12,737 | -86 % | 4.7 | 1.3 |
| global | 4/4 | 4/4 | 0.120 | 0.054 | -55 % | 57,032 | 19,251 | -66 % | 2.8 | 2.0 |
| ALL | 20/24 | 24/24 | 0.765 | 0.250 | -67 % | 58,441 | 13,056 | -78 % | 3.0 | 1.4 |

| Measure (Z) | Round 24 | Round 25 |
|-------------|----------|----------|
| Turns per run | 1.7 | 1.4 |
| API time per run | 4.9 s | 4.4 s |
| Tokens per run | 15,997 | 13,056 |
| Cost of 24 runs | 0.247 USD | 0.250 USD |

Round 26 (`claude-ab/round26-tantivy-coverage.jsonl`) is the regression check on the 15 tantivy tasks (144k lines, round 22 before the change):

| Condition | Round | Correct | Cost | Tokens per run | Turns | API time per run |
|-----------|-------|---------|------|----------------|-------|------------------|
| A | 22 | 22/30 | 1.009 | 51,729 | 2.57 | 9.7 s |
| Z | 22 | 30/30 | 0.517 | 20,114 | 2.20 | 7.7 s |
| A | 26 | 22/30 | 1.025 | 53,068 | 2.73 | 8.0 s |
| Z | 26 | 30/30 | 0.478 | 19,136 | 2.00 | 6.4 s |

Result: fewer turns and a faster answer; no loss of correctness. The cost per answer does not fall on the small projects: the injected capsule is written to the prompt cache, and this costs about as much as the turn that it saves.

## 6. Rounds 27–29 — capsule once per session, vector search, SDK work (ADR-010 §6, ADR-011)

Round 27 (`claude-ab/round27-long-dedupe.jsonl`): long sessions, the 11 questions of the tantivy tasks in one session, 3 sessions per condition.

| Round | Condition | Cost per session | Tokens per question | Turns per question | Correct |
|-------|-----------|-----------------:|--------------------:|-------------------:|--------:|
| 21 | A | 0.641 | 47,546 | 2.21 | 48/66 |
| 21 | Z | 0.521 | 22,607 | 2.00 | 66/66 |
| 27 | A | 0.670 | 48,352 | 2.36 | 24/33 |
| 27 | Z | 0.486 | 20,723 | 1.79 | 33/33 |

Z against A in the same round: −19 % (round 21) and −27 % (round 27).

Rounds 28 and 29: regression check of Z after the vector search and the doc comments (live daemon with the model; fresh sample projects; 2 repetitions).

| Tasks | Round | Correct | Cost | Tokens per run | Turns | API time per run |
|-------|-------|---------|------|----------------|-------|------------------|
| 3 projects | 25 | 24/24 | 0.250 | 13,056 | 1.38 | 4.4 s |
| 3 projects | 28 | 24/24 | 0.239 | 12,695 | 1.42 | 3.6 s |
| tantivy | 26 | 30/30 | 0.478 | 19,136 | 2.00 | 6.4 s |
| tantivy | 29 | 30/30 | 0.389 | 19,923 | 2.13 | 5.5 s |

Result: no regression. The lower tantivy cost in round 29 is partly the variation of the prompt cache between rounds.

## 7. Cost

Round 23: 1.12 USD. Round 24: 1.01 USD. Round 25: 1.02 USD. Round 26: 1.50 USD. Round 27: 3.47 USD. Round 28: 0.24 USD. Round 29: 0.39 USD.
