# ADR-021 — Decisions of P5.5 and P5.6: vector warm start, tool output, languages

- Status: Accepted
- Date: 2026-10-17
- Tasks: D-077, D-079, D-080 of `docs/12-development-plan.md` (ADR-019 asks for a measurement before each of them).

## D-077 — Vector warm start: no snapshot
Measurement on the store of the user (302,000 vectors, multilingual model):

| State | Fill at startup | Queries during the fill |
|---|---|---|
| After P4 (one write lock for each vector) | 68.5 s with no load; 404 s with queries | about 700 ms each |
| After the batched fill of P5.2 (512 vectors under one lock) | 80 s with a query every 0.5 s | p50 27 ms, p95 103 ms |

Decision: no vector snapshot file and no `vector_seq` watermark. The vector index is a view that the fill builds from the canonical store at each start; there is no stored vector state to version. The batched fill keeps queries fast during the fill. A snapshot would add a file format, a checksum and a replay path for a gain of about 80 s once per daemon start. AC-42 ("a snapshot gives the same results as a fresh fill") does not apply.

## D-079 — Shorter tool output: not added
A PostToolUse hook can replace the output of a Read (`updatedToolOutput`). The only safe case is a Read of lines that the capsule already gave unchanged. In the isolated benchmark the edit tasks read files that the capsule did not give (E1: the whole `index_writer.rs`; E3: all uses of a name). Shortening such a Read can hide code that the edit needs. ADR-020 puts correctness first. Decision: no change of tool output. The read guard (400 lines, symbol outline) stays the only limit.

## D-080 — Languages: no new grammar
Files of the indexed projects of the user (Git-tracked, by extension):

| Project | Files with symbol parsing (ts, tsx, py, rs) | Other code | Data and documents |
|---|---|---|---|
| d-xcellence | 4,180 (ts 3,209, tsx 962, py 9) | sql 228, cs 14, mjs 27 | json 682, md 392, … |
| xavier | 705 (ts 467, tsx 238) | sql 26, js 3, mjs 4, sh 2 | json 56, md 36, … |

TSX already uses the TSX grammar. C# has 14 files (0.2 % of the code). SQL files are found by the lexical index; they define no functions that the agent calls. Decision: no new grammar crate now. Measure again when a project of the user has more than 5 % of its code in an unsupported language.

## Limitation recorded with ADR-020
The verification gate (D-081) is a Claude Code Stop hook. The OpenCode plugin has no gate yet; OpenCode sessions get the evidence and the labels of D-082, but no automatic check.
