# Context-efficiency benchmark (ADR-017)

- `locate.json`: 20 change requests ("which file must I change so that …"). The expected file of each request is in `expect`. Tantivy tasks use `{tantivy}`, sample tasks use `{samples}` (`benchmarks/samples/generate.py`).
- `run.py run <tasks> <out.jsonl> U,A0 [reps] [parallel]` runs each task in a new Claude Code session. `U` is the installed ZCIE (`zcie setup`); `A0` is the same Claude Code with all hooks off.
- `run.py report <out.jsonl> …` prints a table per condition: correct answers, the first Read on an expected file, input tokens, output tokens, cost, turns and tool calls by name.

The project folders must be added (`zcie project add <folder>`), so that their files are indexed (ADR-015).
