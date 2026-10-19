# benchmarks/samples

Three sample projects for the multi-project benchmark (ADR-009). See `benchmarks/results/three-projects-2026-10-16.md`.

```sh
cargo build --release --workspace
zcie daemon start
python3 benchmarks/samples/generate.py /tmp/zcie-samples
python3 benchmarks/samples/prepare.py /tmp/zcie-samples /tmp/zcie-samples/tasks.json
AB_CONDITIONS=A,Z python3 benchmarks/claude-ab/run_ab.py /tmp/zcie-samples/tasks.json out.jsonl 2 4
```

To run again on clean projects, delete the projects first: `zcie project delete ledger-rs` (and atlas-ts, orbit-py).
`prepare.py` captures the decisions again; the Z runs of a benchmark capture their prompts as memory.
