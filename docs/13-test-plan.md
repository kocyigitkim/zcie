# 13 — Test Plan

Status: Normative.

> **Warning:** Start this phase ONLY after all development tasks `D-001` to `D-044` are complete.
> Do not write or run tests before that point. See `docs/adr/ADR-007-tests-after-development.md`.

## 1. Entry conditions

Make sure that all these conditions are true before you write the first test:

1. All tasks in `docs/12-development-plan.md` show `- [x] Done`.
2. `cargo build --workspace --release` completes with no errors.
3. `cargo clippy --workspace -- -D warnings` completes with no errors.

If a condition is false, go back to the development plan. Do not start the test phase.

## 2. Test procedure

1. Write the tests in the order of section 5: `TS-01` first, then `TS-02`, and so on.
2. Write all tests of one test suite. Then run that suite.
3. If a test fails, find the cause. Fix the code, not the test. Change a test only if the test disagrees with a document.
4. Run all suites again after each code fix: `cargo test --workspace`.
5. Record each suite result in section 8.
6. When all suites pass, run the benchmarks in section 7. Record the results.
7. Go to `docs/14-acceptance-criteria.md`.

> **Caution:** Do not delete a failing test to make the suite pass. Do not mark a test `#[ignore]` without an entry in section 8 with the reason.

## 3. Test rules

| ID | Rule |
|----|------|
| R-01 | Each test uses its own temporary folder (`tempfile::TempDir`). No test uses `~/.zcie/`. |
| R-02 | Each test is independent. The order of tests does not change the result. |
| R-03 | Each test name says the expected behavior. Example: `capture_with_same_idempotency_key_returns_same_event`. |
| R-04 | Each test checks one behavior. |
| R-05 | Tests do not use the network, except loopback HTTP to a daemon that the test starts. |
| R-06 | Tests do not call a language model. |
| R-07 | Use the fixture repository in `tests/fixtures/` for code tests. |
| R-08 | Keep each test function at 60 lines or fewer. Put shared setup in a helper module. |

## 4. Test locations

| Kind | Location |
|------|----------|
| Unit tests | `#[cfg(test)] mod tests` at the end of the source file. |
| Integration tests of one crate | `crates/<crate>/tests/*.rs`. |
| Cross-crate and end-to-end tests | A test crate `tests/integration` (crate name `zcie-tests`). Add it to the workspace members. |
| Crash tests | `tests/crash-recovery/` (inside the `zcie-tests` crate). |
| Fixtures | `tests/fixtures/` |

### 4.1 Fixture repository

Create `tests/fixtures/sample-repo/` with this content. A helper copies it into a temporary folder and runs `git init` and commits.

| File | Content |
|------|---------|
| `src/animation/weapon_socket.rs` | `struct WeaponSocket` with method `attach_to_bone`. |
| `src/animation/animation_controller.rs` | `fn update_animation` that calls `attach_to_bone`. |
| `web/weapon-socket.ts` | `class WeaponSocket` with method `attachToBone`. |
| `tools/report.py` | `def build_report` and `class Report`. |
| `docs/notes.md` | Two paragraphs of text. |
| `assets/icon.bin` | 16 bytes with a `0x00` byte (binary file). |
| `.env` | `API_KEY=abc` (excluded file). |
| `src/broken.rs` | A file with a syntax error. |

## 5. Test suites

### TS-01 — Types and keys (unit)

| ID | Test | Expected result |
|----|------|-----------------|
| TS-01-01 | Parse each ID kind with the correct prefix. | Success. |
| TS-01-02 | Parse an ID with a wrong prefix. | Error. |
| TS-01-03 | Serialize and deserialize each record type. | Same value. |
| TS-01-04 | Serialize `Relation::ChangedInCommit`. | `"CHANGED_IN_COMMIT"`. |
| TS-01-05 | `key()` with three parts. | Parts joined with `0x00`. |
| TS-01-06 | Sort `seq_part(9)` and `seq_part(10)` as bytes. | `9` comes first. |
| TS-01-07 | `EvidenceKind::max_status` for each kind. | Agrees with `05` section 7.1. |
| TS-01-08 | `Scope::contains` with a wider and a narrower scope. | Agrees with `11` section 4. |

### TS-02 — Security (unit)

| ID | Test | Expected result |
|----|------|-----------------|
| TS-02-01 | Authenticate with a correct key. | `AuthContext` with the correct principal. |
| TS-02-02 | Authenticate with a wrong key or no header. | `Unauthorized`. |
| TS-02-03 | Redact each pattern in `11` section 5. | The secret is replaced. The count is correct. |
| TS-02-04 | Redact a text with no secrets. | Same text. Count 0. |
| TS-02-05 | `is_excluded(".env")` and `is_excluded("src/main.rs")`. | `true`, `false`. |
| TS-02-06 | `generate_key` two times. | Two different 64-character hex strings. |

### TS-03 — Store and object store

| ID | Test | Expected result |
|----|------|-----------------|
| TS-03-01 | Open a new database. | All nine CFs exist. |
| TS-03-02 | Write a batch with keys in two CFs. Read both. | Both values exist. |
| TS-03-03 | `prefix_scan` with a limit. | Only keys with the prefix. No more than the limit. |
| TS-03-04 | `ObjectStore::put` the same bytes two times. | Same hash. One file. |
| TS-03-05 | Change one byte of a blob file. `get` it. | Hash error. |
| TS-03-06 | `sweep_orphans` with an old unreferenced blob and a referenced blob. | Only the unreferenced blob is deleted. |
| TS-03-07 | Try to decrease a watermark. | The value does not decrease. |

### TS-04 — Capture and jobs

| ID | Test | Expected result |
|----|------|-----------------|
| TS-04-01 | Capture one `user.message`. | Receipt with `durable: true`. The event and the payload exist. |
| TS-04-02 | Capture the same idempotency key two times. | Same `event_id`. `duplicate: true`. One event in the store. |
| TS-04-03 | Capture the same key from 10 threads at the same time. | One event. |
| TS-04-04 | Capture a payload larger than `ingest.max_payload_bytes`. | `PayloadTooLarge`. |
| TS-04-05 | Capture a message with an AWS key. | Stored payload has the marker. `redactions` is 1. |
| TS-04-06 | Capture `assistant.message` with `capture.assistant_messages = false`. | `Forbidden`. |
| TS-04-07 | Make a job handler fail three times. | The job is in `dead` and in `queue/dead-letter/`. |
| TS-04-08 | Run the same `ExtractMemory` job two times. | The same records and edges. No duplicates. |
| TS-04-09 | Fill the queue. Capture again. | The event is durable. The job stays in the CF. |

### TS-05 — Memory and graph

| ID | Test | Expected result |
|----|------|-----------------|
| TS-05-01 | Tier A extraction for each pattern in `08` section 3.1. | The expected record or edge. |
| TS-05-02 | Message "Fix #89 weapon socket". | A `UserRequirement` and a link to task `#89`. |
| TS-05-03 | Add an edge with `Temporal` evidence and status `Verified`. | Stored status is `Candidate`. |
| TS-05-04 | Traverse a path with one `Inferred` edge. | Path status is `Inferred`. |
| TS-05-05 | Traverse with `max_depth = 1`. | No node at depth 2. |
| TS-05-06 | Correct a memory record. | Old record is `Superseded` with `superseded_by`. New record exists. |
| TS-05-07 | Timeline of a task. | Events in sequence order. |
| TS-05-08 | `file.changed` and `user.message` with the same `session_id` and `turn_id`. | `AffectsFile` edge with `ClientTrace` evidence. |
| TS-05-09 | `file.changed` at the same time but a different `turn_id`. | No `Verified` edge. |

### TS-06 — Code intelligence

| ID | Test | Expected result |
|----|------|-----------------|
| TS-06-01 | Discover files in the fixture repository. | `.env` is excluded. `icon.bin` is binary. |
| TS-06-02 | Parse `weapon_socket.rs`. | Symbol `WeaponSocket::attach_to_bone` with kind `Method` and a correct span. |
| TS-06-03 | Parse `weapon-socket.ts`. | Symbol `WeaponSocket.attachToBone`. |
| TS-06-04 | Parse `report.py`. | `build_report` (Function) and `Report` (Class). |
| TS-06-05 | Parse `broken.rs`. | `PartialSyntaxError`. No crash. |
| TS-06-06 | Reparse an unchanged file. | Same symbol IDs. Parse is skipped. |
| TS-06-07 | Delete a function and reparse. | The symbol and its edges are gone from current state. |
| TS-06-08 | Commit with trailer `ZCIE-Task: #89`. | `Verified` task-to-commit edge with `CommitTrailer` evidence. |
| TS-06-09 | Commit with no trailer, made one second after a request. | No `Verified` task-to-commit edge. |
| TS-06-10 | Write one file 20 times in 100 ms. | One `IndexFile` job. |
| TS-06-11 | Index a folder with no `.git`. | Files are indexed. Git features are `unavailable`. |

### TS-07 — Search, query and context

| ID | Test | Expected result |
|----|------|-----------------|
| TS-07-01 | Query "where is attach_to_bone defined". | Exact route. The symbol ref. No search call. |
| TS-07-02 | Query a word that is only in `notes.md`. | Lexical route. The file ref. |
| TS-07-03 | Query two times. | The second answer is a cache hit. |
| TS-07-04 | Change a file. Query before the search commit. | `freshness` is `current` from the overlay, or `partial`. Never old content as `current`. |
| TS-07-05 | Response with a budget of 250 tokens. | Estimated tokens ≤ 250. Valid JSON. |
| TS-07-06 | Many results with a small budget. | `truncated: true` and a cursor. No cut code span. |
| TS-07-07 | Query with no verified edge. | No verified claim. Candidates are separate. |
| TS-07-08 | Expand a symbol ref at level `source`. | The exact span lines. |
| TS-07-09 | Expand with an old `expected_revision`. | Stale error. |
| TS-07-10 | Delete the search folder. Run `rebuild_search`. Query again. | Same results as before. |

### TS-08 — Multitenancy and scope

| ID | Test | Expected result |
|----|------|-----------------|
| TS-08-01 | Client A queries workspace B. | `Forbidden`. |
| TS-08-02 | Graph traversal from workspace A. | No node of workspace B. |
| TS-08-03 | Same query from two principals. | Two cache entries. No shared result. |
| TS-08-04 | Read-only key captures an event. | `Forbidden`. |

### TS-09 — Crash recovery

Run the daemon as a child process. Stop it with `SIGKILL`.

| ID | Test | Expected result |
|----|------|-----------------|
| TS-09-01 | Capture 100 events. Kill after each receipt. Restart. | All 100 events exist. |
| TS-09-02 | Kill during a job. Restart. | The job runs again. No duplicates. |
| TS-09-03 | Kill before the search commit. Restart. | `search_seq` catches up with `graph_seq`. |
| TS-09-04 | Corrupt the search folder. Restart. | The search index is rebuilt. |
| TS-09-05 | Replay all events with the same keys after restart. | No new events. |
| TS-09-06 | Start a second daemon. | It stops with a lock error. |

### TS-10 — Forget

| ID | Test | Expected result |
|----|------|-----------------|
| TS-10-01 | Forget an event. Query its text. | No result. |
| TS-10-02 | Forget before the search delete completes. | The query drops the hit. |
| TS-10-03 | Forget an event. Capture it again with the same key. | `duplicate: true`. Not stored again. |
| TS-10-04 | Forget the only event of a blob. Run the sweep. | The blob file is deleted. |

### TS-11 — Gateways and clients (end-to-end)

| ID | Test | Expected result |
|----|------|-----------------|
| TS-11-01 | HTTP request with no key. | 401 with the error body format. |
| TS-11-02 | HTTP request with a non-loopback `Origin`. | 403. |
| TS-11-03 | Each HTTP route with a valid request. | The expected status and body fields. |
| TS-11-04 | Rust SDK `query` against a running daemon. | Same result as HTTP. |
| TS-11-05 | `zcie-mcp`: list tools over stdio. | Exactly `zcie_query` and `zcie_expand`. |
| TS-11-06 | `zcie-mcp`: call `zcie_query`. | Same result as HTTP. |
| TS-11-07 | Run each Claude Code hook script with the daemon stopped. | Exit code 0 within 3 seconds. |
| TS-11-08 | CLI `zcie query` against a running daemon. | JSON output. |

### TS-12 — Weapon socket scenario (end-to-end)

This suite checks the first milestone definition in `docs/README.md` section 4.1. Use the example in `08-memory-and-graph.md` section 9.

1. Start the daemon. Register the fixture repository.
2. Capture `user.message` "Fix #89: weapon attachment moves during animation" with `session_id = s1`, `turn_id = 1`.
3. Capture `file.changed` for `src/animation/weapon_socket.rs` with `session_id = s1`, `turn_id = 1`.
4. Commit the change with trailer `ZCIE-Task: #89`. Send `git.commit`.
5. Capture `test.result` `passed` with a ref to the task.
6. Stop and start the daemon.
7. Query "what did we do for weapon attachment" from the SDK and from `zcie-mcp`.

| ID | Expected result |
|----|-----------------|
| TS-12-01 | The response contains the task, the file, the symbol, the commit and the test. |
| TS-12-02 | Each link has a status and evidence. The task-to-commit link is `Verified`. |
| TS-12-03 | Each memory item has `source_event_ids`. |
| TS-12-04 | The response has a revision and a freshness value. |
| TS-12-05 | The path can be traversed from the commit back to the user message. |
| TS-12-06 | No language model is called. |

### TS-14 — Projects and central memory (ADR-009)

File: `tests/integration/projects.rs`.

| ID | Test | Expected result |
|----|------|-----------------|
| TS-14-01 | `ensure_project` with no ID, with a known ID, with an unknown valid ID, with a bad ID. | One `proj_` ID; the same ID again; the given ID is kept; an error. |
| TS-14-02 | Project query in project A. Memory exists in A and B. | Results of A only. |
| TS-14-03 | Global query. | Results of A and B, each with a `project` label. No cursor. |
| TS-14-04 | Query in A that names B. | The first result is from B. The note says 2 projects. |
| TS-14-05 | Global query with a key for A only. | No result of B. `list_projects` returns A only. |
| TS-14-06 | Rename A. | The ID does not change. The old name is not found. Names stay unique. The memory stays. |
| TS-14-07 | Delete B with a non-admin key, then with the admin key. | Forbidden; then B is not listed and its memory is not returned. |
| TS-14-08 | `names_project` with whole words, a Turkish suffix, a substring and a short name. | Only whole words of 3 or more characters match. |
| TS-14-09 | Run `session-start.sh` in a subfolder of a new Git repository, two times; then in the home folder. | One ID file in the Git root; the same ID after the second run; no ID file in the home folder. |
| TS-14-10 | `zcie project init` from a subfolder; `zcie project show`; remove the ID file; `zcie project migrate`. | The ID file is in the Git root; show finds the project; migrate writes the same ID again. |
| TS-14-11 | `zcie-mcp` started in a project subfolder with no `ZCIE_WORKSPACE`. Call `zcie_query`, then with `scope: global`. | The first result has only the current project; the global result has both projects. |

## 6. Token regression

Put these tests in `tests/token-regression/`.

| ID | Test | Expected result |
|----|------|-----------------|
| TK-01 | 20 fixed queries with the default budget. | Each response ≤ 250 estimated tokens. |
| TK-02 | Store the estimated token count of each response in `tests/token-regression/baseline.json`. | A later run fails if a count grows by more than 10%. |

## 7. Benchmarks

Run benchmarks after all test suites pass. Benchmarks do not block the acceptance phase. They record values only. All values are targets, not guarantees (principle P-010).

1. Record the computer: CPU, RAM, SSD model, OS version.
2. Use the fixture generator in `benchmarks/corpus-generator/` to make a repository of 100,000 lines.
3. Measure warm-cache and cold-cache values separately.
4. Run each measurement 1000 times. Record p50, p95 and p99.

| Operation | Target p95 |
|-----------|------------|
| Warm exact entity lookup | ≤ 10 ms |
| Warm symbol lookup | ≤ 20 ms |
| Warm lexical and metadata search | ≤ 60 ms |
| Warm graph-neighborhood query | ≤ 40 ms |
| Warm hybrid retrieval without vectors | ≤ 150 ms |
| Durable capture of a small event | ≤ 25 ms |
| Small file change to current index | ≤ 500 ms |
| L0 cache hit | ≤ 1 ms |

Write the results to `benchmarks/results/<date>.md`.

## 8. Test results record

Fill this table during the test phase.

| Suite | Tests | Passed | Failed | Ignored (reason) | Date |
|-------|-------|--------|--------|------------------|------|
| TS-01 | 10 | 10 | 0 | 0 | 2026-10-16 |
| TS-02 | 8 | 8 | 0 | 0 | 2026-10-16 |
| TS-03 | 7 | 7 | 0 | 0 | 2026-10-16 |
| TS-04 | 9 | 9 | 0 | 0 | 2026-10-16 |
| TS-05 | 9 | 9 | 0 | 0 | 2026-10-16 |
| TS-06 | 12 | 12 | 0 | 0 | 2026-10-16 (incl. regression I-32) |
| TS-07 | 10 | 10 | 0 | — | 2026-10-16 |
| TS-08 | 4 | 4 | 0 | — | 2026-10-16 |
| TS-09 | 6 | 6 | 0 | 0 | 2026-10-16 |
| TS-10 | 4 | 4 | 0 | — | 2026-10-16 |
| TS-11 | 7 | 7 | 0 | 0 | 2026-10-16 |
| TS-12 | 1 (6 checks) | 1 | 0 | 0 | 2026-10-16 |
| TS-14 | 11 | 11 | 0 | 0 | 2026-10-16 |
| TK | 2 (1 test fn) | 2 | 0 | — | 2026-10-16 |

## 9. Exit conditions

The test phase is complete when all these conditions are true:

1. All suites in section 5 and section 6 pass.
2. No test is ignored without a reason in section 8.
3. `cargo test --workspace` passes two times in a row.
4. The benchmark results are recorded (section 7).

Then go to `docs/14-acceptance-criteria.md`.
