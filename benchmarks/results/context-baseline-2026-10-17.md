# Context-efficiency baseline — 2026-10-17 (ADR-017, D-055)

Version: ZCIE before phase P4 (ADR-016 state), installed with `zcie setup` (Claude Code hooks for all projects, multilingual vectors, capsule budget 1,200). Model: the Claude Code default model. Runs: `benchmarks/context/run.py`. Each task is a new `claude -p` session. Condition **U** = the installed ZCIE; **A0** = the same Claude Code with all hooks off (no ZCIE). Raw records: `benchmarks/results/context/base-*.jsonl`.

## Code questions (`benchmarks/context/code.json`, 27 tasks: 3 sample projects, tantivy)

| Condition | Runs | Correct | Input tokens per run | Output per run | Cost per run | Turns per run | Tool calls per run |
|---|---|---|---|---|---|---|---|
| A0 | 27 | 21 | 22,994 | 497 | $0.0158 | 2.81 | Grep 1.11, Bash 0.30, Read 0.26, Glob 0.07 |
| U | 54 | 54 | 16,435 | 339 | $0.0169 | 1.81 | Grep 0.48, Read 0.17, Bash 0.06 |

File-search calls (Read, Grep, Glob) per run: A0 1.44, U 0.65.

## Localization (`benchmarks/context/locate.json`, 20 change requests)

| Condition | Runs | Correct | Input tokens per run | Cost per run | Turns per run | Tool calls per run |
|---|---|---|---|---|---|---|
| A0 | 20 | 20 | 21,316 | $0.0093 | 2.50 | Grep 1.45, Read 0.05 |
| U | 40 | 38 | 21,712 | $0.0121 | 2.52 | Grep 1.50, Glob 0.03 |

ZCIE gives no gain for change requests: the model searches with Grep as with no ZCIE. The model rarely reads a file for these requests (it answers from the Grep output), so "the first file that the model reads" (AC-36 before this file) cannot be measured. AC-36 now uses the file-search calls and the correct answers.

## Memory and capsule (no model calls)

| Check | Result |
|---|---|
| 30-stage simulation (`simulate.py`) | 17/17; capsule 216 tokens per question on average |
| User ideas (`analyze_ui.py`) | 15/15 in the capsule; capsule 720 tokens per question on average |
| Everyday prompts (`noise.py everyday.json`) | 0/15 with a capsule |
| Short generic prompts (`noise.py generic.json`) | 3/15 with a capsule |
| Real Claude runs (earlier today, same version) | 30-stage 17/17 (Z6, Z7); user ideas 15/15 (R3, R4) |

## Targets from this baseline (AC-32 to AC-36)
- AC-32: code questions, U: 16,435 → 11,505 input tokens per run or fewer, 54/54 correct.
- AC-33: code questions, U: 0.65 → 0.39 file-search calls per run or fewer.
- AC-34: capsule 216 → 162 tokens (simulation) and 720 → 540 tokens (user ideas) or fewer; 17/17 and 15/15 stay.
- AC-35: generic prompts 3/15 → 0/15; everyday 0/15 stays.
- AC-36: localization, U: 1.53 → 0.6 file-search calls per run or fewer, 38/40 correct or more.
