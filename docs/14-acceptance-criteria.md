# 14 — Acceptance Criteria

Status: Normative.

> **Warning:** Examine these criteria ONLY after the test phase is complete.
> All exit conditions in `docs/13-test-plan.md` section 9 MUST be true first.

## 1. Procedure

1. Make sure that the test phase exit conditions are true. If not, go back to `docs/13-test-plan.md`.
2. Examine each criterion in section 3 in order.
3. Use the evidence source in the "Evidence" column. Most criteria refer to a test ID.
4. Write `PASS` or `FAIL` in the "Result" column. Write the evidence (test name, command output or file) in the "Notes" column.
5. If a criterion is `FAIL`, fix the code. Run all tests again (`cargo test --workspace`). Then examine all criteria again from the start.
6. The first milestone is accepted when all criteria are `PASS`.

> **Caution:** A criterion is `PASS` only with evidence. Do not write `PASS` from a reading of the code alone, except where the "Evidence" column says "review".

## 2. Result values

| Value | Meaning |
|-------|---------|
| `PASS` | The criterion is true. The evidence is recorded. |
| `FAIL` | The criterion is false, or no evidence exists. |
| `N/A` | Not permitted for milestone 1 criteria. |

## 3. Criteria

| ID | Criterion | Evidence | Result | Notes |
|----|-----------|----------|--------|-------|
| AC-01 | Source types and capture permissions are explicit in the configuration and are enforced. | TS-04-06, `config/default.toml`, `11-security.md` | PASS | `assistant_message_is_forbidden_when_policy_disables_it`; capture policy keys in `config/default.toml` [capture]; ADR-008 I-08. |
| AC-02 | Each captured event survives a daemon crash after the durable receipt. | TS-09-01 | PASS | TS-09-01 `captured_events_survive_repeated_kill_and_restart`: 100 events, 10 SIGKILL/restart cycles, all events readable. |
| AC-03 | No event is duplicated after a replay with the same idempotency key. | TS-04-02, TS-04-03, TS-09-05 | PASS | `capture_with_same_idempotency_key_returns_same_event`, `concurrent_captures_with_same_key_store_one_event` (10 threads), `replayed_idempotency_keys_after_restart_are_duplicates`. |
| AC-04 | A file edit is detected. Stale cached results are not returned as current. | TS-06-10, TS-07-04 | PASS | `rapid_writes_to_one_file_give_one_change`; `ts_07_04_changed_file_is_not_returned_as_current_old_content`; manual: watcher indexed a new symbol in ~250 ms. |
| AC-05 | Each retrieved memory item has source provenance and scope. | TS-12-03, TS-05-06 | PASS | TS-12 weapon scenario checks `source_event_ids` of memory records; `correction_supersedes_old_record_and_keeps_it`. |
| AC-06 | User request → task → symbol/file → verified commit can be traversed in both directions. | TS-12-01, TS-12-05 | PASS | TS-12 (TS-12-01, TS-12-05): request → task → file/symbol → commit and back to the message. |
| AC-07 | An unverified association is never described as a verified implementation. | TS-05-03, TS-05-09, TS-06-09, TS-07-07 | PASS | `temporal_evidence_lowers_verified_to_candidate`, `file_change_in_other_turn_makes_no_verified_link`, `commit_without_trailer_makes_no_implementation_link`, `ts_07_07_query_without_verified_edge_makes_no_verified_claim`. |
| AC-08 | Scope controls apply to queries, graph traversal and the cache. | TS-08-01 to TS-08-04 | PASS | TS-08-01..04 (`tests/multitenancy/scope.rs`): query, traversal, cache and write permission. |
| AC-09 | Query results state the revision and the index freshness. | TS-07-04, TS-12-04 | PASS | `ts_07_04_...`, TS-12-04: every response has `freshness`, `watermarks` and item `revision`. |
| AC-10 | No remote model API is called for indexing or exact queries. | TS-12-06; review of `Cargo.toml` dependencies (no model client crate) | PASS | TS-12-06 (no model client in the code path); review: `cargo tree` has no model or embedding client; no non-loopback URL in production code. |
| AC-11 | All common queries work through the two MCP tools only. | TS-11-05, TS-11-06 | PASS | `mcp_gateway_lists_two_tools_and_answers_like_http` (TS-11-05, TS-11-06); TS-12 query through `zcie-mcp`. |
| AC-12 | SDK and HTTP clients use the engine without MCP or Claude Code. | TS-11-03, TS-11-04 | PASS | `each_route_answers_a_valid_request` (TS-11-03), `sdk_query_returns_the_same_results_as_http` (TS-11-04), `cli_query_prints_json`. |
| AC-13 | The search index can be rebuilt from the canonical store. | TS-07-10, TS-09-04 | PASS | `ts_07_10_rebuilt_search_index_gives_same_results`, `corrupted_search_index_is_rebuilt_at_startup`. |
| AC-14 | Corruption, interrupted index updates, deletions, rapid writes and concurrent clients are tested. | TS-03-05, TS-09-02, TS-09-03, TS-10-*, TS-06-10, TS-04-03 | PASS | `corrupted_blob_gives_hash_error`, `killed_jobs_run_again_without_duplicates`, `search_watermark_catches_up_after_restart`, TS-10-01..04, `rapid_writes_to_one_file_give_one_change`, `concurrent_captures_with_same_key_store_one_event`. |
| AC-15 | Model-facing responses obey the token budget. | TS-07-05, TS-07-06, TK-01 | PASS | `ts_07_05_...`, `ts_07_06_...`, TK-01/TK-02 (20 queries ≤ 250 tokens, baseline file). |
| AC-16 | Forgotten data is not returned by any query. | TS-10-01, TS-10-02 | PASS | `ts_10_01_forgotten_event_is_not_returned`, `ts_10_02_forgotten_hit_is_dropped_before_search_delete_completes`. |
| AC-17 | Secrets are redacted before storage. Excluded files are not indexed. | TS-04-05, TS-06-01 | PASS | `capture_redacts_aws_key_before_storage`, `each_redaction_rule_replaces_its_secret`, `discovery_excludes_env_and_marks_binary_files`. |
| AC-18 | Adapter hooks never block the user when the daemon is not available. | TS-11-07 | PASS | `claude_code_hooks_exit_zero_when_daemon_is_down` (each hook, closed port, exit 0 within 3 s). |
| AC-19 | Only one daemon can open the database. | TS-09-06 | PASS | `second_daemon_on_same_home_stops_with_lock_error`. |
| AC-20 | The benchmark report states the corpus, the hardware, cold and warm cache, the latency distribution and the token counts. | `benchmarks/results/<date>.md` (review) | PASS | Review: `benchmarks/results/2026-10-16.md` (corpus, hardware, cold and warm, p50/p95/p99, tokens). All targets met. |
| AC-21 | The code obeys the simplicity rules C-01 to C-17. | Review. Use `cargo clippy -- -D warnings` and a line count per function and file. | PASS | Review: `cargo clippy --workspace --all-targets -- -D warnings` clean; scan found no function > 40 lines and no file > 400 lines in production code; no `unsafe`, no `macro_rules!`. |
| AC-22 | All development tasks are marked done and all test suites pass. | `12-development-plan.md`, `13-test-plan.md` section 8 | PASS | `12-development-plan.md`: 44/44 tasks done. `13-test-plan.md` §8: all suites pass; `cargo test --workspace --no-fail-fast` passed twice (87 tests, 0 failed). |

## 4. Acceptance record

Fill this section when all criteria are examined.

| Item | Value |
|------|-------|
| Date | 2026-10-16 |
| Commit SHA | (working tree, not committed yet) |
| Criteria `PASS` | 22 / 22 |
| Criteria `FAIL` | 0 |
| Accepted (yes / no) | yes |
| Examined by | Claude (implementing agent) |
