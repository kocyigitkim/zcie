# Correctness with ZCIE active — final measurement, 2026-10-18 (ADR-020, AC-47)

Build: all of P5 (D-066 to D-082). Method: `benchmarks/isolated` (frozen home `c89d67e55ac630588986b68d4b389d24`, parser version 4; unchanged after each run), model `sonnet`, Claude Code 2.1.302, conditions U (ZCIE hooks with the verification gate) and UP (U and the tool profile `auto`), 3 repetitions, seed `p56`. `A0` and `A0P` (no ZCIE) come from `isolated-p51-2026-10-17.md` (1 repetition; they do not use ZCIE). Raw records: `benchmarks/results/isolated/p56-final.jsonl`, `p56-edit-gate.jsonl`, `p53.jsonl`.

## Correct answers

| Set | U | UP | A0 (no ZCIE) |
|---|---|---|---|
| code questions (23 tasks × 3) | **69/69** | **69/69** | 19/23 |
| edit tasks (4 × 3), checked by a command | **12/12** | **12/12** | 3/4 |
| change requests, locate (20 × 3) | **60/60** | **60/60** | 20/20 |
| edit tasks again with the final gate (Bash changes checked; 4 × 1) | 4/4 | 4/4 | — |
| 30-stage memory check (no model), 8 runs on the installed daemon | 17/17 in runs 2 to 8; run 1: 14/17 | | |
| user-ideas check (15 questions) | 15/15 | | |
| everyday and generic prompts with a capsule | 0/15 and 0/15 | | |

Notes:
- **C12 (locate).** The task expected only `indexer/merger.rs`. The code has two valid answers: `merger.rs` has the hard limit `MAX_DOC_LIMIT` (2³¹); `log_merge_policy.rs` has `DEFAULT_MAX_DOCS_BEFORE_MERGE`, the practical size of a merged segment with the default policy. The task now accepts both (`expect_any`, `benchmarks/context/locate.json`); 3 records of `p53.jsonl` were graded again. In the final run 5 of 6 answers were `merger.rs`.
- **Memory run 1 (14/17).** The last 3 questions got no capsule: the hook waited at most 2 s for the daemon, and the daemon was slow (4 benchmark daemons and Claude sessions on the same computer; hook p95 1.8 s, maximum 3.7 s under that load). Fix: the hook waits up to 4 s, Claude Code gives it 10 s, and a timeout prints a line that no context was added. Runs 6 to 8 under the same load: 17/17 each.

## Tokens (U and UP against A0, paired by task)

| Group | A0 tokens per correct run | U | UP |
|---|---|---|---|
| code | 48,208 | 19,782 (−59 %) | 10,466 (−78 %) |
| edit | 61,217 | 50,824 (−17 %) | 31,963 (−48 %) |
| locate | 40,569 | 28,210 (−30 %) | 13,697 (−66 %) |
| code + edit (the 27 tasks of AC-32) | 49,982 | 24,381 (−51 %) | 13,651 (−73 %) |

File-search calls per run (Read, Grep, Glob), the 27 tasks of AC-33: A0 1.07, U 0.29, UP 0.33.

## Problems found and fixed in this measurement
1. A superseded record was shown as "verified" when a verified path found it.
2. A task made by a forgotten message stayed after the forget.
3. All Claude Code hooks had a 5 s limit; the verification gate needs up to 120 s.
4. The gate found no Cargo.toml in a monorepo (crates in folders), and it did not see changes made with Bash (`sed -i`); it now finds the nearest manifest and asks Git after a Bash command.
5. Hooks have no login PATH: `cargo` was not found and the gate passed silently. The gate now adds `~/.cargo/bin`, `/opt/homebrew/bin` and `/usr/local/bin`, and says when a check program is missing.
6. A live Claude Code session showed the gate at work: the model left `pub fn label() -> i32 { "label" }`, the gate blocked the stop with the compiler error, the model fixed it, and the crate compiles.
