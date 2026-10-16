# 08 — Memory and Graph

Status: Normative. Crates: `zcie-memory`, `zcie-graph`.

## 1. Two kinds of memory

| Kind | Content | Store | Can change? |
|------|---------|-------|-------------|
| Raw memory | Events and their original payloads. | CF `events` + object store. | No. Only `forget` removes it. |
| Structured memory | Memory records, tasks and edges derived from raw memory. | CF `entities`, `edges_out`, `edges_in`. | Yes. Each change increments `generation`. |

Rules:

- Raw memory is the source of truth. Structured memory MUST NOT replace raw memory.
- Each memory record MUST have one or more `source_event_ids`.
- Each edge MUST have an `evidence` with one or more `source_event_ids`, or a `revision`.
- The engine MUST be able to make all structured memory again from raw memory and the repository files.

## 2. Ref format

A ref has the form `<kind>:<id>`. Use these kinds only:

| Ref kind | Entity | Example |
|----------|--------|---------|
| `ev` | Event | `ev:ev_01JAB3...` |
| `file` | File | `file:file_01JAB3...` |
| `sym` | Symbol | `sym:sym_01JAB3...` |
| `mem` | Memory record | `mem:mem_01JAB3...` |
| `task` | Task | `task:task_01JAB3...` |
| `commit` | Commit | `commit:commit_<sha>` |

Put the ref parser and formatter in `zcie-types` (`Ref::parse`, `Ref::to_string`).

## 3. Tier A extraction (deterministic, mandatory)

The `ExtractMemory` job handler does Tier A extraction. It uses `regex` and lookups only. It uses no language model.

### 3.1 Patterns

Compile each regex one time (for example in a `std::sync::OnceLock`).

| ID | Finds | Regex (Rust syntax) | Result |
|----|-------|---------------------|--------|
| X-01 | File path | `(?:[\w.-]+/)+[\w.-]+\.\w{1,8}` | Look up the path in the worktree. If found: `TextMention` edge to the file. |
| X-02 | Symbol-like name | `\b[A-Za-z_][A-Za-z0-9_]*(?:(?:::|\.)[A-Za-z_][A-Za-z0-9_]*)*\b` with a length of 4 or more and a `_`, `::`, `.`, or an inner capital letter | Look up the name in CF `lookup`, prefix `t / "sym_name" / <workspace_id> / <lowercase name> /`. If exactly 1 to 5 symbols match: `TextMention` edge to each symbol. If more than 5 match: no edge. |
| X-03 | External task ref | `(?:^|\s)#(\d{1,7})\b` or `\b([A-Z][A-Z0-9]{1,9}-\d{1,7})\b` | Task ref. See section 4. |
| X-04 | Internal task ref | `\btask:task_[0-9A-HJKMNP-TV-Z]{26}\b` | Task ref. See section 4. |
| X-05 | Git SHA | `\b[0-9a-f]{7,40}\b` | If a stored commit starts with the value: `TextMention` edge to the commit. |
| X-06 | Error signature | `(?m)^.*(?:error\[E\d{4}\]|panicked at|Traceback \(most recent call last\)|\b\w+(?:Error|Exception)\b:).*$` | One `Observation` memory record per distinct line (5 lines or fewer). |
| X-07 | Decision marker | `(?i)\b(?:we decided|decision:|we will use|karar:|karar verdik)\b` | One `DesignDecision` memory record with the sentence that contains the marker. Status `Active`, confidence `0.6`. |
| X-08 | Preference marker | `(?i)\b(?:always|never|prefer|her zaman|asla|tercih)\b` at the start of a sentence | One `Preference` memory record. Confidence `0.5`. |

Name lookups in X-02 use the scope of the event. Do not match names in other workspaces.

### 3.2 Rules by event type

| Event type | Memory records | Other actions |
|------------|----------------|---------------|
| `user.message` | One `UserRequirement` (text = first 500 characters, confidence `1.0`, status `Active`). Plus X-07 and X-08 results. | X-01 to X-05 edges from the requirement record. Section 4 task rule. |
| `assistant.message` | X-07 results only. | X-01, X-02 edges. |
| `test.result` | One `Observation` (text = test name and status). | Section 5.4. |
| `command.run` | X-06 results only, if exit code is not 0. | None. |
| `tool.result` | X-06 results only. | None. |
| `file.changed`, `file.deleted` | None. | Make `IndexFile` or `DeleteFile` job. Section 5.3. |
| `git.commit` | None. | Make `IndexCommit` job. |
| `user.confirmation` | None. | Section 5.2. |
| `session.start`, `session.end` | None. | None. |

Edges from a memory record to its source event: `mem:<id>` -`DerivedFrom`-> `ev:<id>`, status `Verified`, evidence `Derivation`.

## 4. Task rule

Keep task creation simple. A task is created only from an explicit task ref.

1. If the event text has an X-04 match, use that task. If the task does not exist, ignore the ref.
2. If the event text has an X-03 match, look up CF `lookup`, key `t / "task_ext" / <workspace_id> / <value>`.
   - If found, use that task.
   - If not found and the event is a `user.message`, make a new `TaskRecord`. Set `title` = first 120 characters of the message. Set `external_ref` = the value. Set `state = Open`. Store the lookup entry.
3. For each task found or made, add edge `task:<id>` -`RequestedBy`-> `mem:<requirement id>`. Status `Verified`, evidence `Derivation` (the user wrote the ref).
4. A user can make a task without a ref through the API (`POST /v1/tasks`). See `docs/10-api-and-gateways.md`.

A message without a task ref makes a `UserRequirement` memory record only. It does not make a task.

## 5. Linking and verification

### 5.1 Edge creation function

All edges are made by one function: `zcie_graph::put_edge(batch, edge) -> EdgeId`.

1. Lower `edge.status` to the maximum for `edge.evidence.kind` (`docs/05-data-model.md` section 7.1).
2. If an edge with the same `(from, relation, to)` exists, keep the edge with the higher status. Merge the `source_event_ids`. Increment `generation`.
3. Write the edge in `edges_out` with key `<tenant> / <from> / <relation> / <to>`.
4. Write the reverse entry in `edges_in` with key `<tenant> / <to> / <relation> / <from>`.
5. Write both in the same `WriteBatch`.

### 5.2 Verification sources

Only these four sources can make a `Verified` link between a request, a task and code changes.

| Source | How it works | Edge |
|--------|--------------|------|
| `CommitTrailer` | The commit message has `ZCIE-Task: <ref>`. See `docs/07-ingest-and-indexing.md` section 7.1. | `task` -`Implements`-> `commit` |
| `UserConfirmation` | A `user.confirmation` event with payload `{ "from": ref, "relation": "IMPLEMENTS", "to": ref }`. Or the API `POST /v1/links` with `"evidence": "user_confirmation"`. | As given |
| `ClientTrace` | A `file.changed` event has the same `source.session_id` and `source.turn_id` as a `user.message` event. The client states that the change came from that turn. | `mem:<requirement>` -`AffectsFile`-> `file`; and `task` -`AffectsFile`-> `file` for each task of that requirement |
| `TestOutcome` | A `test.result` payload has `"refs": ["task:...", "sym:..."]`. | `<ref>` -`TestedBy`-> `ev:<test event>`; if status `failed`, also `<ref>` -`FailedIn`-> `ev:<test event>` |

> **Warning:** Do not make a `Verified` edge from time proximity. A file that changed near the time of a message is NOT an implementation of the message.

### 5.3 Client trace for file changes

When a `file.changed` event has a `source.turn_id`:

1. Find the `user.message` events with the same `session_id` and `turn_id`. Use the lookup CF `lookup`, key prefix `t / "turn" / <session_id> / <turn_id> /`.
2. For each such event, find its `UserRequirement` record.
3. Add the edges in the `ClientTrace` row of the table in section 5.2.

### 5.4 Task state from tests

- If a `test.result` with status `passed` has a task ref, set the task state to `Resolved`.
- If a `test.result` with status `failed` has a task ref, set the task state to `Failed`.
- Store the old state in the task history list. Increment `generation`.

### 5.5 Implementation path

To answer "which code implements the request", follow this path:

```
mem(UserRequirement) <-RequestedBy- task -Implements-> commit <-ChangedInCommit- file -DefinesSymbol-> sym
```

- The path status is the lowest status of all edges in the path.
- If the path status is `Verified`, the answer MAY say "implemented by".
- Otherwise the answer MUST say "candidate" or "possibly related". It MUST list the evidence kind.

## 6. Corrections, contradictions and history

Never delete or overwrite a memory record to correct it.

### 6.1 Correction (`correct_memory`)

1. Make a new memory record. Set `kind` = old kind. Set `text` = corrected text. Set `source_event_ids` = the correction event.
2. Set the old record `status = Superseded`. Set `superseded_by` = new ID. Increment `generation`.
3. Add edge `mem:<new>` -`Supersedes`-> `mem:<old>`. Status `Verified`, evidence `UserConfirmation`.
4. Write all in one `WriteBatch`. Make `UpdateSearch` jobs for both records.

### 6.2 Contradiction

- Milestone 1 does not detect contradictions automatically.
- The API `POST /v1/links` with relation `CONTRADICTS` sets both records `status = Disputed`.
- A query MUST show a `Disputed` record with its status. It MUST NOT show it as a fact.

### 6.3 Retraction

- `forget` with a memory selector sets `status = Retracted` and removes the text. See `docs/11-security.md`.

## 7. Timeline

`get_timeline(ref) -> Vec<TimelineItem>` does these steps:

1. Collect the start entity and its neighbors at depth 1 and 2 (section 8). Use `query.graph_max_fanout`.
2. Collect all `source_event_ids` of these entities and of the edges between them.
3. Read the event envelopes. Sort them by `seq`.
4. Return at most 200 items. Each item has: `seq`, `event_id`, `event_type`, `timestamp_utc`, `summary` (first 160 characters of the payload), `refs` (the entities linked to the event).

## 8. Graph traversal

`zcie_graph::neighbors(scope, start_ref, relations, direction, max_depth) -> Vec<PathItem>`

- Use a breadth-first search.
- `max_depth` MUST be `query.graph_max_depth` or less.
- For each node, read at most `query.graph_max_fanout` edges. Use a RocksDB prefix iterator on `edges_out` or `edges_in`.
- Skip edges with status `Rejected`.
- Skip a node that is outside the request scope (`docs/11-security.md`).
- Keep a visited set. Do not visit a node two times.
- `PathItem` has: `ref`, `depth`, `via_relation`, `path_status` (lowest status on the path), `edge_ids`.
- Use one RocksDB snapshot for the full traversal.

## 9. Worked example: weapon attachment

Workspace `atheron`. Repository `repo_A`. Worktree `wt_main`.

**Step 1.** The user writes in Claude Code: "Weapon attachment moves during animation #89. Fix attachToBone in src/animation/weapon-socket.ts."
The hook sends `user.message`, `session_id = s-18`, `turn_id = u-421`. The daemon stores `ev_E1` (seq 1001).

**Step 2.** `ExtractMemory` for `ev_E1` makes:

| Record | Content |
|--------|---------|
| `mem_M1` | `UserRequirement`, text = the message, source `[ev_E1]` |
| `task_T89` | `TaskRecord`, title from message, `external_ref = "#89"`, `state = Open` |

| Edge | Status | Evidence |
|------|--------|----------|
| `mem:mem_M1` -`DerivedFrom`-> `ev:ev_E1` | Verified | Derivation |
| `task:task_T89` -`RequestedBy`-> `mem:mem_M1` | Verified | Derivation |
| `mem:mem_M1` -`References`-> `file:file_WS` (X-01 path) | Inferred | TextMention |
| `mem:mem_M1` -`References`-> `sym:sym_ATB` (X-02 `attachToBone`) | Inferred | TextMention |

**Step 3.** The agent edits `weapon-socket.ts`. The hook sends `file.changed` with `turn_id = u-421` (`ev_E2`). The handler adds:

| Edge | Status | Evidence |
|------|--------|----------|
| `mem:mem_M1` -`AffectsFile`-> `file:file_WS` | Verified | ClientTrace |
| `task:task_T89` -`AffectsFile`-> `file:file_WS` | Verified | ClientTrace |

The `IndexFile` job parses the file again. The symbol `WeaponSocket.attachToBone` keeps ID `sym_ATB`.

**Step 4.** The test hook sends `test.result` (`ev_E3`) with `{"name":"weapon-socket.integration","status":"passed","refs":["task:task_T89"]}`.
Edge `task:task_T89` -`TestedBy`-> `ev:ev_E3` (Verified, TestOutcome). Task state becomes `Resolved`.

**Step 5.** The user commits with the message line `ZCIE-Task: #89`. The Git hook sends `git.commit` (`ev_E4`). `IndexCommit` adds:

| Edge | Status | Evidence |
|------|--------|----------|
| `file:file_WS` -`ChangedInCommit`-> `commit:commit_abc123` | Verified | GitHistory |
| `task:task_T89` -`Implements`-> `commit:commit_abc123` | Verified | CommitTrailer |

**Step 6.** Later, in OpenCode, the user asks: "What did we do for the weapon animation before?"
The query engine finds `mem_M1` and `task_T89` through Tantivy (`conversations` index). It follows the path in section 5.5. All edges are `Verified`. The capsule returns: task `#89` (Resolved), commit `abc123`, file `weapon-socket.ts`, symbol `WeaponSocket.attachToBone` with span, test `ev_E3` passed, source message `ev_E1`.
