# Isolated, paired benchmark (ADR-019, D-067)

The earlier benchmarks (`benchmarks/context`, condition U) use the installed store of the user. Each run stores its prompts as memory of the benchmark projects, so a later run sees the earlier ones. This benchmark removes that effect.

1. Make the frozen home once:
   `python3 snapshot.py <frozen home> <samples dir> <tantivy dir> [model folder]`
   It indexes the 3 sample projects and tantivy with the project IDs of their `.claude/zcie_id` files and stores the seed decisions. `manifest.json` has the tree hash of the home.
2. Run the trials:
   `ZCIE_BENCH_SAMPLES=<samples> ZCIE_BENCH_TANTIVY=<tantivy> python3 run.py <frozen home> <out.jsonl> A0,A0P,U,UP <reps> <parallel> <seed> ../context/code.json ../context/locate.json`
   - Each U/UP trial runs in a copy-on-write clone (`cp -c`, APFS) with its own daemon and port. The clone is deleted after the trial.
   - Claude Code gets only the settings of the condition (`--setting-sources project --settings <file>`). The model is fixed with `AB_MODEL` (default `sonnet`).
   - The seed selects the order of the conditions of each task and repetition.
   - The run stops if the frozen home differs from its manifest, before or after the run.
3. Report: `python3 report.py <out.jsonl> [A0]` gives the means per task group and condition, and the paired differences to the baseline with 95 % bootstrap intervals.

Conditions: `A0` no hooks; `A0P` no hooks and the tool profile `auto` (6 heavy tools denied, tool search on); `U` the ZCIE hooks; `UP` the ZCIE hooks and the profile `auto`. The difference `A0P − A0` is the effect of the tool profile; `U − A0` is the effect of the context; `UP − A0` is both.
