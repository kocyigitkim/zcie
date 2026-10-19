# ADR-020 — Correctness guarantees with ZCIE active

- Status: Accepted; implemented (D-081, D-082). Results: `benchmarks/results/isolated-p56-correctness-2026-10-18.md`.
- Date: 2026-10-17
- Requirement of the user: "When ZCIE is active, the answers and the code that the user asks for must be 100 % correct. Without ZCIE errors can occur; with ZCIE they must not."

## Context
ZCIE does not write the answers or the code; the model does. No component can make every output of a language model correct. ZCIE can control three things:

1. **The facts that it gives.** A wrong, stale or unresolved fact in the capsule makes a wrong answer likely. ZCIE must never give such a fact as true.
2. **The check of the work.** A code change that does not build or does not pass its checks is not a finished change. ZCIE can run the checks of the project when the model stops, and give the failures back to the model before the user sees the turn as done.
3. **The measurement.** The fixed benchmark sets are the evidence. With ZCIE active, each task of each set must be correct in each repetition. Each failure is a defect to remove, not a statistic.

## Decision
1. **Fact correctness (D-082).**
   - A fact in the capsule has a status (`verified`, `inferred`, `candidate`, `superseded`) and a source. Only `verified` and user-stated facts are given as facts; `inferred` and `candidate` items say so in the capsule.
   - An assistant statement never becomes a verified fact automatically, and never supersedes a user statement (D-073).
   - A source slice of a file with a pending change is marked "changed: read it again" (D-069, D-076).
   - A caller is "verified" only when the call resolves through the same file, an import or a qualified path (D-070, D-071).
2. **Verification gate (D-081).**
   - The Stop hook runs the check command of the project when the turn changed files (Edit, Write, MultiEdit, NotebookEdit).
   - The command comes from `.claude/zcie_verify` in the project root, else from detection for each changed file: the nearest `Cargo.toml` → `cargo check --quiet`; the nearest `tsconfig.json` → `npx --no-install tsc --noEmit`; a Python file → `python3 -m py_compile <file>`. After a Bash command, the changed files of the Git working tree are checked too.
   - The hook adds `~/.cargo/bin`, `/opt/homebrew/bin` and `/usr/local/bin` to PATH; a missing check program never blocks but is reported. Claude Code gives the gate 150 s.
   - A failed check blocks the stop (`decision: block`) with the first 60 lines of the output, so the model fixes the code in the same turn. At most 3 blocks per session turn; then the hook reports the failure and stops blocking.
   - A check that takes more than 120 seconds, or a project with no detected check, never blocks.
   - The setting `check` of `zcie setup` (`auto`, `off`; default `auto`) turns the gate on or off.
3. **Measured target (AC-46, AC-47).** In the isolated benchmark, condition U and UP: every task of `code.json` and `locate.json` correct in 3 repetitions; the 30-stage memory check 17/17 and the user-ideas check 15/15.

## Consequences
- A turn that edits code can take longer (the check runs at the end).
- "100 % correct" is a property of the measured sets and of the gate; it is not a proof for every possible prompt. The capsule never claims more certainty than its evidence.
