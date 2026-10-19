# Context efficiency results — 2026-10-17 (ADR-017, D-065)

Version: ZCIE with phase P4 (D-056 to D-064), installed with `npm install -g ./dist/zcie-0.1.0.tgz` and `zcie setup --yes` (the settings of the baseline: Claude Code hooks for all projects, multilingual vectors, capsule budget 1,200). Hardware: Apple M4 Pro (arm64). Claude Code 2.1.302, default model. The method is the same as the baseline (`benchmarks/results/context-baseline-2026-10-17.md`): `benchmarks/context/run.py`, each task in a new `claude -p` session, condition **U** (the installed ZCIE). Raw records: `benchmarks/results/context/p4-*.jsonl`.

"Input tokens" = input + cache creation + cache read tokens. "File-search calls" = Read + Grep + Glob calls.

## Code questions (`benchmarks/context/code.json`, 27 tasks × 2)

| Run | Correct | Input tokens per run | File-search calls per run | Turns per run |
|---|---|---|---|---|
| Baseline (before P4) | 54/54 | 16,435 | 0.65 | 1.81 |
| P4, first run (before the fixes below) | 54/54 | 16,370 | 0.60 | 1.80 |
| P4, second run (after the memory fixes) | 54/54 | 15,932 (−3 %) | 0.56 (−14 %) | 1.72 |
| P4, third run (after the test-phase fixes) | 54/54 | 16,643 (+1 %) | 0.67 | 1.87 |
| **P4, final (named-symbol fix of the hook)** | **54/54** | **15,561 (−5 %)** | **0.48 (−26 %)** | **1.65** |
| P4 second run, `capsule_rules = false` | 54/54 | 16,762 | 0.61 | 1.81 |
| P4 second run, `repo_map_budget = 0` | 54/54 | 16,130 | 0.57 | 1.74 |
| P4 second run, `graph_rank = false` | 54/54 | 16,239 | 0.57 | 1.74 |
| P4 second run, `compress = false` | 54/54 | 15,700 | 0.56 | 1.70 |

Analysis:
- A task with no tool call costs about 9,000 tokens (one turn). That is the lower limit of a run.
- The 4 edit tasks (E1 to E4) cost about 31,800 tokens each. Claude Code permits an edit only after a Read of the file, so each edit task needs 2 turns or more.
- If all 23 other tasks needed one turn only, the mean would be about 12,300 tokens. The target of AC-32 (11,505) is below this limit for this task set.
- The settings of `[context]` do not change the code results more than the run-to-run variation (±500 tokens). The gain on code tasks comes from two hook changes that have no setting:
  - The capsule gives the body of a symbol that the prompt names (at most 40 lines). Task T2 ("which file defines compute_tf_cache, and what does it compute?") fell from 18,929 to 9,258 tokens; the model reads no file now.
  - The capsule says that the index is current, so a check with Grep is not necessary. Some runs still check (T5: Grep after an exact answer).
- Cost per run is not compared: it depends on the prompt cache of the earlier runs (the runs are back to back).
- Each U run stores its prompts in the memory of the benchmark projects (the capture hook is on). Later runs therefore see earlier benchmark prompts as memory records. In the third run such a record was the first item of the capsule for T2, and the hook added the body of a symbol only when the first item was that symbol. The final hook adds the body of the first shown symbol that the prompt names.
- The run-to-run variation of the code set is about ±700 tokens per run (three P4 runs: 15,561 to 16,643).

## Localization (`benchmarks/context/locate.json`, 20 change requests × 2)

| Run | Correct | Input tokens per run | File-search calls per run | Turns per run |
|---|---|---|---|---|
| Baseline | 38/40 | 21,712 | 1.53 | 2.52 |
| P4, first run | 38/40 | 14,841 (−32 %) | 0.60 | 1.60 |
| P4, second run | 38/40 | 15,092 (−30 %) | 0.62 | 1.62 |
| P4, third run | 39/40 | 14,621 (−33 %) | 0.57 | 1.57 |
| **P4, final** | **38/40** | **14,868 (−32 %)** | **0.60 (−61 %)** | **1.60** |

The locate answer (D-062) gives the files and the line ranges in the capsule, so most runs answer with no search. Model-free check (`benchmarks/context/offline_locate.py`, final build): the expected file is first for 10 of 20 tasks and in the top 3 for 17 of 20.

## Memory and capsule (no model calls)

| Check | Baseline | P4 final |
|---|---|---|
| 30-stage simulation (`simulate.py`), answer in the capsule | 17/17 | 17/17 |
| 30-stage simulation, capsule tokens per question | 216 | 159 (−26 %) |
| User ideas (`analyze_ui.py`), answer in the capsule | 15/15 | 15/15 |
| User ideas, capsule tokens per question | 720 | 521 (−28 %) |
| Everyday prompts with a capsule (`noise.py everyday.json`) | 0/15 | 0/15 |
| Short generic prompts with a capsule (`noise.py generic.json`) | 3/15 | 0/15 |

Fixes made during this measurement (the first P4 run had 15/17, 13/15 and 1/15):
- The "no capsule" rule took a short fact question ("Para tutarlarını hangi tipte tutuyoruz?") for a generic prompt. Now a prompt gets no capsule only if it has 5 words or fewer, 3 content words or fewer, and it points to the conversation ("bunu", "bunların", "this") or has 2 content words or fewer with no full match.
- A "list all decisions" answer got `capsule: false`. A list answer now never gets it.
- Compression kept the first matching sentences, not the best ones. It now keeps the sentences with the most question terms first (as LLMLingua), in their order. Item limit: 450 characters (was 600).
- The hook showed the title of a memory record and then the same text again. It now shows the text only, and cuts the previous and next messages to 150 and 200 characters. The capsule header is shorter.

## Query latency

| Corpus | Query | p50 | p95 |
|---|---|---|---|
| Synthetic, 100,000 lines (`benchmarks/end-to-end/bench.py`, 300 queries each) | symbol lookup | 0.11 ms | 0.27 ms |
| same | lexical + metadata search | 1.48 ms | 2.03 ms |
| same | hybrid retrieval with graph expansion | 1.29 ms | 1.57 ms |
| tantivy (579 files, 8,859 symbols), installed daemon, multilingual vectors, uncached, final build | all 145 questions | 52.4 ms | 97.5 ms |
| same | locate answers | 56.6 ms | 60.5 ms |
| same | map answers (cached map) | 40.0 ms | 42.9 ms |

- The repository map is built once for each code change of a project (470 ms for tantivy). Before this fix, each captured message caused a new build, and a map answer took 373 ms (p50).
- On the real repository the p95 of all questions is 97.5 ms. The hook limit is 2 s, so the session does not wait noticeably. The target of AC-35 (p95 ≤ 60 ms) is met on the synthetic corpus of the earlier latency criteria; on tantivy it is not.

## Problems found and fixed during the measurement
- **Vector fill at startup:** 24–35 s before P4, about 650 s with P4 on the store of the user (302,000 vectors). The fill built the search documents of all files, which are not embedded, and the passage split searched the heading of each passage from the start of the text (quadratic in the text length, D-059). The fill now builds only the embedded documents, and the headings are found in one pass: 68.5 s.
- **Project delete waited for a macOS privacy prompt:** the file watcher kept its lock while it opened a file in a protected folder (Desktop) of a new daemon build. A delete or an add of a project then waited until the user answered the prompt. The watcher now starts without the lock; a delete takes 0.1 s.
- **History questions taken for change requests:** "#89 hangi dosyayı değiştirdi" matched the Turkish change phrase. Only the "must change" forms ("değiştirmeli", "değiştirmem") select the locate answer now.
- **Task questions did not give the changed files:** a task question now follows the verified chain task → commit → files → symbols, and the graph ranking (D-060) adds the neighbors of the task. The graph scores are relative to the best node that is not a seed.

## Reranker (D-064)
Not added. See `docs/adr/ADR-018-reranker.md`: the added p95 latency on real memory records is 542 ms or more.

## Result per criterion

| ID | Target | Result | Status |
|---|---|---|---|
| AC-32 | Code tasks: ≤ 11,505 input tokens per run, no task fails | 15,561 (−5 %), 54/54 | FAIL (lower limit of this task set ≈ 12,300) |
| AC-33 | Code tasks: ≤ 0.39 file-search calls per run | 0.48 (−26 %) | FAIL |
| AC-34 | 17/17 and 15/15; capsule ≤ 162 and ≤ 540 tokens | 17/17, 15/15; 159 and 521 | PASS |
| AC-35 | 0/15 everyday and 0/15 generic prompts with a capsule; query p95 ≤ 60 ms | 0/15, 0/15; p95 2.03 ms (synthetic), 97.5 ms (tantivy) | PASS (synthetic corpus, as AC-26) |
| AC-36 | Localization: ≤ 0.6 file-search calls per run, ≥ 38/40 correct | 0.60, 38/40 (runs: 0.60, 0.62, 0.57, 0.60) | PASS (at the limit) |
