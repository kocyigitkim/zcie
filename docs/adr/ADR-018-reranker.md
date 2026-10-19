# ADR-018 — Local reranker: not added

- Status: Accepted (result of the spike D-064)
- Date: 2026-10-17

## Context
ADR-017 lists reranking with a cross-encoder as a method that can reduce retrieval failures. D-064 is a spike. It finds out if a local cross-encoder improves the ranking of ZCIE enough to justify its memory and its latency.

The rule of D-064: keep the reranker only if both conditions are true:
- hit@1 improves by 3 questions or more;
- the added p95 latency is 100 ms or less.

## Method
- Script: `benchmarks/rerank/spike.py`. The script only reads; it sends queries and never sends events.
- The daemon gives the top 30 results of each question. The cross-encoder scores each pair (question, title + snippet), with at most 256 tokens for each pair.
- Two orders are measured:
  - **reranked**: the order of the cross-encoder scores;
  - **blended**: reciprocal rank fusion of the search order and the reranker order (k = 10, not tuned).
- Runtime: ONNX Runtime on the CPU (4 threads), on the computer of the user (Apple Silicon). A Rust integration would use the same runtime (`ort`), so the latency is representative.
- Question sets:
  - **vector**: the 36 labelled questions of `benchmarks/vector/eval.py` (sample projects, multilingual vectors on, P4 context text on). Short snippets.
  - **ideas**: the 15 Turkish questions of `benchmarks/user-ideas/questions.json`, on the memory of the ZCIEngine project in the installed daemon. Long snippets (conversation records).
- Models (permissive licenses, 300 MB or less):
  - `cross-encoder/mmarco-mMiniLMv2-L12-H384-v1`, multilingual, int8, 119 MB, Apache-2.0;
  - `jinaai/jina-reranker-v1-turbo-en`, English, int8, 38 MB, Apache-2.0.

## Results

| Set | Order | hit@1 | hit@5 | MRR | Rerank p50 / p95 |
|---|---|---|---|---|---|
| vector (36) | search (no reranker) | 23 | 30 | 0.719 | — |
| vector | mmarco, reranked | 14 | 25 | 0.533 | 131 / 141 ms |
| vector | mmarco, blended | 20 | 31 | 0.691 | 131 / 141 ms |
| vector | jina-turbo, reranked | 18 | 27 | 0.618 | 74 / 87 ms |
| vector | jina-turbo, blended | 26 | 33 | 0.789 | 74 / 87 ms |
| ideas (15) | search (no reranker) | 12 | 15 | 0.900 | — |
| ideas | mmarco, reranked | 12 | 15 | 0.889 | 803 / 866 ms |
| ideas | mmarco, blended | 13 | 15 | 0.933 | 803 / 866 ms |
| ideas | jina-turbo, reranked | 13 | 13 | 0.867 | 532 / 542 ms |
| ideas | jina-turbo, blended | 12 | 14 | 0.865 | 532 / 542 ms |

Notes:
- A reranked order alone is worse than the search order for both models. The models are trained on web passages, not on code symbols and short memory records.
- Only one combination meets the hit@1 condition: jina-turbo, blended, on the vector set (+3). On the ideas set the same combination loses one hit@5.
- On real memory records the snippets are long. The added p95 latency is then 542 ms or more. That is more than 5 times the limit.
- The same day, the P4 context text (D-059) improved the search order without a reranker: hit@1 21 → 23, hit@5 27 → 30, MRR 0.659 → 0.719 (`benchmarks/vector/eval.py`, multilingual vectors on).

## Decision
1. ZCIE does not add a local reranker. No crate `zcie-rerank` and no setting `[context] rerank`.
2. The latency condition fails on real data (p95 542 ms or more against 100 ms). The quality condition is met by one combination on one set only.
3. The spike script stays in `benchmarks/rerank/spike.py`, so that a later model can be measured with the same method.

## Consequences
- The query p95 target of AC-35 is 60 ms (the reranker variant of 150 ms does not apply).
- No model download and no added memory for the user.
- Examine again only if a code-aware multilingual cross-encoder of 100 MB or less is available, or if the snippets are cut to about 64 tokens before reranking. Then the same rule applies.
