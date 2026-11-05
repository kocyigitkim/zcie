# Embedding models — 2026-10-24 (ADR-032)

`benchmarks/vector/eval.py <samples> <model folder> <work>` with `ZCIE_VECTOR_MODEL` set to the model; the 3 sample projects of `benchmarks/samples/generate.py`. MRR of the first 10 results with vector search on.

| model | exact | paraphrase | hard | memory | turkish | all | hit@5 |
|---|---|---|---|---|---|---|---|
| potion-multilingual-128M (default) | 1.000 | 0.800 | 0.365 | 0.667 | 0.875 | **0.731** | 31 |
| potion-retrieval-32M | 1.000 | 0.827 | 0.292 | 1.000 | 0.125 | 0.586 | 24 |
| potion-code-16M-v2 | 1.000 | 0.785 | 0.205 | 0.714 | 0.125 | 0.527 | 20 |
| vector search off | | | | | | 0.527 | |

Decision: the default stays. The English models lose the Turkish questions; the code model adds nothing over the lexical search on this set.
