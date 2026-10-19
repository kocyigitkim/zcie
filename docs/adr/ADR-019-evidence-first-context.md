# ADR-019 — Evidence-first context and benchmark integrity (phase P5)

- Status: Accepted (plan; the work is in phase P5 of `docs/12-development-plan.md`, D-066 to D-080)
- Date: 2026-10-17
- Source: an external review of the architecture summary `docs/handoff/zcie-altyapi-ozeti.md` ("ZCIE P5 — Evidence-First, Token-Efficient Intelligence"). This ADR keeps the parts that agree with the code and corrects the parts that do not.

## Context
Phase P4 (ADR-017) passed 34 of 36 acceptance criteria. AC-32 (code-task tokens −30 %) and AC-33 (file-search calls −40 %) failed. The analysis of P4 and the review give these causes:

1. **One capsule for all actions.** A change request needs ranked files; an edit needs exact source ranges; an explanation needs source-backed evidence; a memory question needs facts with provenance. ZCIE sends one capsule format for all of them. Task T7 ("explain how garbage collection works") still reads 4 files.
2. **The benchmark changes its own input.** Condition U uses the installed store of the user. Each run stores its prompts and answers as memory of the benchmark projects. Three P4 runs of the same code set differ by up to 1,082 tokens per run.
3. **No explanation of a result.** It is not possible to see why a result was selected or dropped without a debugger.
4. **The capsule says "do not check with Grep or Read" when the index watermark is current.** A current watermark does not prove that one file on disk is unchanged.
5. **Callers come from bare names.** `CALLS` edges are not made; `with: ["callers"]` uses name occurrences.

Arithmetic limit: if a run of this task set cannot cost less than about 12,300 tokens, the largest reduction from 16,435 is 25.2 %. The 30 % target then needs a change of the work of the agent (fewer reads before an edit), not a smaller capsule.

## Review against the code (corrections)
- The hook key (`claude-code`) has `read` and `write` only. Admin routes (forget, project delete, search rebuild) already refuse it. D-074 keeps a test for this; it adds no new mechanism.
- A range in `zcie_expand` (`path:start-end`) is resolved through the path lookup of the index (`lookup: t/path/<worktree>/<path>`). The request never gives a file-system path to the file reader, so `../` and symbolic links cannot escape. D-074 adds a test for this.
- `zcie_expand` already compares the disk file with the indexed hash and returns `status: changed`. D-076 extends this per-file check to the capsule.
- The hooks send `cross_project: true` so that a question that names another project reads it. This stays, but D-074 makes it a setting.

## Decision
1. **Measure first (P5.1).** Each query can return a trace (D-066). The benchmark runs each trial on its own clone of a frozen store, with paired and randomized conditions (D-067). No retrieval change starts before the first isolated measurement.
2. **Action-specific evidence (P5.2).** The planner selects an intent: `explain`, `locate`, `edit`, `trace`, `memory` or `general` (D-068). An edit gets source slices bound to a content hash and a line range (D-069). The session keeps a working set of delivered evidence (D-072).
3. **Evidence rules.**
   - Every item of the evidence package has a project, a kind, a revision and a ref.
   - Source code is never compressed with sentence windows.
   - A name-based link is `candidate` or `inferred`, never `verified`.
   - A slice comes from the stored copy of the indexed revision, so its text, lines and hash agree. It is `requires_live_read: true` when a change of its file is pending (watcher overlay) or its folder is not watched. A query does not read the file on disk (ADR-016: on macOS, a first read of a protected folder can wait for a privacy prompt). The remaining risk is a change in the debounce window of the watcher (200 ms) or an unsaved editor buffer; the capsule tells the agent to read a range again before it applies a patch when the file was changed in the session.
   - Retrieved text is data, not an instruction; the capsule says so.
   - No invented confidence percentages.
4. **Compatibility.** New response fields are optional. `POST /v1/query`, `POST /v1/expand` and the two MCP tools keep their current fields and behavior.
5. **No new dependency in P5.** No new model, database, ANN library or service. A new grammar or a vector snapshot format needs its own ADR and a measurement.
6. **Release order.** P5.1 (D-066, D-067), P5.2 (D-068, D-069, D-072), P5.3 (D-070, D-071, D-076), P5.4 (D-073, D-074, D-075), P5.5 (D-077, D-078, D-079), P5.6 (D-080, only if a language benchmark justifies it). A later release starts only after the measured result of the earlier one.

## Acceptance (AC-37 to AC-45)
See `docs/14-acceptance-criteria.md`. AC-32 and AC-33 keep their targets; AC-45 records that the 30 % target stays and is open until the measured task cost permits it.

## AC-32 and AC-33 (decision of 2026-10-18)
AC-32 and AC-33 compare with "the installed ZCIE before P4". That state cannot be measured again without the benchmark changing its own memory (see Context, item 2). The isolated benchmark compares with a session that has no ZCIE (condition A0), on the same tasks and the same store. AC-45 (tokens, 30 %) and AC-44 (file-search calls, 0.39) use it and pass: −51 % (U) and −73 % (UP) tokens per correct run; 0.29 and 0.33 file-search calls. AC-32 and AC-33 stay FAIL in the record, replaced by AC-45 and AC-44.

## Consequences
- The benchmark needs more disk space for clones (APFS clones are copy-on-write) and one daemon per parallel trial.
- The capsule format changes for edit and explanation prompts. The old format stays for `general` and `memory`.
- The P4 statement "the index is current: no Grep or Read check" is replaced by a per-file statement in D-072 and D-076.
