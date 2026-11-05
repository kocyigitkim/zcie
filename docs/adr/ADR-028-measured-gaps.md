# ADR-028 — Measured gaps: HFL symbols, generated files, stale memory, read window, benchmarks

- Status: Accepted
- Date: 2026-10-23
- Tasks: D-090 to D-096 of `docs/12-development-plan.md`

## Context
`zcie usage --days 7` (2026-10-23) shows sessions with ZCIE that cost more than sessions with no ZCIE in three projects: hellfurylanguage +17 %, container-deploy +73 %, muhammet +13 %. Each group has 1 to 4 sessions, so these values are signals, not proof.
- hellfurylanguage has 1,608 Git-tracked `.hfl` files (about 10 % of its code files). ZCIE has no symbol extractor for `.hfl`. ADR-021 (D-080) says: measure again when an unsupported language has more than 5 % of the code. This limit is now true.
- hellfurylanguage has 3,844 Git-tracked `.map` files (source maps). They are generated text and add noise to the lexical index.
- A memory record can name a file ("K1 is in `bm25.rs:9`"). When the file changes later, the record can be false. ZCIE marks a record as superseded only after an explicit correction.
- The read guard keeps lines 1 to 250 of a large file. The part that the task needs is often lower in the file.
- The isolated benchmark has tasks for the sample projects and tantivy only. Its report shows the sum of input tokens, not the cache split.
- ZCIE has no memory test from outside the project.

## Decision
1. **HFL symbols (D-090).** `.hfl` files get the language `hfl`. A line-based extractor (no Tree-sitter grammar exists) finds `func`, `public func`, `native func`, `struct`, `class`, `enum`, top-level `const` and `global var` declarations, `import "<path>"` lines, call sites (`name(`, `x.name(`) and used names. A declaration ends before the next line with the same or a smaller indent. `hfl` is in the default languages. `PARSER_VERSION` becomes 5, so all files are parsed again once.
2. **Generated files (D-091).** The built-in exclusions get `**/*.map`, `**/*.min.js` and `**/*.min.css`. They apply also to Git-tracked files.
3. **Stale memory (D-092).** When a memory record names a path of an indexed file, and that file was indexed again after the source event of the record (`FileRecord.indexed_seq` >= event `seq`; a watcher job takes the newest event seq, and an unchanged file is never indexed again), the result gets the status `possibly_stale`. The status weight of `possibly_stale` is 0.5 (other). The capsule shows `[possibly stale: the named file changed after this was said; check the file]`. The status is computed at query time; it is never stored.
4. **Read window (D-093).** The user chose to apply read pruning (2026-10-23). For a Read of more than 400 lines with no offset and no limit, the read guard reads 250 lines that start 20 lines above the first range of the session working set in the same file. With no such range, it reads lines 1 to 250 as before. The note says which lines were read and gives the outline. The model can read other lines with offset/limit or `zcie_expand`; no line is hidden without a note.
5. **Benchmark of hellfurylanguage (D-094).** `snapshot.py` indexes the folder of `ZCIE_BENCH_HFL` when it is set. `load_tasks` accepts `{hfl}` in `cwd`. `benchmarks/context/hfl.json` has locate and explain tasks for hellfurylanguage.
6. **Cache split (D-095).** `report.py` shows cache creation, cache read and the weighted cost per run (input ×1, cache creation ×1.25, cache read ×0.1, output ×5, the weights of `zcie usage`).
7. **External memory test (D-096).** `benchmarks/longmemeval/run.py` loads LongMemEval questions (JSON of the public data set), stores the history sessions of each question as events in a new ZCIE home, asks the question with `/v1/query`, and counts a hit when the answer text is in the returned results. This measures retrieval only, not the answer of a model.

## Consequences
- The HFL extractor reads lines, not a syntax tree. A declaration inside a multi-line string can give a false symbol. `quality` is always `complete`.
- The next scan of a worktree removes the records of the newly excluded files (`scan_worktree` makes a `DeleteFile` job for each stored path that the scan does not list).
- `possibly_stale` can mark a record whose file changed in an unrelated part. The label tells the model to check the file; it does not hide the record.
- The read window changes which 250 lines the model gets first. It never gives fewer lines than before.
