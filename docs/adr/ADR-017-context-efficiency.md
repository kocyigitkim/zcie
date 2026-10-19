# ADR-017 — Context efficiency: fewer tokens, more correct work

- Status: Accepted; implemented in phase P4 (D-055 to D-065). Results: section "Results".
- Date: 2026-10-17

## Context
Research of accepted methods (summary in this ADR, sources at the end) shows one rule. A small context with the relevant facts is both cheaper and better than a large context:
- Accuracy falls by 20 to 30 points when the relevant text is in the middle of a long context ("Lost in the Middle").
- All 18 tested models get less reliable as the input grows, most with near-topic distractors ("Context Rot", Chroma 2025).

ZCIE already has:
- hybrid search (BM25, vectors, RRF);
- extracted memory instead of full histories;
- follow-up context;
- a token-budgeted capsule;
- deferred tool loading;
- prompt-cache friendly injection.

These methods are not yet in ZCIE:

| Method | Source | Reported effect |
|---|---|---|
| Repository map: symbol graph, PageRank, token budget | Aider | the agent finds the right file without file scans |
| Personalized PageRank over a knowledge graph | HippoRAG | up to +20 % on multi-hop questions |
| Context text for each chunk before indexing | Anthropic Contextual Retrieval | retrieval failures −35 % (−49 % with BM25) |
| Reranking of the candidates | cross-encoder | retrieval failures −67 % together with the item above |
| Query-aware compression of context | LLMLingua, sentence filtering | 3× to 9× on conversation text |
| Coarse-to-fine localization: file, then class/function, then lines | Agentless, AutoCodeRover | $0.34–0.70 per SWE-bench issue |
| Server-side work: return results, not raw data | Anthropic code execution with MCP | −78 % to −98 % tokens in tool-heavy work |
| Short tool output with windows and limits | SWE-agent | better results than a raw shell |

The capsule also has a defect that the research explains. On 2026-10-17, the prompt "bunların planını oluştur" got a capsule with unrelated old decisions, two copies of one record, and the same follow-up text twice. Unrelated near-topic text is the type of distractor that "Context Rot" measured.

## Decision
1. **Measure first.** Each change must reduce tokens or improve correctness on a fixed benchmark set, with no loss of correctness. The baseline is recorded before the first change (D-055).
2. **Do the work in the daemon, not in the model.** Search, ranking, graph walks and aggregation run locally, with no model call. The model gets results, not raw data.
3. **The capsule obeys these rules:**
   - a hard token budget for each query class;
   - no duplicate item, full text or follow-up;
   - the best item first and the second best last;
   - no capsule for a short prompt that names no project fact.
4. **Methods and tasks:**

   | Method | Task |
   |---|---|
   | Repository map | D-057, D-058 |
   | Context text for symbols and passages, with no model | D-059 |
   | Personalized PageRank over the ZCIE graph | D-060 |
   | Extractive compression of snippets | D-061 |
   | Localization mode of `zcie_query` | D-062 |
   | Batch expand | D-063 |
   | Local reranker (a spike decides) | D-064 |

5. **No new MCP tool.** ADR-006 permits two tools. The map and the localization are modes of `zcie_query`, and the batch is a form of `zcie_expand`.
6. **New behavior can be turned off.** Each change has a setting in `engine.toml`, and the benchmark compares on and off.

## Targets (acceptance criteria AC-32 to AC-36)
| ID | Target | Benchmark |
|---|---|---|
| AC-32 | Input tokens per solved code task −30 % or more; no task that passed before fails | `benchmarks/claude-ab` code tasks (3 projects, tantivy) |
| AC-33 | File-search calls (Read, Grep, Glob) per code task −40 % or more | same |
| AC-34 | Memory answers stay 100 % (30-stage 17/17, user ideas 15/15); capsule tokens per injected prompt −25 % or more | `benchmarks/memory-30`, `benchmarks/user-ideas` |
| AC-35 | No capsule for 15 everyday prompts and 15 short generic prompts; query p95 ≤ 60 ms (≤ 150 ms with the reranker) | `benchmarks/user-ideas/noise.py`, `benchmarks/end-to-end` |
| AC-36 | For "which file must I change" tasks, file-search calls per run 0.6 or fewer (baseline 1.53), 38 of 40 correct or more | `benchmarks/context/locate.json` (D-055) |

## Consequences
- The plan adds a repository map, graph ranking and compression. The capsule format stays text; clients do not change.
- The reranker needs a local model of 100 to 300 MB. The spike in D-064 decides if its gain is worth the memory and the latency. If it is not, the task stops with a recorded result.

## Results (D-065, 2026-10-17)
Full numbers: `benchmarks/results/context-efficiency-2026-10-17.md`.

| ID | Result | Status |
|---|---|---|
| AC-32 | Code tasks 16,435 → 15,561 input tokens per run (−5 %), 54/54 correct | FAIL |
| AC-33 | Code tasks 0.65 → 0.48 file-search calls per run (−26 %) | FAIL |
| AC-34 | 17/17 and 15/15; capsule 216 → 159 and 720 → 521 tokens | PASS |
| AC-35 | 0/15 everyday and 0/15 generic prompts with a capsule; query p95 2.03 ms (synthetic corpus) | PASS |
| AC-36 | Localization 1.53 → 0.60 file-search calls per run, 21,712 → 14,868 tokens (−32 %), 38/40 | PASS (at the limit) |

What the results show:
- Memory answers: the capsule rules, the "no capsule" rule and the importance-based compression give the same answers with about 27 % fewer capsule tokens.
- Change requests: the locate answer removes most searches (−59 % file-search calls, −30 % tokens).
- Code questions: a run with no tool call costs about 9,000 tokens; an edit task costs about 31,800 tokens, because Claude Code permits an edit only after a Read. With these 27 tasks the mean cannot go below about 12,300 tokens. The `[context]` settings do not change the code results more than the run-to-run variation. The two hook changes (the body of a named symbol in the capsule, and the note that the index is current) reduce reads, but some runs still check an exact answer with Grep.
- The reranker is not added (ADR-018).
- Real repository latency: on tantivy the query p95 is 97.5 ms, against 2.03 ms on the synthetic corpus.
- The measurement found and fixed a slow vector fill at startup (650 s → 68.5 s on the store of the user) and a project delete that waited for a macOS privacy prompt. Details: the results file.

Next steps (not in this phase):
- Make the capsule of a code question more complete for "explain" tasks (T7 still reads 4 files).
- Measure AC-32 with a task set that has no edit tasks, or count the edit tasks separately.
- Reduce the locate latency on large repositories (one search instead of two).

## Sources
- LLMLingua: https://www.microsoft.com/en-us/research/blog/llmlingua-innovating-llm-efficiency-with-prompt-compression/
- Aider repository map (secondary): https://github.com/NousResearch/hermes-agent/issues/535
- Contextual Retrieval (secondary): https://www.maginative.com/article/anthropics-contextual-retrieval-technique-enhances-rag-accuracy-by-67
- Lost in the Middle: https://ai-papers.aktagon.com/papers/lost-middle/
- Context Rot: https://trychroma.com/research/context-rot
- HippoRAG and graph RAG comparison: https://arxiv.org/pdf/2503.04338
- Agentless: https://arxiv.org/html/2407.01489v2
- AutoCodeRover: https://arxiv.org/html/2404.05427v3
- Code execution with MCP: https://www.anthropic.com/engineering/code-execution-with-mcp
- Advanced tool use: https://anthropic.com/engineering/advanced-tool-use
- Cross-encoder and late interaction: https://arxiv.org/abs/2406.17968v1
