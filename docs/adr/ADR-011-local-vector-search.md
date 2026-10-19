# ADR-011 — Local vector search with static embeddings

- Status: Accepted
- Date: 2026-10-16

## Context
The lexical index finds a result only if the question and the result share a word (after identifier split and stemming). A question such as "which relational store holds the account balances" does not find the decision "the ledger keeps all balances in Postgres 16" if the words differ. `docs/12-development-plan.md` section 8 names an OPTIONAL crate `zcie-vector`. Principles: no paid model, no network at query time, small memory, simple code (`docs/00-agent-rules.md`).

Options:

| Option | Model run | Size | New crates |
|--------|-----------|------|------------|
| ONNX transformer (for example fastembed) | Neural network per text, milliseconds | 90 MB+ model, ONNX runtime library | Several, with native code |
| model2vec static embeddings | Sum of token vectors, microseconds | 31 MB (potion-base-8M) | None |

## Decision

### 1. Model
- model2vec `minishlab/potion-base-8M` (MIT), 256 dimensions, BERT WordPiece vocabulary of 29,528 tokens.
- A text vector is the normalized mean of its token vectors. `crates/zcie-vector` reads `tokenizer.json` and `model.safetensors` itself (no new crate): BERT uncased normalization, split at white space and punctuation, WordPiece with `##`.
- `zcie vector install` downloads the 3 files with `curl` from a pinned repository revision and checks each file with a BLAKE3 hash. A file with a wrong hash is not kept.
- The daemon uses the model if `<home>/models/<vector.model>/model.safetensors` exists and `[vector] enabled = true` (the default). Without the model, nothing changes.
- `potion-retrieval-32M` (512 dimensions, 4 times larger) was measured too. It gave no better result on the evaluation set (section 4), so the small model is the default.

### 2. What is embedded
- Symbols: the name and the path split into words ("apply_late_fee" → "apply late fee"), the signature, and the comment lines of the symbol document. The symbol document now includes the doc comments above the symbol (up to 8 lines; attribute lines are skipped). This also helps the lexical index.
- Memory records (except stored requests and observations, which repeat chat), tasks and commits.
- Not embedded: files and raw conversation events (long texts; the lexical index finds them).

### 3. Index
- `FlatIndex`: in memory, one list per workspace, exact cosine search over all vectors of the workspace. Vectors are int8 (`round(v × 127)`): 150,000 symbols use about 40 MB.
- `VectorIndex` is a trait (the exception in rule C-05) so that an approximate index can replace the flat index later.
- Updates: the search writer applies each `SearchOp` to the vector index too (upsert, delete, delete all). The vector index therefore follows indexing, forget and the search rebuild.
- Start: a background thread fills the index from the store with the same document builders as the search rebuild. It does not replace a vector that the writer already made. Measured: 16,188 vectors in 257 ms (all projects of this computer, including 144k lines of tantivy).

### 4. Query
- The search route (and the routes that use it) asks the vector index for the `max_results × 4` most similar refs with a cosine of 0.30 or more.
- Lexical and vector lists are fused with reciprocal rank fusion (k = 60). The fused score, scaled to 1.0, replaces the lexical score. The symbol, exact and task routes do not change.

Evaluation (`benchmarks/vector/eval.py`, 28 labelled questions over the 3 sample projects, results in `benchmarks/results/vector-2026-10-16.md`):

| Group | n | hit@5 off → on | MRR off → on |
|-------|---|----------------|--------------|
| Paraphrase (some shared words) | 13 | 11 → 11 | 0.727 → 0.764 |
| Hard (no shared content word) | 8 | 2 → 4 | 0.181 → 0.281 |
| Memory | 3 | 2 → 3 | 0.500 → 0.778 |
| Exact names | 4 | 4 → 4 | 1.000 → 1.000 |
| All | 28 | 19 → 22 | 0.586 → 0.661 |

## Consequences
- Questions with other words than the code or the memory find more results. Exact-name questions do not change.
- Static embeddings know common words only. Rare words ("tardy", "memoized", "propulsion") still fail: 4 of 8 hard questions have no result in the first 10.
- Memory: the model (31 MB) and the int8 vectors. The live daemon used 45 MB RSS after the fill.
- The flat index reads all vectors of a workspace per query. This is fast for the measured sizes; an approximate index is the next step for millions of vectors.
