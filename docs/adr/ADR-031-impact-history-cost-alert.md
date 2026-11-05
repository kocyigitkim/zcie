# ADR-031 — Change impact, file history and the cost alert

- Status: Accepted
- Date: 2026-10-23
- Tasks: D-105 to D-107 of `docs/12-development-plan.md`

## Context
- `zcie_expand` with `with: ["callers", "tests"]` gives the direct callers and the direct tests of a symbol (D-071). The capsule of an edit prompt does not show them, and a test that calls a caller of the symbol is not found.
- Commits are indexed with their changed paths (200 newest per scan). No result links a symbol to the commits that changed its file.
- `zcie usage` marks a session as an outlier when it costs 3 times the median per turn of its project or more (ADR-024). ZCIE Bar shows this only when the user opens the report.

## Decision
1. **Change impact (D-105).** For a query with the intent `edit`, the response gets `impact`: lines for the first symbol result that is not a test.
   - `caller <path>:<line> <text> [<status>: <reason>]`: the direct callers (at most 5; verified first).
   - `test <path>:<line> <text> [via <caller name>]`: tests that call the symbol, or call a direct caller of it that is `verified` or `inferred` (two steps; at most 5).
2. **File history (D-106).** For the intents `edit`, `explain` and `trace`, `impact` also gets at most 3 lines `commit <short sha> <date> <first line of the message>`: the newest indexed commits whose changed paths have the file of that symbol. The history is per file, not per line: the commit records have no line ranges.
3. The Claude Code capsule shows `impact` under "Change impact of <symbol> (check before you edit)". The lines are evidence of the index; the capsule says that the list can be incomplete.
4. **Cost alert (D-107).** ZCIE Bar sends one macOS notification when a session becomes an outlier between two refreshes: "<project>: this session costs N× the usual cost per turn". Each session gets the notification once. No notification at the first load (old outliers).

## Consequences
- An edit query reads the callers twice deep: about 6 caller resolutions. The measured cost of one resolution is small (repository graph in the cache), but it is not measured for large projects yet.
- A test that reaches the symbol in three or more steps is not listed.
- macOS asks the user once to allow notifications of ZCIE Bar.
