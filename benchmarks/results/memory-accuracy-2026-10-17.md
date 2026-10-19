# Memory accuracy — 2026-10-17 (ADR-013)

Goal: each question about earlier sessions gets the correct answer in a NEW Claude Code session (no `--resume`, no shared context). Two question sets:

- **30-stage test** (`benchmarks/memory-30`): a copy of the sample project `ledger-rs`. 13 stages teach facts, decisions, rules and corrections; 17 stages ask about them. Each stage is a new `claude -p` session. Conditions: **Z** = ZCIE hooks of `zcie claude setup` (the first session makes the project); **A** = no ZCIE.
- **User-ideas test** (`benchmarks/user-ideas`): 15 questions about the ideas and decisions of the user in the ZCIEngine project, answered from the imported conversations (`zcie import claude`). Each question is a new session. Condition **R** = read-only ZCIE hooks (the context capsule, no capture, so that the test questions do not become memory); **A** = no ZCIE.

Grading: an answer is correct if it has each expected item (one string of a list of alternatives) and no forbidden string; case, accents, dotless i and the decimal comma ("2,25") are ignored (`run.py --regrade`). Model: Claude Code default model, user settings only, tools Read/Grep/Glob/LS/Bash. Hardware: Apple Silicon Mac, release build, one daemon with about 300,000 vectors and 3,800 pending file jobs during the runs.

## 1. Results in new sessions

### 1.1 30-stage test (17 questions per run)

| Run | Code state | Correct | Cost (30 sessions) | Turns per question | Wall time |
|---|---|---|---|---|---|
| Before this work (try 1) | ADR-012 | 8/17 | — | — | — |
| A0 (no ZCIE) | — | 8/17 | $1.462 | 2.71 | 322 s |
| Z0, Z1, Z2 | before fix 1 (section 2) | 16/17 each | $0.628–0.651 | 1.12–1.24 | 253–259 s |
| Z3, Z4 | after fix 1 | 17/17 each | $0.658, $0.681 | 1.06, 1.00 | 257, 268 s |
| Z5 | after fixes 1–3 | 17/17 | $0.628 | 1.24 | 250 s |
| Z6, Z7 | final | 17/17 each | $0.629, $0.650 | 1.18 | 254, 258 s |

- Final code: **85 of 85** questions correct in 5 runs (Z3 to Z7). Without ZCIE: 8 of 17.
- Cost with ZCIE: −56 % against A (mean $0.645 against $1.462); turns per question 1.15 against 2.71.
- A misses S05, S07, S15, S19, S23, S25, S27, S28, S29: the facts of earlier sessions are not in the code (rates, versions, reviewers, release days, the payment provider, the corrected limit, the list of all decisions).

### 1.2 User-ideas test (15 questions per run)

| Run | Code state | Correct | Cost | Turns per question |
|---|---|---|---|---|
| A0 (no ZCIE) | — | 11/15 | $0.929 | 3.27 |
| R0 | before fixes 2–4 | 14/15 (Q09) | $0.341 | 1.07 |
| R1 | before fixes 2–4 | 13/15 (Q07, Q09) | $0.109 | 1.20 |
| R2 | after fix 2 | 14/15 (Q09) | $0.163 | 1.00 |
| R3, R4 | final | **15/15 each** | $0.293, $0.133 | 1.00, 1.07 |

- A reads the documents of the repository and answers 11 questions; it misses what the user said only in the chat (Q06 automatic project ID, Q07 "bana soru sorma", Q08 "testleri sen yap", Q09 the follow-up about automatic tool loading).
- Answers are not copied to the repository: they quote the private conversations of the user.

## 2. Faults found by the real runs and their fixes

| # | Fault (run) | Cause | Fix |
|---|---|---|---|
| 1 | S27 "Which database did we reject, and why?" wrong in Z0–Z2 | The correction "PostgreSQL 16 değil, 17" superseded the statement that also says "MySQL rejected". The server ranked it first, but the hook moved superseded records to the end BEFORE it took the best 6, so the record was cut. | The hook takes the best 6 first, then puts superseded records last, with the note "where they differ, the correction is true". |
| 2 | Q07 "soru sorulmasını istiyor mu?" wrong in R1 | Six rules ("Never …") of a pasted specification showed the full document as their text; the best window of the document matched the question. | A sentence record shows at most 390 characters around the sentence. |
| 3 | Q09 refinement missing (R0–R2) | The refinement "Claude bunu yine otomatik yükleyebilsin fakat maliyet artmasın" says "bunu" and has no topic words. | Follow-up context (previous message), the next 2 short messages of the top requests, TR/EN equivalents (araç/tool), capsule budget 1,200. |
| 4 | Turkish decimal "%2,25" graded wrong (Z1, Z2 S05) | The grader compared "2.25" only. | The grader reads a decimal comma as a point. |
| 5 | A capsule for `<task-notification>` prompts | Only the capture hook skipped them. | `zcie_is_user_text` in both hooks. |
| 6 | Capsules for "JSON ile YAML farkı", "README commit mesajı" | Capital words counted as project names. | General format words are not names; a name counts only in statements. |

## 3. Checks without a model

| Check | Before | Final |
|---|---|---|
| 30-stage capsule has the answer (`simulate.py`) | 12/17 | 17/17 |
| User-ideas: answer in the top 10 / in the capsule | 15/15 / 14/15 | 15/15 / 15/15 |
| Everyday prompts with a capsule (`noise.py`, 15 prompts, ZCIEngine and ledger-rs) | 2/15 | 0/15, 0/15 |
| Captured message searchable after | — | 0.1 s (3,800 file jobs waiting) |
| Unigram tokenizer IDs equal to Hugging Face `tokenizers` 0.21.1 | — | 12,752/12,752 lines |
| `cargo test --workspace --release` | 134 | 145 passed, 0 failed |

## 4. Vector evaluation (`benchmarks/vector/eval.py`, 36 questions)

| Group | n | off hit@1 | 8M hit@1 | multilingual hit@1 | off MRR | 8M MRR | multilingual MRR |
|---|---|---|---|---|---|---|---|
| paraphrase | 13 | 10 | 10 | 9 | 0.785 | 0.815 | 0.762 |
| hard | 8 | 1 | 2 | 2 | 0.201 | 0.354 | 0.396 |
| memory | 3 | 1 | 2 | 1 | 0.548 | 0.833 | 0.500 |
| turkish (Turkish question, English code) | 8 | 1 | 1 | **6** | 0.125 | 0.125 | **0.812** |
| exact | 4 | 4 | 4 | 4 | 1.000 | 1.000 | 1.000 |
| all | 36 | 17 | 19 | **22** | 0.513 | 0.581 | **0.696** |

The multilingual model (default) loads in about 0.3 s and uses about 200 MB with the index. Cross-language top-1 on 8 Turkish/English pairs: 6/8 (8M: 1/8).

## 5. Open items
- `zcie daemon stop` waited more than 30 s twice while many file jobs ran (the next stop took 0.2 s). The shutdown now logs the time of each step ("workers stopped", "search writer stopped") to find the slow step.
- The bilingual equivalents are a fixed list (`crates/zcie-query/src/synonyms.rs`).
- One full `cargo test --workspace` run failed `killed_jobs_run_again_without_duplicates`: the restarted test daemon did not answer within 30 s while the live daemon used about 300 % CPU. The test passed alone 3 of 3 times and the next full run passed 145 of 145.
- The live daemon still indexes the projects that `zcie import claude` registered (about 300 % CPU, 5,000 queued background jobs). The graph watermark waits for the oldest file jobs, so `zcie doctor` reports "events behind"; captured messages are searchable at once (0.1 s).
- The test runs made temporary projects in the live store (sim-*, proj-Z*, …). They were deleted after the runs (23 projects).
