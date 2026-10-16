# 09 — Query and Context

Status: Normative. Crates: `zcie-query`, `zcie-context`, `zcie-cache`, `zcie-search`.

> **Note:** Implementation decisions of milestone 1 are in `docs/adr/ADR-008-implementation-decisions.md`. Where they differ from this file, ADR-008 applies.

## 1. Overview

The read path finds the smallest set of cited evidence that answers a question. It uses no language model.

```
QueryRequest
  -> authenticate, resolve scope
  -> normalize + classify
  -> L0 cache            -- hit  -> freshness check -> return
  -> exact lookup        -- hit  -> step "graph" (if needed) -> compile
  -> Tantivy search
  -> graph expansion + rerank
  -> (OPTIONAL) vector search
  -> freshness check (watermarks + live overlay)
  -> token firewall -> QueryResponse
```

## 2. Request and response types

```rust
pub struct QueryRequest {
    pub scope: Scope,
    pub question: String,                 // 2000 characters or fewer
    pub mode: QueryMode,                  // default Auto
    pub token_budget: Option<u32>,        // default query.default_token_budget
    pub max_results: Option<u32>,         // default query.max_results
    pub cursor: Option<String>,           // continuation from an earlier response
}

pub enum QueryMode { Auto, Exact, Search, Timeline, Memory }

pub struct QueryResponse {
    pub query_id: String,                 // "q_<ulid>"
    pub answer_kind: AnswerKind,
    pub answer: Option<String>,           // only for AnswerKind::Local
    pub results: Vec<ResultItem>,
    pub memory_links: Vec<String>,        // refs
    pub freshness: Freshness,
    pub watermarks: Watermarks,
    pub token_budget: u32,
    pub token_estimate: u32,
    pub truncated: bool,
    pub cursor: Option<String>,
}

pub enum AnswerKind { Local, Evidence, NotFound }

pub struct ResultItem {
    pub r#ref: String,                    // serialized as "ref"
    pub kind: String,                     // "symbol", "file", "memory", "task", "commit", "event"
    pub title: String,                    // symbol qualified name, file path, task title, memory text (160 chars)
    pub path: Option<String>,
    pub range: Option<[u32; 2]>,
    pub status: Option<String>,           // edge path status or memory status
    pub evidence_status: Option<String>,  // "verified_ast", "verified_commit", "inferred_text", "candidate"
    pub revision: Option<String>,
    pub score: f32,
    pub snippet: Option<String>,          // only at level L1 and higher
}

pub enum Freshness { Current, Stale, Partial }

pub struct Watermarks { pub canonical_seq: u64, pub graph_seq: u64, pub search_seq: u64 }
```

Serialize all enums in `snake_case`.

### 2.1 Example response

```json
{
  "query_id": "q_01JAB3...",
  "answer_kind": "evidence",
  "answer": null,
  "results": [
    {
      "ref": "sym:sym_01JAB3...",
      "kind": "symbol",
      "title": "WeaponSocket.attachToBone",
      "path": "src/animation/weapon-socket.ts",
      "range": [81, 119],
      "status": "verified",
      "evidence_status": "verified_ast",
      "revision": "abc123",
      "score": 0.92,
      "snippet": null
    }
  ],
  "memory_links": ["task:task_01JAB3...", "mem:mem_01JAB3..."],
  "freshness": "current",
  "watermarks": { "canonical_seq": 1004, "graph_seq": 1004, "search_seq": 1003 },
  "token_budget": 250,
  "token_estimate": 142,
  "truncated": false,
  "cursor": null
}
```

## 3. Normalization and classification

### 3.1 Normalization

1. Trim the question. Change it to lower case for classification only. Keep the original for search.
2. Replace a sequence of white space with one space.
3. The normalized question is part of the cache key.

### 3.2 Classifier (rules, no language model)

If `mode` is not `Auto`, use the given mode. Otherwise apply these rules in order. Use the first rule that matches.

| Order | Condition | Class | Route |
|-------|-----------|-------|-------|
| 1 | The question is a ref (`^(ev|file|sym|mem|task|commit):\S+$`) | `ExactRef` | Entity lookup. |
| 2 | Contains `where is .* defined`, `definition of`, `nerede tanımlı` | `SymbolDefinition` | Symbol lookup by name. |
| 3 | Contains `who calls`, `callers of`, `kim çağırıyor` | `Callers` | Reverse graph (`Calls`, OPTIONAL in milestone 1; return `not_found` with reason if no `Calls` edges exist). |
| 4 | Contains `what did i ask`, `what changed since`, `history of`, `timeline`, `daha önce`, `ne yapmıştık` | `Timeline` | Memory + timeline. |
| 5 | Contains `which commit`, `implemented`, `hangi commit` | `Implementation` | Implementation path (`docs/08-memory-and-graph.md` section 5.5). |
| 6 | Contains a task ref (X-03, X-04) | `Task` | Task lookup + neighbors. |
| 7 | Contains a single token that matches a symbol name exactly | `SymbolDefinition` | Symbol lookup. |
| 8 | Any other question | `Search` | Tantivy + graph rerank. |

Put the rule keywords in one constant list in `zcie-query/src/classify.rs`.

## 4. Routes

### 4.1 L0 cache

- See section 8. On a hit, do the freshness check (section 7). If the result is still `current`, return it.

### 4.2 Exact lookup

| Class | Lookup |
|-------|--------|
| `ExactRef` | Read the entity from CF `entities` (or `events`). |
| `SymbolDefinition` | Read CF `lookup`, prefix `t / "sym_name" / <workspace_id> / <lowercase name> /` → list of symbol IDs. Also try the `qualified_name`. |
| `Task` | Read CF `lookup`, key `t / "task_ext" / <workspace_id> / <external_ref>`, or read the task ID directly. |

If the exact lookup finds one to `max_results` items, skip Tantivy.

### 4.3 Lexical search (Tantivy)

1. Build a `BooleanQuery`:
   - `Must`: a `TermQuery` for `tenant_id` and for `workspace_id`.
   - `Must`: a `TermQuery` for `repository_id` if the scope has one.
   - `Should`: the parsed question on fields `name` (boost 3.0), `path` (boost 2.0) and `body` (boost 1.0). Use `QueryParser` with lenient parsing.
2. Search the indexes in this order by class: `Timeline` → `conversations` then `code`; other classes → `code`, `documents`, `conversations`.
3. Take the top `max_results * 3` hits from all indexes.

### 4.4 Graph expansion and rerank

1. For the top hits, read neighbors at depth 1 (`zcie_graph::neighbors`, `max_depth = 1`).
2. Add a neighbor as a new result if it is a `task`, `mem` or `commit` and not in the list.
3. Compute the final score (section 5). Sort by score. Keep `max_results`.

### 4.5 Vector search (OPTIONAL)

Not in milestone 1. The `zcie-vector` crate defines the trait `VectorIndex` only. The query planner calls it only if configuration enables it.

## 5. Ranking

Use one simple formula. Keep the weights in `limits.rs`.

```
score = 0.6 * lexical_norm + 0.2 * status_weight + 0.1 * recency + 0.1 * kind_weight
```

| Part | Value |
|------|-------|
| `lexical_norm` | Tantivy score divided by the top Tantivy score (0.0 to 1.0). Exact lookup hits: 1.0. |
| `status_weight` | verified 1.0, inferred 0.6, candidate 0.3, none 0.5. |
| `recency` | `1.0 / (1.0 + age_days / 30.0)` from the entity time. |
| `kind_weight` | `Timeline` and `Task` classes: task and memory 1.0, others 0.5. Other classes: symbol 1.0, file 0.8, others 0.6. |

## 6. Retrieval levels and expansion

| Level | Content | Default budget |
|-------|---------|----------------|
| L0 Metadata | Refs, titles, paths, ranges, status. No code. | 80–250 tokens. Default for `query`. |
| L1 Evidence | L0 + snippets (3 lines around the match) + memory text + source event IDs. | 250–700 tokens. |
| L2 Source | Full symbol bodies, types, related tests. | Request-defined, up to `query.max_token_budget`. |
| L3 Audit | Original event payloads, full files, full logs, history. | Request-defined, up to `query.max_token_budget`. Permission checked. |

`query` returns level L0. If the token budget is 400 or more, it returns L1.

### 6.1 Expand

```rust
pub struct ExpandRequest {
    pub scope: Scope,
    pub refs: Vec<String>,                // 1 to 20 refs
    pub level: ExpandLevel,               // Evidence (L1), Source (L2), Audit (L3)
    pub token_budget: Option<u32>,
    pub expected_revision: Option<String>,
}

pub struct ExpandResponse {
    pub items: Vec<ExpandItem>,
    pub freshness: Freshness,
    pub token_estimate: u32,
    pub truncated: bool,
    pub cursor: Option<String>,
}

pub struct ExpandItem {
    pub r#ref: String,
    pub status: ExpandStatus,             // Ok, Changed, NotFound, Forbidden
    pub revision: Option<String>,
    pub content: Option<String>,
    pub provenance: Vec<String>,          // source event refs
}
```

- For a symbol at level `Source`: read the current file. Return lines `span[0]..=span[1]`.
- If the current file hash differs from `SymbolRecord.file_hash`, parse the file again in memory. Find the symbol by `qualified_name`. Set `status = Changed`. Return the new span.
- If `expected_revision` is given and differs from the current revision, set `status = Changed`.
- For an event at level `Audit`: read and decompress the payload blob.

## 7. Freshness

Compute freshness for each response:

| Label | Condition |
|-------|-----------|
| `current` | `search_seq >= canonical_seq` and `graph_seq >= canonical_seq`, and no result path is in the live overlay. |
| `partial` | A secondary index is behind, or a result path is in the live overlay, and the engine could read the current source for the affected results. |
| `stale` | A result path is in the live overlay and the engine could not confirm the result from the current file. |

Rules:

- For a result path in the live overlay, read the current file from disk. Check that the symbol name still exists in the file text. If not, drop the result.
- For `ExactRef` queries on events, read CF `events`. Events are readable at once after ACK.
- MUST NOT return `current` when a condition for `partial` or `stale` is true.

## 8. L0 cache

- Use `lru::LruCache` behind one `std::sync::Mutex`. Capacity: `cache.l0_capacity_entries`.
- Cache key: BLAKE3 hash of `(tenant_id, principal_id, workspace_id, repository_id, worktree_id, policy_version, normalized question, mode, token_budget, max_results)`.
- Cache value: the `QueryResponse` and the set of refs and paths it contains, and the `graph_seq` at creation time.
- Cache only responses with `freshness = current`.
- Cache a `not_found` response for `cache.negative_ttl_ms` only.

### 8.1 Invalidation

- Keep a map from ref and from path to cache keys.
- When a job changes an entity or a file, remove all cache entries for that ref or path.
- When an ACL, policy, correction or `forget` operation occurs, clear the full cache for the workspace.
- Do not rely on TTL for positive entries.

## 9. Token firewall

The token firewall applies to all model-facing responses (MCP gateway and `query`/`expand` with default options).

Token estimate: `tokens = ceil(chars / 4)` of the serialized JSON.

Algorithm:

1. Clamp the budget: `budget = min(request.token_budget or default, query.max_token_budget)`.
2. Make the response envelope with no results. Compute its tokens.
3. Take the results in score order.
4. For each result, compute its tokens. If the total stays at or below the budget, add it. Otherwise stop.
5. Never cut a code snippet or a content field in the middle. Add the complete item or do not add it.
6. If one or more results were not added, set `truncated = true`. Set `cursor` = base64 of `{query_id, offset}`. Store the remaining results in the L0 cache for 5 minutes under the `query_id`.
7. If the first item alone is more than the budget, return it as a ref only (no snippet, no content). Set `truncated = true`.

A call with `cursor` returns the next page from the stored results.

## 10. Local answer engine

The engine MAY return `answer_kind = local` with a short `answer` text only for these classes:

| Class | Condition | Answer template |
|-------|-----------|-----------------|
| `SymbolDefinition` | Exactly one symbol matches. | `"<qualified_name> is defined in <path>:<start>-<end> (revision <rev>)."` |
| `Implementation` | One `Verified` path exists. | `"Task <ref> is implemented by commit <sha>, files: <paths>."` |
| `ExactRef` | The entity exists. | `"<kind> <title>, status <status>."` |

Rules:

- MUST NOT make an answer from `Inferred` or `Candidate` edges. Return `evidence` instead.
- MUST NOT invent a relation, a path or a symbol that is not in the store.
- If nothing is found, return `answer_kind = not_found` and an empty `results` list. Add `candidates` only if they exist, with their status.
