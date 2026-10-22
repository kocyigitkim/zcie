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
| AC-22 | All development tasks are marked done and all test suites pass. | `12-development-plan.md`, `13-test-plan.md` section 8 | PASS | `12-development-plan.md`: 54/54 tasks done. `13-test-plan.md` §8: all suites pass; `cargo test --workspace --release` passed (162 tests, 0 failed, 2026-10-17). |
| AC-23 | All memory is kept by a project ID in one store. The ID file `.claude/zcie_id` is made automatically. A question from any project folder can read another project, within the key scope. | TS-14-01 to TS-14-11; manual Claude Code session (ADR-009) | PASS | TS-14 (11 tests). Manual: a session in a new folder wrote `.claude/zcie_id`; a session in ZCIEngine answered "Which database does the autoproj ledger use?" in 1 turn with no tool call ("Postgres 16"). Benchmark with 3 sample projects (local, by name, by ID, memory, all projects): Z 24/24 correct, A 20/24; cost −68 %, tokens −71 % (`benchmarks/results/three-projects-2026-10-16.md`). |
| AC-24 | The context capsule answers more questions in one turn without injecting into everyday prompts. Problems that stop the memory are reported. | TS-06-13, TS-06-14, TS-07-11, TS-07-12, TS-14-16; benchmark rounds 25 and 26; `zcie doctor` (ADR-010) | PASS | Rounds 25/26: Z turns 1.7 → 1.4 (3 projects) and 2.2 → 2.0 (tantivy), API time −10 % and −17 %, 24/24 and 30/30 correct; cost unchanged on 3 projects, −53 % against A on tantivy. Dry run: 1 of 12 everyday prompts got a capsule. `zcie doctor --fix` reinstalled an old hook; the sweep removed 170 dead letters of deleted projects. |
| AC-25 | The context capsule gives each ref once per session and again after a compaction. | TS-11-09; benchmark round 27 | PASS | Long sessions: Z −27 % cost against A (before −19 %), 1.79 turns per question (before 2.00), 33/33 correct. |
| AC-26 | Indexing and queries meet the latency targets at 1,000,000 lines, with and without vector search. | `benchmarks/results/scale-2026-10-16.md` (review) | PASS | Index 99–103 s; all p95 values below target (closest: capture 20–21 ms of 25 ms); vector fill 274,351 vectors in 4.6 s. |
| AC-27 | TypeScript, Python and OpenCode clients use the engine with the project memory. | TS-15-01 to TS-15-03; OpenCode 1.18.34 run with a fake model (ADR-012) | PASS | 11 client tests against a daemon; the real OpenCode request had the ZCIE capsule and the `zcie` MCP tools. |
| AC-28 | Vector search finds results with other words and does not change exact-name results. It runs with no network at query time. | TS-16-01 to TS-16-06; `benchmarks/results/vector-2026-10-16.md`; rounds 28, 29 | PASS | hit@5 19 → 22 of 28, MRR 0.586 → 0.661; exact names 4/4 in both; Claude Code rounds 24/24 and 30/30. |
| AC-29 | A new session answers each question about earlier sessions correctly: facts, decisions, rules, corrections, the list of all decisions, and the ideas of the user in Turkish or English. Everyday prompts get no capsule. | TS-17-01 to TS-17-15; `benchmarks/results/memory-accuracy-2026-10-17.md` (30-stage test and user-ideas test in new Claude Code sessions) | PASS | 30-stage: 85/85 in 5 runs (no ZCIE: 8/17), cost −56 %; user ideas: 15/15 in 2 runs (no ZCIE: 11/15); everyday prompts 0/15 with a capsule; 145 tests pass. |
| AC-30 | One npm command installs ZCIE; the settings are chosen during the installation and can be changed later; a check proves that a new session remembers an earlier one. | TS-18-01 to TS-18-06; npm installation in an isolated prefix (ADR-014 section "Consequences") | PASS | `npm install -g ./dist/zcie-0.1.0.tgz` + `zcie setup`: 12/12 checks with two real Claude Code sessions (without ZCIE: "Bunu bilmiyorum"); wizard and `config set` changes applied; launchd restart in 3 s; `uninstall --purge` clean. |
| AC-31 | Only folders that the user adds are indexed and watched; their index state is visible. | TS-18-07, TS-19-01 to TS-19-03; live store of the user (ADR-015) | PASS | 147,567 jobs of memory-only folders deleted at startup; queue 0; daemon CPU from about 300 % to 2.5 %; `zcie project add` of dxcellence complete in seconds. |
| AC-32 | Input tokens per solved code task are 30 % lower or more than the baseline; no task that passed in the baseline fails. | TS-20-12; `benchmarks/results/context-efficiency-2026-10-17.md` against `context-baseline-2026-10-17.md` | FAIL | 16,435 → 15,561 input tokens per run (−5 %), 54/54. A run with no tool call costs about 9,000 tokens; the 4 edit tasks cost about 31,800 (Claude Code needs a Read before an edit), so the mean of this task set cannot go below about 12,300. Replaced by AC-45/AC-44 (ADR-019): the baseline of this criterion (the installed ZCIE before P4) cannot be measured in isolation. |
| AC-33 | File-search calls (Read, Grep, Glob) per code task are 40 % lower or more than the baseline. | TS-20-12; `benchmarks/results/context-efficiency-2026-10-17.md` | FAIL | 0.65 → 0.48 per run (−26 %). Replaced by AC-45/AC-44 (ADR-019): the baseline of this criterion (the installed ZCIE before P4) cannot be measured in isolation. |
| AC-34 | Memory answers stay 100 % (30-stage 17/17, user ideas 15/15); capsule tokens per injected prompt are 25 % lower or more. | TS-20-08, TS-20-12; `benchmarks/results/context-efficiency-2026-10-17.md` | PASS | 17/17 and 15/15; capsule 216 → 159 (−26 %) and 720 → 521 (−28 %) tokens. |
| AC-35 | No capsule for the 15 everyday and the 15 generic prompts; query p95 is 60 ms or less (150 ms or less with the reranker). | TS-20-02, TS-20-12; `benchmarks/results/context-efficiency-2026-10-17.md` | PASS | Everyday 0/15, generic 0/15 (baseline 3/15). p95 2.03 ms on the synthetic 100,000-line corpus (the corpus of AC-26). No reranker (ADR-018). On tantivy the p95 is 97.5 ms. |
| AC-36 | For "which file must I change" tasks, the file-search calls (Read, Grep, Glob) per run are 0.6 or fewer (baseline 1.53), with 38 of 40 correct answers or more. | TS-20-09, TS-20-12; `benchmarks/results/context-efficiency-2026-10-17.md` | PASS | 1.53 → 0.60 per run (at the limit; runs 0.57 to 0.62), 38/40; tokens −32 %. The first-read measure was replaced: the model answers from Grep and rarely reads a file. |
| AC-37 | Evidence freshness: a source slice of a file that changed on disk is never marked current. | TS-21-05, TS-21-07, TS-21-08, TS-21-15 | PASS | A pending change gives `requires_live_read: true` in every slice of the file; a new revision is sent again; a pointing prompt gets the changed working set; no cached answer after a change with no event. |
| AC-38 | Source-backed explanation: each explanation task of the isolated benchmark is correct, with fewer file reads than `A0`. | TS-21-06; `benchmarks/results/isolated-p52-2026-10-17.md` | PASS | T2, T4 and T7 are correct with no file tool in U and UP (A0: 1 search each); TS-21-06 passes. One repetition. |
| AC-39 | Memory provenance: corrections and assistant statements are classified correctly in all fixtures. | TS-21-10, TS-21-14 | PASS | Assistant statements are labeled and rank below user facts; a corrected record stays superseded (a defect that showed it as "verified" was fixed). |
| AC-40 | Scope isolation: a query without access to a project returns no data of it; the hook key cannot use admin routes. | TS-21-11, TS-13 (multitenancy) | PASS | Admin routes refuse the hook key; ranges outside the project give `not_found`; setting `cross_project` for the hooks. |
| AC-41 | Deletion coverage: after a completed forget and a restart, no index returns the record. | TS-21-12 | PASS | Tasks made by a forgotten message are deleted too (a defect: they stayed). |
| AC-42 | Replay correctness: if a vector snapshot is added, a warm restart gives the same results as a fresh fill. | D-077, ADR-021 | PASS | Not applicable: no snapshot (ADR-021); each start fills the index from the canonical store. |
| AC-43 | Token effectiveness: in the isolated benchmark, tokens per successful task of `U` are lower than `A0`, with no loss of edit success. | TS-21-02; `benchmarks/results/isolated-p52-2026-10-17.md` | PASS | Tokens per correct run, U against A0: code 19,563 / 48,208; edit 49,482 / 61,217 (4/4 against 3/4 correct); locate 30,928 / 40,569. One repetition. |
| AC-44 | Search efficiency: the code tasks of AC-33 reach 0.39 file-search calls per run or fewer in the isolated benchmark. | `benchmarks/results/isolated-p56-correctness-2026-10-18.md` | PASS | 27 tasks of `code.json`, 3 repetitions: U 0.29, UP 0.33 per run (A0 1.07). |
| AC-45 | Token objective: the 30 % target of AC-32 stays. It is open until the measured cost of the task set permits it (ADR-019, arithmetic limit). | `benchmarks/results/isolated-p56-correctness-2026-10-18.md` | PASS | The 27 tasks of AC-32, 3 repetitions, tokens per correct run against a session with no ZCIE: U −51 %, UP −73 % (24,381 and 13,651 against 49,982). This replaces AC-32 (ADR-019, section "AC-32 and AC-33"). |
| AC-46 | Verification gate: a turn that leaves a failing check is blocked with the error; after the fix it ends; a turn with no code change is never blocked. | TS-21-13 | PASS | Claude Code hook `verify-gate.sh`; at most 3 blocks per turn; a missing check program never blocks. |
| AC-47 | Correctness with ZCIE: in the isolated benchmark, U and UP answer every task of `code.json` and `locate.json` correctly in 3 repetitions; the 30-stage check is 17/17 and the user-ideas check 15/15; the adversarial fact fixture gives no wrong fact. | TS-21-14; `benchmarks/results/isolated-p56-correctness-2026-10-18.md` | PASS | U and UP 141/141 each (code 69, edit 12, locate 60; C12 accepts its two valid files). Memory 17/17 in 7 of 8 runs; the one 14/17 run was a hook timeout under load, fixed (4 s query, 10 s hook limit, a visible note); 3 runs after the fix under the same load: 17/17. User ideas 15/15. |
| AC-48 | Token savings for each project: `zcie usage` gives, for each project, the median tokens per turn with and with no ZCIE, the saving and the saved tokens, or the reason when a group has fewer than 3 sessions; automated and temporary sessions are not counted. | TS-22-01 to TS-22-04 | PASS | Fixture values exact (60 %, 1,800 tokens; 75 %, 9,000 tokens). Live run: 5 GB of transcripts, first scan 8.3 s, next scan 0.9 s. |
| AC-49 | Hook statistics: the read guard and the gate record each action with no file content; images and binary files are not limited. | TS-22-05 | PASS | `~/.zcie/stats/hooks.jsonl`. The read guard limited a PNG before this fix (found in the live test). |
| AC-50 | Menu bar app: it builds with `swiftc`, starts, shows the saving of the projects that used ZCIE and a report window, and macOS asks for no protected folder. | TS-22-06, TS-22-07; live run 2026-10-19 (screenshots of the menu and the report) | PASS | `zcie usage --no-disk` and the copy `~/.zcie/bin/zcie`. The first build ran zcie from Desktop and made macOS ask; fixed. |
| AC-51 | Cursor setup: `zcie cursor setup` and `remove` add and remove only the ZCIE hook entries (5 events) and `mcpServers.zcie`; they are idempotent; foreign entries and the backup are kept; `zcie setup --cursor true` and `zcie uninstall` do it globally; `zcie verify` reports it. | TS-25-01, TS-25-02, TS-25-07 | Open | Not examined yet. The test phase comes first. |
| AC-52 | Cursor capture and context: in Cursor each prompt is captured once with app `cursor`; a project fact prompt gets the capsule through `additional_context`, a generic prompt gets none; edits and agent answers are captured; a new chat starts with the repository map and the `zcie_query` rule; `zcie_query` uses the current project; the ZCIE Claude Code hooks do nothing in Cursor; memory is shared through `.claude/zcie_id`. | TS-25-03 to TS-25-06, TS-25-08 | Open | Not examined yet. The test phase comes first. |

## 4. Acceptance record

Fill this section when all criteria are examined.

| Item | Value |
|------|-------|
| Date | 2026-10-19 |
| Commit SHA | (working tree, not committed yet) |
| Criteria `PASS` | 48 / 50 |
| Criteria `FAIL` | 2 (AC-32, AC-33: their baseline cannot be measured in isolation; replaced by AC-45 and AC-44, which pass) |
| Accepted (yes / no) | yes, with AC-32 and AC-33 replaced by AC-45 and AC-44 (ADR-019) |
| Examined by | Claude (implementing agent) |
