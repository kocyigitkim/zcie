# LongMemEval retrieval test (ADR-028, D-096)

1. Get the data set (public): `longmemeval_s.json` or `longmemeval_oracle.json` from the LongMemEval repository (https://github.com/xiaowu0162/LongMemEval).
2. Build the daemon: `cargo build --release -p zcied`.
3. Run: `python3 run.py <data.json> <out.jsonl> [limit] [top k]`.

The test counts a hit when the answer text is in the top k results of `/v1/query`. It does not ask a model, so it measures retrieval, not answer quality. Abstention questions and answers longer than 60 characters are skipped. The result is not comparable with the published LongMemEval scores, which grade the answer of a model.
