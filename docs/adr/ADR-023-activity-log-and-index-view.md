# ADR-023 — Activity log of the daemon, index view and cost-weighted savings

- Status: Accepted
- Date: 2026-10-19
- Requirement of the user: "I must see the index details, the index read and write details, everything. And you compute millions: are they real values?"

## Context
1. The daemon kept only process counters (queries, jobs) in memory. The reads and writes of the index for each project were not visible.
2. The savings report (ADR-022) used raw tokens per turn. The values are real sums of `message.usage` of each model call, but about 99 % of them are cache reads. A cache read costs 0.1 of an input token, so a raw token count does not show the cost.
3. `GET /v1/health` walked all blob files for `storage_bytes`: 2.5 s for each call.

## Decision
1. **Activity log.** The daemon appends one JSON line for each operation to `<home>/stats/activity.jsonl` (`zcie_telemetry::ActivityLog`; at 20 MB it moves to `activity.1.jsonl`):
   - `query`: project, client, ms, question (160 characters), class, results, tokens, budget, truncated, cached.
   - `expand`: project, client, ms, refs, first ref, items, tokens.
   - `capture`: project, client, ms, event type, payload bytes, duplicate, seq.
   - `job`: project, kind, target (the path of a file job), ms, ok or error, store keys written, search operations, new jobs.
2. **Index section of `zcie usage`.** `usage/index.rs` reads the log incrementally (cache `<home>/run/usage-index.json`: offset, totals for each project and day, latency buckets, the last 200 operations). It asks the daemon for the health and the state of each project (files, symbols, commits, memories, pending jobs, indexed) at most every 30 s, with a 3 s timeout.
3. **Index page of ZCIE Bar.** Daemon tiles (version, uptime, storage, event sequence and lag, queue, failed jobs); reads and writes of each project (chart); a table of the projects; the last operations, live, filtered by reads or writes. Each project page has an Index section.
4. **Cost-weighted savings.** A session keeps its 1-hour cache writes apart. Cost in input-token units = input + cache write 5 min × 1.25 + cache write 1 h × 2 + cache read × 0.1 + output × 5 (the price ratios of Claude models). The saving uses the median cost per turn. The raw token values stay in the report.
5. `ObjectStore::total_bytes` keeps its result for 60 s.

## Consequences
- The log grows with the indexing work (one line for each indexed file). The rotation keeps at most about 40 MB.
- The log has the questions of the queries (first 160 characters). It stays in the ZCIE home of the user, like the store.
