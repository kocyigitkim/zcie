# ADR-032 — MCP 2026-07-28, embedding model, fast rescan, telemetry export, similar tasks, shared memory

- Status: Accepted
- Date: 2026-10-24
- Tasks: D-108 to D-111 of `docs/12-development-plan.md`
- The security items of the research (provenance labels, write gate) are left out on the request of the user (2026-10-24).

## Measurements before the decision
- **MCP 2026-07-28.** `zcie-mcp` uses `rmcp` 3.5.1; its latest protocol version is 2026-07-28. Test (2026-10-24): an `initialize` with 2025-06-18 works; a `tools/list` and a `tools/call zcie_query` with no `initialize`, with the protocol version in `_meta`, return `resultType: complete` and the answer. No change is needed.
- **Embedding model.** `benchmarks/vector/eval.py` on the 3 sample projects, MRR with vector search on: potion-multilingual-128M 0.731 (Turkish 0.875, hit@5 31), potion-retrieval-32M 0.586 (Turkish 0.125), potion-code-16M-v2 0.527 (Turkish 0.125; equal to vector search off). The default model stays. Results: `benchmarks/results/embedding-models-2026-10-24.md`.
- **Rescan.** hellfurylanguage (7,140 files): first index 108 s, a rescan with no change 11 s. Each scan makes one `IndexFile` job for each tracked file; the job reads and hashes the file.

## Decision
1. **Fast rescan instead of a Merkle tree (D-108).** A Merkle tree finds the changed folders between two copies; in ZCIE the code reaches other computers through Git, and each computer indexes its own copy, so no index is compared between computers. The local cost is the read of unchanged files. `FileRecord` gets `mtime_ns` (nanoseconds since the Unix epoch; 0 = unknown). A scan makes no `IndexFile` job for a file whose stored record is not deleted, has the same size, the same nonzero `mtime_ns`, the current `PARSER_VERSION`, and its content blob (if `content_stored`). Watcher events still index the file.
2. **Telemetry export (D-109).** `zcie telemetry export [--days N] [--endpoint URL] [--out FILE]` converts `<home>/stats/activity.jsonl` (and the rotated `activity.1.jsonl`) to OpenTelemetry traces in OTLP/JSON. One span for each line: name `zcie.<op>`; attributes `gen_ai.operation.name` (`execute_tool` for query and expand, else `zcie.<op>`), `gen_ai.tool.name` (`zcie_query`, `zcie_expand`), `zcie.workspace`, and the other fields of the line as `zcie.<field>`. End time = `ts`; start time = `ts` − `ms`. Trace and span IDs come from the BLAKE3 hash of the line. With an endpoint (also `OTEL_EXPORTER_OTLP_ENDPOINT`) the CLI posts batches of 1,000 spans to `<endpoint>/v1/traces`; else it writes the JSON to the file or stdout. The GenAI conventions are still "Development"; the attribute names can change. The HTTP client has no TLS backend: the endpoint must be `http://` (a local collector).
3. **Similar earlier task (D-110).** For an `edit` query, the first memory result whose `evidence_status` starts with `user_requirement` and whose term coverage of the question is 0.6 or more is an earlier request. The query reads the events of its turn (`turn_prefix` of its source event): the paths of `file.changed` events (relative to a worktree root of the project) and the commands of `command.run` events with exit code 0. When the turn changed at least one file, `impact` gets one line: `similar earlier task: "<request, 100 characters>" changed <at most 5 paths>; passed: <at most 2 commands>`.
4. **Shared memory through Git (D-111).** `zcie memory export [--out FILE]` writes the current decisions and preferences of the project (status active or verified, not assistant statements) as JSON lines `{"ref", "kind", "text"}`; default file `.zcie/shared-memory.jsonl` in the project folder. `zcie memory import [FILE]` sends each line as a `user.message` event (app `zcie-share`, idempotency key `share:<ref>`), so the extraction makes the records again. The user commits the file; another computer or a team member imports it.

## Consequences
- A file changed with the same size and the same modification time (a tool that restores the time) is not indexed again by a scan; the watcher or the next real change indexes it.
- An imported decision is a new record of the importing computer; its `valid_from` is the import time, not the original time.
- Plan caching (APC, arXiv 2506.14852) needs a model to adapt a plan; item 3 is the part that needs no model.
