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
| TS-05-10 | A pasted 2,700-character document with "#89" and "STE-100", and a short "Fix #90" message. | Only "#90" makes a task. |
| TS-05-11 | A statement "PostgreSQL 16", an unrelated statement, then "Düzeltme: PostgreSQL 16 değil, 17". | The old statement is superseded, the unrelated one stays active; the first memory result has "17". |
| TS-05-12 | Eight statements, a correction and a question; then "bütün kararları ve kuralları listele". | Every current statement is listed (titles); the corrected one and the question are not. |

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
| TS-06-13 | Parse TypeScript with `const`, `let`, a const arrow function, a destructuring and a local `const` (`crates/zcie-parse/tests/constants.rs`). | Only the top-level `const` is a `Constant`; its signature has the value; the arrow function is a `Function`. |
| TS-06-14 | Parse Python with UPPER_CASE module assignments, a lowercase assignment, a class attribute and a local assignment. | Only the module UPPER_CASE names are `Constant`. |
| TS-06-15 | Index a 2.5 MB source file (limit 1 MB). | A record with the streamed hash (equal to the full hash); no stored content; not parsed. The file is never read whole. |

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
| TS-07-11 | Query with a question whose words are in a symbol name; query with chat words. | `match_coverage` ≥ 0.6; `match_coverage` = 0. |
| TS-07-12 | "Which commit changed …" with no task in the store. | Class `implementation`; the lexical search finds the commit. |
| TS-07-13 | Register two copies of the fixture repository in one project. Query a symbol. | One result per symbol (same path, range and revision). |
| TS-07-14 | A 6,000-character message with one fact in the middle; a question about the fact. | The snippet (≤ 600 characters) has the fact. |
| TS-07-15 | Turkish questions with inflected words ("API'sine", "sınırı", "motorun") against stored facts; "Kullanıcı … istedi". | The facts are found; coverage ≥ 0.6; the user-intent question is class `timeline`. |
| TS-07-16 | Two search rebuilds in a row after a captured decision. | The second rebuild is skipped; the decision stays searchable. |
| TS-07-17 | A 42,000-character pasted document with one decision in the middle; a question about it. Unit: `passages` (`crates/zcie-search/tests/passages.rs`). | A result snippet has the decision. Passages ≤ 1,500 characters, end at sentence ends, cover the whole text. |
| TS-07-18 | One decision and twelve `command.run` events with the same words. | The decision is the first result. |

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
| TS-11-09 | Run the context hook two times with the same prompt and session; then SessionStart with `source: compact`; then the hook again. | The first and the third run inject the capsule; the second run injects nothing. |

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
| TS-14-16 | Dead-letter and pending jobs of a missing project, a tenant job, and a project that is then deleted. | The sweep removes the jobs of the missing project only; the tenant job stays; the delete removes the rest. |
| TS-14-12 | Query in A that contains the project ID of B. | The first result is from B. |
| TS-14-13 | `asks_all_projects`; a query in A with "which of my projects …". | Phrases in English and Turkish match; the results have A and B. |
| TS-14-14 | `stems`; a query over all projects where a third project matches only the question words. | camelCase and paths split; question words are not terms; the project with "Redis" comes first; the noise project is dropped. |
| TS-14-15 | The same idempotency key in two projects; a replay; delete a project and make a new one with the same name. | Two events; the replay is a duplicate; the new project stores the event; no job stays pending. |

### TS-15 — TypeScript and Python SDKs, OpenCode plugin (ADR-012)

File: `tests/integration/sdks.rs`. It starts a daemon and runs the tests of each client against it. A missing `node` or `python3` skips that client with a printed reason.

| ID | Test | Expected result |
|----|------|-----------------|
| TS-15-01 | TypeScript SDK (`sdk/typescript/test/client.test.mjs`): health, project, repository, query, expand; idempotent capture; error status and code; `findProjectId` and `fromEnv`. | 4 of 4 pass. |
| TS-15-02 | Python SDK (`sdk/python/tests/test_client.py`): the same 4 tests. | 4 of 4 pass. |
| TS-15-03 | OpenCode plugin (`adapters/opencode/test/plugin.test.mjs`): the plugin makes the project and the ID file; a capsule as a synthetic part; no second copy in the session; again after `session.compacted`; no capsule for chat; no failure and no ID file when the daemon does not run; the gate rules. | 3 of 3 pass. |

### TS-16 — Vector search (ADR-011)

Files: `crates/zcie-vector/tests/vector.rs`, `tests/integration/vector.rs`. Model tests need the model in `ZCIE_TEST_MODEL_POTION_BASE_8M` or `~/.zcie/models/potion-base-8M`; without it they print a reason and return.

| ID | Test | Expected result |
|----|------|-----------------|
| TS-16-01 | Tokenizer: accents, punctuation, WordPiece pieces, unknown word. | The BERT uncased tokens. |
| TS-16-02 | Identifier split and entity text (doc comments; files are not embedded). | "apply late fee"; the comment is in the text. |
| TS-16-03 | Flat index: upsert, insert-if-absent, other workspace, remove, clear. | The ranks and the counts are correct. |
| TS-16-04 | Model: three paraphrase questions against three code texts. | Each question is nearest to its code. |
| TS-16-05 | Engine with vectors off and on: a memory question with no shared word. | Not found off; rank 1 on. |
| TS-16-06 | Restart (startup fill), then forget the session. | Rank 1 after the restart; not found after the forget. |

### TS-17 — Memory accuracy (ADR-013)

Files: `crates/zcie-vector/tests/vector.rs`, `tests/integration/memory_graph.rs`, `tests/integration/query.rs`, `tests/integration/gateways.rs`. TS-17-04 needs the model in `ZCIE_TEST_MODEL_POTION_MULTILINGUAL_128M` or `~/.zcie/models/potion-multilingual-128M`; without it the test prints a reason and returns.

| ID | Test | Expected result |
|----|------|-----------------|
| TS-17-01 | Unigram tokenizer with a small model: Viterbi pieces, punctuation as a word, an unknown character, a BPE model. | The pieces with the best score; the unknown character is dropped; BPE is refused. |
| TS-17-02 | Unigram normalizer: "…", full-width letters, a no-break space. | "...", ASCII letters, a space. |
| TS-17-03 | Model folder: the default model is missing, `potion-base-8M` is installed; a configured other model. | The installed model is used; a configured model has no fallback. |
| TS-17-04 | Multilingual model: 4 questions (Turkish and English) against 4 texts (code and decisions in the other language). | Each question is nearest to its text. |
| TS-17-05 | Entity text of memory records: a stated requirement, a stored question, an observation, a decision. | The requirement and the decision are embedded; the question and the observation are not. |
| TS-17-06 | "Düzeltme: PostgreSQL 16 değil, PostgreSQL 17 kullanacağız." after a statement with 16 and an unrelated statement. | The statement with 16 is superseded; the unrelated statement stays active; the query gives 17 first. |
| TS-17-07 | "Bütün kararları listele" with 8 statements, a correction and a stored question. | All current statements; no stored question. |
| TS-17-08 | A short follow-up message after a message about tool definitions; a message of another session between them. | The follow-up has the earlier message of its own session as context; a query about tool definitions finds it. |
| TS-17-09 | Context migration on a record with no context. | The migration sets the context once. |
| TS-17-10 | A shell command "! zcie doctor --fix" between two messages. | No memory record for the command; the next message gets the message before the command as context. |
| TS-17-11 | Bilingual equivalents ("toolların" and "araç tanımları"). | The message with "toolların" is the first memory result; match coverage ≥ 0.6. |
| TS-17-12 | Context hook with a `<task-notification>` prompt (in TS-11-09). | No capsule. |
| TS-17-13 | Match coverage for general questions with format words (JSON, YAML, README) and for a project name. | 0.0 for the general questions; at least 0.6 for "PostgreSQL" in a statement; 0.0 for a symbol. |
| TS-17-14 | A decision sentence of a long pasted document, and a short request that answers the question. | The request ranks before the sentence; the sentence record shows at most 400 characters around it. |
| TS-17-15 | A request, two short next messages and a long message in one session. | The request has the two short messages as `follow_up`; the long message is not a follow-up. |

### TS-18 — Installation, settings and verification (ADR-014)

File: `tests/integration/install.rs`. Each test has its own home, `HOME` and port; autostart is off. TS-18-06 needs `node`; without it the test prints a reason and returns.

| ID | Test | Expected result |
|----|------|-----------------|
| TS-18-01 | `zcie setup --yes` with options (project folder, port, vector off, autostart off). | Exit 0; all verify checks ✓, including "a NEW session gets the fact of an earlier session"; `setup.toml`, `engine.toml` and the Claude Code settings have the values. |
| TS-18-02 | `zcie config set budget 900 workers 1`; then wrong values (budget 99999, vector fast, unknown key); `config show`. | The new values are applied to the Claude Code settings and `engine.toml`; each wrong value fails and changes nothing. |
| TS-18-03 | The wizard with piped answers (language tr, profile lean, the rest kept); a second run that declines the summary. | The answered values change and the others stay; "Nothing was changed" after the decline. |
| TS-18-04 | `zcie verify` before and after `zcie daemon stop`. | Exit 0, then a failure with "✗  daemon does not answer". |
| TS-18-05 | `zcie uninstall --purge`. | The home folder is deleted; the Claude Code settings have no ZCIE entry; `verify` fails. |
| TS-18-06 | The npm launcher without and with the native programs; a command before setup. | "has no build for" with exit 1; the version of `zcie`; the hint "Run: zcie setup". |
| TS-18-07 | `zcie project add` of a small folder; `zcie project status` of the folder and of a folder with no project. | The add waits until "index complete"; 3 files and 1 symbol; the query finds the symbol; the hint "zcie project add" for the other folder. |
| TS-18-08 | `zcie project add` of a repository that holds an ignored inner repository and an ignored `data/`; then with `--nested`. | Both folders are reported as skipped; with `--nested` the inner repository gets its own project ID and its symbol. |
| TS-18-09 | SessionStart hook in an inner repository with no ID file, inside an added project. | The inner repository gets its own project, not the outer one. |
| TS-18-10 | Setup defaults, then the Stop hook `assistant-message.sh` with `last_assistant_message`. | `assistant_messages = true` in `engine.toml`; the Stop hook is set; a query finds the answer. |

### TS-19 — Only added folders are indexed (ADR-015)

File: `tests/integration/indexing_scope.rs`.

| ID | Test | Expected result |
|----|------|-----------------|
| TS-19-01 | A folder registered for the memory only; then `index = true`; then a registration for the memory again. | 0 files and "not indexed" first; files and symbols after the add; the folder stays indexed. |
| TS-19-02 | New files in an added folder: one under a `.gitignore` folder (`build/`), one in `src/`. | Only `src/` file is indexed. |
| TS-19-03 | A pending file job of a memory-only folder, then a restart. | The job is deleted at startup; 0 files. |
| TS-19-04 | "Bu proje hakkında bana bilgi ver" in a folder with a README, `compose.yml`, `src/` and an inner project. | The answer lists the top level and the inner project; the README start is a file result. |
| TS-19-05 | An assistant answer captured with `capture.assistant_messages = true`. | A later question finds the answer. |
| TS-19-06 | Blob sweep with age 0 after a folder is indexed. | The content of the indexed file stays. |
| TS-19-07 | The content blob of an indexed file is deleted; the folder is scanned again. | The blob is stored again. |

### TS-20 — Context efficiency (ADR-017)

Write these tests after D-055 to D-065 are done. Files: `tests/integration/context_efficiency.rs` (engine), `tests/integration/context_hooks.rs` (hook), the ledger fixture `ledger_repo` in `tests/support/fixture.rs`, unit tests in `crates/zcie-query/src/{capsule_rules,compress,expand_batch}.rs`, the benchmark scripts of D-055.

| ID | Test | Expected result |
|----|------|-----------------|
| TS-20-01 | Two results with the same full text; a follow-up text equal to an item title. | One item; the follow-up text is not shown twice. |
| TS-20-02 | The 15 prompts of `benchmarks/user-ideas/generic.json` (it has "bunların planını oluştur"), with 3 project facts in memory. | `capsule = false` for each; the hook prints nothing. A short fact question ("Kod incelemesini kim yapıyor?") keeps its capsule. |
| TS-20-03 | A response with 5 items. | The best item is first and the second best is last. |
| TS-20-04 | A fixture file that calls a function of another file; then the call is removed. | The file references (`file_refs`, D-057) have the name; after the change the name is deleted. |
| TS-20-05 | `--mode map` with a budget of 300 tokens, with a general question and with a question about one file (the seed). | The map has 300 tokens or fewer; the seeded file ranks higher (first) with the seed. |
| TS-20-06 | Symbol text with context (path, container, file comment). | The indexed body has the context; the shown title and snippet do not change. |
| TS-20-07 | "#89 hangi dosyayı değiştirdi ve hangi test geçti?" on the fixture. | Task, commit, file and test event in the first 6 results. |
| TS-20-08 | Compression of a 2,000-character snippet with 2 matching sentences. | Both sentences are kept, in order; "…" marks the gaps; 450 characters or fewer (the limit after D-065). |
| TS-20-09 | `--mode locate` "add a tax to the late fee" on the ledger fixture. | The fee file is the first file; each line has `path:start-end`; 300 tokens or fewer. |
| TS-20-10 | `zcie_expand` with 3 ranges and one range of 250 lines; a symbol ref with `with: ["callers", "tests"]`. | 3 sources in the budget; the long range is cut at 100 lines with the "more" range; the caller line and the test line are given. |
| TS-20-11 | Each `[context]` setting set to false. | The behavior of the task is off (no `capsule = false`, no map, no graph score, one window instead of compression); the other tests of the suite still pass. |
| TS-20-12 | Benchmarks of D-065 against the baseline of D-055. | The numbers meet AC-32 to AC-36. |

### TS-21 — Evidence-first context (ADR-019)

Write these tests after the tasks of a P5 release are done. Files: `tests/integration/evidence.rs` (new), `tests/integration/context_hooks.rs`, unit tests in `crates/zcie-query/src/{intent,evidence,trace}.rs`, `benchmarks/isolated/`.

| ID | Test | Expected result |
|----|------|-----------------|
| TS-21-01 | A query with `explain: true` and the same query without it. | The trace lists each candidate with its source and reason, and the stage durations; the response without `explain` has no `trace` field. |
| TS-21-02 | Two isolated benchmark runs of 2 tasks with the same seed. | The manifests of the clones are equal; the frozen home is unchanged; the condition order is the same for the same seed. |
| TS-21-03 | The prompts of `code.json` and `locate.json`. | Each prompt gets the intent of its task class. |
| TS-21-04 | An edit prompt on the ledger fixture. | `evidence` has the slice of the function to change, with correct lines and hash, and `requires_live_read: false`. |
| TS-21-05 | The same prompt after the file changed on disk (not indexed yet). | `requires_live_read: true`; the hook prints "changed". |
| TS-21-06 | An explanation prompt about two connected functions. | `intent = explain`; the evidence has the bodies of both functions; no text is compressed. |
| TS-21-07 | The same edit prompt twice in one hook session; then a change of the file and the prompt again. | The slices are printed once; after the change they are printed again with "changed". |
| TS-21-08 | "bunu düzelt" after an edit prompt in the same session. | The capsule has the slices of the working set only if their files changed; else no capsule. |
| TS-21-09 | Two functions with the same name in two modules; a reference through an import. | Two resolution keys; the reference resolves to one; a bare name stays `candidate` (P5.3). |
| TS-21-10 | An assistant answer that contradicts a user requirement. | The user requirement stays active (P5.4). |
| TS-21-11 | The hook key on admin routes; range expansion with `../` and absolute paths outside the project. | 403 for each admin route; `not_found` for each path outside the project (P5.4). |
| TS-21-12 | Forget a record, restart, then lexical, vector, graph and expand queries (P5.4). | No result has the record. |
| TS-21-13 | The Stop hook after an edit that leaves a Rust syntax error; after a fix; a turn with no edit. | Block with the compiler error; then no block; no block with no edit (ADR-020). |
| TS-21-14 | The adversarial fact fixture: a wrong assistant statement, a superseded value, a changed file. | No wrong value is shown as a fact; each is labeled (ADR-020). |
| TS-21-15 | A file changed on disk with no event (editor), then the same question. | The answer has the new content, not a cached one (D-078). |

### TS-22 — Token savings report and menu bar app (ADR-022)

Files: `apps/zcie/src/commands/usage/tests.rs` (TS-22-01 to 03, 07 to 10), `tests/integration/usage.rs` (TS-22-04, 05), `apps/zcie-bar/test.sh` (TS-22-06).

| ID | Test | Expected result |
|----|------|-----------------|
| TS-22-01 | A fixture transcript: typed messages, notifications, meta records, tool results, an assistant message in 3 records, a ZCIE capsule, a subagent file, a session of `sdk-cli`. | 2 turns; each message counted once; Read 1, searches 3 (grep, Grep, Glob); 1 capsule; the subagent adds to its session; the `sdk-cli` session is automated. |
| TS-22-02 | Scan, append lines (one written in two parts), scan again; then make the file shorter. | The incremental result equals a full scan; a half line is read once when complete; a shorter file is read again from the start. |
| TS-22-03 | The report of 3 + 3 sessions of a project, a project with 1 + 1 sessions, automated, old, empty and temporary sessions, two folders with the same name. | Medians 200 and 500, saving 60 %, 1,800 tokens; 1 + 1 sessions give 800 tokens with no minimum; excluded sessions are not counted; names `x/repo` and `y/repo`. |
| TS-22-04 | `zcie usage --json` and the table on fixture transcripts and a hook statistics file. | Saving 75 %, 6,750 tokens, no outlier, confidence medium; gate pass 1; the cache file is written; the table shows "-75%" and the reason of the other project. |
| TS-22-05 | The read guard on a 900-line file, an image and a binary file; the gate after a Python syntax error and after the fix; a hook that runs from the repository. | Lines `read_guard` (900, 250), `gate block`, `gate pass`; no line for the image and the binary file; no file content; no statistics file in the repository. |
| TS-22-06 | The decode check of the app on the output of `zcie usage --json`. | The report decodes; the first project and its saving are printed. |
| TS-22-07 | Root records with `--no-disk`: a recorded root, a root from the SessionStart hook, a linked worktree. | The recorded roots are used; the worktree goes to its repository; a guess is not recorded. |
| TS-22-08 | Two scans within one minute after an append. | The second result has the new lines; the cache file is not written again. |
| TS-22-09 | A project with 1 session with ZCIE and none with no ZCIE; 3 sessions with no ZCIE in other projects; then none. | Basis `all projects`, saving 50 % against their median (200), note "rough…", `rough_saved_tokens` 200 and `saved_tokens` 0; with none: no value. |
| TS-22-10 | A session that ZCIE joined later: a turn and a subagent before the first ZCIE output, the turn of the first capsule, a later subagent. | The part before (1 turn, its tokens and the early subagent) counts as a session with no ZCIE; the turn of the capsule and the rest count as ZCIE. |

### TS-23 — Activity log and index view (ADR-023)

| ID | Test | Expected result |
|----|------|-----------------|
| TS-23-01 | A capture and a query on a test engine (`tests/integration/usage.rs`). | The activity log has `capture`, `job` and `query` lines with a time and ms; the query line has the question, results and tokens. |
| TS-23-02 | A fixture log of queries, an expand, a capture and jobs, read in two parts (`usage/index.rs`). | Totals for each project are exact; p50 5 ms and p95 100 ms from the buckets; a failed job counts; the newest operation is first; a third read reads nothing. |

### TS-24 — Honest savings and subagent context (ADR-024)

| ID | Test | Expected result |
|----|------|-----------------|
| TS-24-01 | A project with a large ZCIE session (6 subagents), a small one, 3 sessions with no ZCIE, and an automated session (`usage/tests.rs`). | The large session is an outlier (cost per turn ≥ 3 × median), listed with subagents 6 and share 93 %, and left out; saving against the median 300; confidence low; automated 1 session. |
| TS-24-02 | The subagent hook on an Agent call with a task that names a symbol of an indexed fixture; on a Read call; with the daemon stopped (`tests/integration/context_hooks.rs`). | The task gets the ZCIE capsule appended (`updatedInput.prompt`); other tools and a stopped daemon give no output. |

### TS-25 — Cursor adapter (ADR-025)

Files: `apps/zcie/src/commands/cursor.rs` (TS-25-01, 02), `tests/integration/context_hooks.rs` (TS-25-03, 04), `tests/integration/install.rs` (TS-25-05, 07, 08), `tests/integration/gateways.rs` (TS-25-06).

| ID | Test | Expected result |
|----|------|-----------------|
| TS-25-01 | `add_hooks` on a config with a foreign `beforeSubmitPrompt` entry and `version` 1; then `add_hooks` again; then `remove_hooks`. | Each of the 5 events has one ZCIE entry after two runs; the foreign entry stays; after the removal only the foreign entry is left. |
| TS-25-02 | `mcp_entry` for a project folder. | The command ends with `zcie-mcp`; `args` is empty; `env` has `ZCIE_URL`, `ZCIE_API_KEY_FILE` and `ZCIE_PROJECT_DIR`. |
| TS-25-03 | `cursor-hook.sh` on a `beforeSubmitPrompt` input with `cursor_version` and a project fact prompt, then a generic prompt, on a test daemon with a fixture project. | The project prompt gives JSON with `continue` true and `additional_context` that contains "ZCIE context"; the generic prompt gives `{"continue":true}` only. |
| TS-25-04 | `inject-context.sh` with `cursor_version` and no `ZCIE_APP`; then the same input without `cursor_version`. | The first run prints nothing; the second run prints the capsule. |
| TS-25-05 | `sessionStart` in a Git repository with no `.claude/zcie_id`; then `afterFileEdit` and `afterAgentResponse` in the same session. | The id file is written; `additional_context` starts with "ZCIE: this project"; the daemon holds the file change and the assistant message with app `cursor`. |
| TS-25-06 | `zcie-mcp` started in `/` with `ZCIE_PROJECT_DIR` set to a project with `.claude/zcie_id`, then `zcie_query`. | The query returns only the data of that project. |
| TS-25-07 | `zcie cursor setup --dir P` twice; `zcie cursor remove --dir P`; `zcie setup -y --cursor true`; `zcie verify`; `zcie uninstall`, with a test `HOME`. | Project setup writes 5 hook events and `mcpServers.zcie`, with no duplicates and a foreign server kept; remove deletes only ZCIE entries; global setup writes `HOME/.cursor`; verify prints "Cursor hooks installed"; uninstall removes the entries. |
| TS-25-08 | `sessionStart`, then `sessionEnd` twice with a `transcript_path` of one turn (timestamp + user_query, a thought, a final answer); a second session with `beforeSubmitPrompt`, then `sessionEnd` with another transcript. | The daemon holds the user.message and the assistant.message (the last text, not the thought) with app `cursor`; the second transcript is not stored. |

### TS-26 — Codex CLI adapter (ADR-026)

Files: `apps/zcie/src/commands/codex.rs` (TS-26-01), `tests/integration/context_hooks.rs` (TS-26-02).

| ID | Input | Expected result |
|---|---|---|
| TS-26-01 | `remove_hooks` and `add_hooks` twice on a config with a foreign `Stop` group; then `remove_hooks`. | Each ZCIE script has one group; the foreign group stays; after the removal only the foreign group is left. |
| TS-26-02 | `codex-hook.sh inject-context.sh` on a `UserPromptSubmit` input with a project fact prompt, then a generic prompt, on a test server. | The project prompt gives `hookSpecificOutput` with `hookEventName` `UserPromptSubmit` and `additionalContext` that contains "ZCIE context"; the generic prompt gives no output. |

### TS-27 — Windows 11 support (ADR-027)

Files: `apps/zcie/src/commands/service.rs` (TS-27-01), `.github/workflows/ci.yml` (TS-27-02).

| ID | Input | Expected result |
|---|---|---|
| TS-27-01 | `startup_script` for a `zcied.exe` and a home in a folder with a space. | The VBScript quotes both paths and starts the program with window style 0. |
| TS-27-02 | CI on `windows-latest`: clippy, unit tests, `build-npm.sh`, `npm install -g`, `zcie setup -y`, `zcie project add`, `zcie query`, `zcie verify`, `zcie daemon stop`. | Every step passes. The query finds the symbol. |

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

### 7.1 Multi-project benchmark (ADR-009)

Run `benchmarks/samples/generate.py` and `benchmarks/samples/prepare.py`, then `benchmarks/claude-ab/run_ab.py` with `AB_CONDITIONS=A,Z` (see `benchmarks/samples/README.md`). The report is `benchmarks/results/three-projects-2026-10-16.md`. Expected result: Z is correct in all runs and costs less than A in each task group.

## 8. Test results record

Fill this table during the test phase.

| Suite | Tests | Passed | Failed | Ignored (reason) | Date |
|-------|-------|--------|--------|------------------|------|
| TS-01 | 10 | 10 | 0 | 0 | 2026-10-16 |
| TS-02 | 8 | 8 | 0 | 0 | 2026-10-16 |
| TS-03 | 7 | 7 | 0 | 0 | 2026-10-16 |
| TS-04 | 9 | 9 | 0 | 0 | 2026-10-16 |
| TS-05 | 12 | 12 | 0 | 0 | 2026-10-16 |
| TS-06 | 15 | 15 | 0 | 0 | 2026-10-16 (incl. regression I-32) |
| TS-07 | 18 | 18 | 0 | — | 2026-10-16 |
| TS-08 | 4 | 4 | 0 | — | 2026-10-16 |
| TS-09 | 6 | 6 | 0 | 0 | 2026-10-16 |
| TS-10 | 4 | 4 | 0 | — | 2026-10-16 |
| TS-11 | 8 | 8 | 0 | 0 | 2026-10-16 |
| TS-12 | 1 (6 checks) | 1 | 0 | 0 | 2026-10-16 |
| TS-14 | 16 | 16 | 0 | 0 | 2026-10-16 |
| TS-15 | 3 (11 client tests) | 3 | 0 | 0 | 2026-10-16 |
| TS-16 | 6 | 6 | 0 | 0 (model present) | 2026-10-16 |
| TS-17 | 15 | 15 | 0 | 0 (model present) | 2026-10-17 |
| TS-18 | 10 | 10 | 0 | 0 | 2026-10-17 |
| TS-19 | 7 | 7 | 0 | 0 | 2026-10-17 |
| TS-20 | 12 | 11 | 1 | 0 | 2026-10-17 (TS-20-12: AC-32 and AC-33 are not met) |
| TS-21 | 15 | 15 | 0 | 0 | 2026-10-17 (all releases P5.1 to P5.6; ADR-020) |
| TS-22 | 10 | 10 | 0 | 0 | 2026-10-19 (ADR-022) |
| TS-23 | 2 | 2 | 0 | 0 | 2026-10-19 (ADR-023) |
| TS-24 | 2 | 2 | 0 | 0 | 2026-10-19 (ADR-024) |
| TK | 2 (1 test fn) | 2 | 0 | — | 2026-10-16 |

## 9. Exit conditions

The test phase is complete when all these conditions are true:

1. All suites in section 5 and section 6 pass.
2. No test is ignored without a reason in section 8.
3. `cargo test --workspace` passes two times in a row.
4. The benchmark results are recorded (section 7).

Then go to `docs/14-acceptance-criteria.md`.
