# Claude Code with and without ZCIE — token benchmark (2026-10-16)

Status: Measured values. Raw data: `benchmarks/results/claude-ab/*.jsonl`. Scripts: `benchmarks/claude-ab/`.

## 1. Question

How many tokens does a real Claude Code session use to answer project questions without ZCIE, and with ZCIE?

## 2. Method

| Item | Value |
|------|-------|
| Client | Claude Code 2.1.301, headless (`claude -p --output-format json`), default model of the user |
| Main corpus | Tantivy source code (9 crates, 446 files, 144,396 lines of Rust), one Git commit, indexed by ZCIE in 7 s |
| Other corpora | This repository (ZCIEngine) and a sandbox repository with two Claude-made commits (`#201`, `#202`) |
| Tasks | 14 questions (`benchmarks/claude-ab/tasks.json`): 6 symbol lookups, 2 "how/explain", 1 multi-file trace, 2 Git history, 3 earlier decisions that exist only in captured conversations |
| Equal for all conditions | Tools Read, Grep, Glob, LS, Bash; no Edit/Write; no subagents; no web; user and project settings and hooks off (`--setting-sources user`); `--strict-mcp-config` |
| A — without ZCIE | No MCP server, no hook |
| B — ZCIE through MCP | `zcie` MCP server (`zcie_query`, `zcie_expand`) plus the system hint "use zcie first" |
| C — ZCIE context injection | No MCP. A `UserPromptSubmit` hook (`adapters/claude-code/hooks/inject-context.sh`) queries ZCIE with the prompt and adds a capsule of 60–300 tokens to the context |
| Metrics | `usage` of the result JSON: input, cache write, cache read, output tokens; `total_cost_usd`; turns; time; correctness (expected substrings in the answer) |
| Total tokens | input + cache write + cache read + output (all tokens the model processed) |
| Spend | $16.73 for all measured runs (corrected for cumulative session costs) |

## 3. Results

### 3.1 Round 1 — A vs B (MCP), 10 tasks × 3 repetitions

| Condition | Runs | Total tokens | Cost $ | Mean turns | Mean time s | Correct |
|---|---:|---:|---:|---:|---:|---:|
| A | 30 | 1,710,135 | 1.474 | 2.9 | 9.0 | 30/30 |
| B | 30 | 2,275,969 | 1.698 | 3.6 | 10.2 | 30/30 |

ZCIE through MCP used **33 % more tokens**. Cause (from stream traces): Claude Code defers MCP tool schemas, so each session spends one extra turn on `ToolSearch`; the model then called `zcie_query` and `zcie_expand` as separate turns. Each turn re-reads the cached context of about 20,000 tokens. The ZCIE payload itself was small (600–1,000 characters).

Fixes made after round 1: `zcie_query` now returns the source of the best symbols in the same call; task questions get a local answer with the verified commit; `exact` mode always uses the symbol route; decision words ("decide", "agreed", "karar") route to memory.

### 3.2 Round 2 — B with `ENABLE_TOOL_SEARCH=false`

Disabling tool deferral removed the `ToolSearch` turn, but it also loaded all deferred built-in tools into the prompt. The context per turn grew from about 20k to about 50k tokens. B used **72 % more tokens** than A. This setting is not recommended. (46 of 60 runs completed; the run hit the 10-minute limit.)

### 3.3 Round 3 — A vs B after the fixes, 14 tasks × 2 repetitions

| Condition | Runs | Total tokens | Cost $ | Mean turns | Correct |
|---|---:|---:|---:|---:|---:|
| A | 28 | 1,561,769 | 1.138 | 2.7 | 23/28 |
| B | 28 | 2,015,108 | 1.860 | 3.5 | 28/28 |

B answered every question correctly. A failed 5 of the 6 earlier-decision questions ("I have no record of that decision"). B still used **29 % more tokens**, because of the MCP turns.

### 3.4 Round 4 — A vs C (context injection), 14 tasks × 3 repetitions

| Condition | Runs | Total tokens | Output | Cost $ | Mean turns | Mean time s | Correct |
|---|---:|---:|---:|---:|---:|---:|---:|
| A | 42 | 2,430,924 | 21,642 | 1.246 | 2.8 | 10.9 | 34/42 |
| C | 42 | 1,943,568 | 17,258 | 1.615 | 2.2 | 8.6 | **42/42** |

| Task group | Total tokens C vs A | Turns A → C |
|---|---:|---|
| Symbol lookups (T1–T3, T5, T6, T8) | −1 % to −34 % (T5 +18 %) | 2.0–3.0 → 2.0–2.3 |
| Explain / how (T4, T7) | −1 %, +25 % | 3.0 → 3.0–4.0 |
| Git history (T9, T10) | −22 %, +1 % | 3.0 → 2.3, 2.0 → 2.0 |
| Earlier decisions (T11–T13) | **−29 % to −68 %** | 2.3–3.0 → **1.0–1.7** |
| Multi-file trace (T14) | −18 % | 5.0 → 4.0 |

C used **20 % fewer tokens**, 20 % fewer output tokens, 21 % fewer turns and 21 % less time, with 8 more correct answers. The **cost was 30 % higher**: each one-question session writes the injected capsule into the prompt cache, and a cache write costs about 12 times a cache read.

### 3.5 Round 5 — long sessions: 11 Tantivy questions in one session (`--resume`), 2 sessions per condition

| Condition | Total tokens / session | Cost / session | Turns | Correct |
|---|---:|---:|---:|---:|
| A | 642,108 | $1.659 | 29.0 | 9/11 |
| C | 636,119 | $1.701 | 24.0 | **11/11** |

In a long session the token use and the cost were the same within noise (−1 % tokens, +3 % cost). C needed 17 % fewer turns and answered all questions.

### 3.6 Correction of the long-session costs (rounds 5 and later)

In a resumed session (`--resume`), `total_cost_usd` is **cumulative** for the whole session, while `usage` is per call. Section 3.5 added the cumulative costs step by step. The corrected session costs are: A $0.348, C $0.339 (−3 %). The token numbers were correct.

### 3.7 Where Claude Code spends tokens — context probe

One-turn prompt, second run (warm cache), Tantivy folder (`benchmarks/claude-context/probe.py`, raw data `claude-ab/context-probe.jsonl`):

| Configuration | Context per turn (tokens) |
|---|---:|
| Default (26 tools, claude.ai MCP connectors, skills) | 23,141 |
| Without MCP connectors | 21,205 |
| … and without skills | 20,784 |
| … and only 6 tools (`--tools Read,Grep,Glob,Bash,Edit,Write`) | 8,170 |
| … and no dynamic sections, no user settings | 7,482 |
| `permissions.deny` of 21 unused tools (5 tools remain) | **6,608** |
| Floor: custom one-line system prompt and 6 tools | 5,757 |

**About 60 % of the context of every turn is built-in tool definitions** (Cron, Workflow, RemoteTrigger, PushNotification, DesignSync, SendMessage, Monitor, NotebookEdit, WebFetch, WebSearch, Task, …). Claude Code sends them again on every turn. The lean profile `adapters/claude-code/lean-settings.json` removes them with `permissions.deny`, so it also works for interactive `claude`.

### 3.8 Rounds 6 and 9 — A vs lean (L) vs lean + selective ZCIE injection (LZ)

Selective injection (ADR-008 I-35): the hook adds a capsule only for an exact answer, a task/history/memory question, or a symbol named in the prompt.

Final round 9, single-question sessions (14 tasks × 2):

| Condition | Tokens / question | Cost / question | Turns | Time | Correct |
|---|---:|---:|---:|---:|---:|
| A — default | 49,278 | $0.0119 | 2.5 | 10.0 s | 22/28 |
| L — lean tool profile | 19,749 (−60 %) | $0.0085 (−29 %) | 2.5 | 12.1 s | 22/28 |
| **LZ — lean + ZCIE** | **15,343 (−69 %)** | **$0.0059 (−50 %)** | **1.9** | **8.3 s** | **28/28** |

Final round 9, long sessions (11 questions in one session, 2 sessions each):

| Condition | Tokens / session | Cost / session | Turns | Correct |
|---|---:|---:|---:|---:|
| A — default | 531,456 | $0.133 | 25 | 8/11 |
| L — lean | 266,822 (−50 %) | $0.119 (−11 %) | 28 | 8/11 |
| **LZ — lean + ZCIE** | **238,858 (−55 %)** | **$0.103 (−23 %)** | **22** | **11/11** |

Round 6 (same setup, injection not yet selective) gave LZ −68 % tokens and −65 % cost for single questions. Token savings are larger than cost savings, because the saved tokens are mostly cache reads (about 10 % of the input price); cache writes (1-hour TTL) and output dominate the bill.

### 3.9 Round 11 — `zcie claude setup` (final), including code editing

15 tasks (11 questions, 4 code edits checked in the edited files) × 2 repetitions. Z = the settings that `zcie claude setup` writes; Z5 = Z with `--cache-5m`.

| Group | Condition | Tokens / task | Cost / task | Turns | Correct |
|---|---|---:|---:|---:|---:|
| All | A — default | 50,985 | $0.0230 | 2.7 | 22/30 |
| All | **Z — zcie claude setup** | **18,841 (−63 %)** | **$0.0132 (−42 %)** | 2.2 | **30/30** |
| All | Z5 — setup + 5-min cache | 19,768 (−61 %) | $0.0106 (−54 %) | 2.4 | 29/30 |
| Code editing | A | 63,329 | $0.0506 | 3.0 | 6/8 |
| Code editing | Z | 30,446 (−52 %) | $0.0260 (−49 %) | 3.2 | 8/8 |
| Memory / decisions | A | 38,487 | $0.0236 | 2.4 | 0/8 |
| Memory / decisions | Z | 13,186 (−66 %) | $0.0113 (−52 %) | 1.5 | 8/8 |

### 3.10 All tools known, minimum tokens — alternatives (rounds 12–19)

Raw request capture (`benchmarks/claude-context/capture_request.py`): Claude Code sends each loaded tool as `name` + `description` + `input_schema` in `tools`, on every turn. Default: 25 tools ≈ 18,400 tokens, plus the skill list ≈ 4,300–5,000 tokens and agent types ≈ 400 tokens in the first message.

| Option | Context / turn | Single question: tokens / cost | Long session: tokens / cost |
|---|---:|---:|---:|
| Default | ≈ 27,400 | 0 / 0 | 0 / 0 |
| `ENABLE_TOOL_SEARCH=true` (all tools deferred by name) | ≈ 16,300 | −17 % / +41 % | −12 % / +5 % |
| `balanced` profile (15 denied) | ≈ 14,100 | −35 % / +71 % | −31 % / +41 % |
| **`lean` profile (21 denied) + catalog of denied tools + ZCIE** | **≈ 6,500** | **−65 % / −41 %** | **−55 % / −14 %** |

The deferred and balanced options put a large block after the varying prompt, so each session writes it to the prompt cache again; their cost goes up. The lean profile keeps all tools *known* through a one-line-per-tool catalog at session start (≈ 150 tokens); a denied tool is enabled by the user (`zcie claude tools enable <Name>`, or a message that names the need). Final accuracy with the lean profile: 30/30 single tasks, 11/11 long-session questions (default: 22/30, 8/11).

### 3.11 Automatic tool loading without extra cost — `auto` profile (rounds 20–21)

`auto` = `ENABLE_TOOL_SEARCH=true` and only the 6 non-deferrable heavy tools denied (`Skill`, `Agent`, `Workflow`, `ScheduleWakeup`, `ReportFindings`, `ListAgents`). Context per turn ≈ 6,500 tokens (default ≈ 27,400). The model loads WebSearch, WebFetch, LSP, NotebookEdit, Cron and the others itself through ToolSearch; a test session used `ToolSearch` → `WebSearch` with no hint.

| | A — default | Z — lean | **H — auto** |
|---|---:|---:|---:|
| Single question, cost | $0.0228 | −46 % | **−26 %** |
| Code edit, cost | $0.0517 | −47 % | **−46 %** |
| Long session (6 sessions each), cost | $0.132 ± 0.018 | −15 % | **+2 % (± 0.012)** |
| Long session, tokens | 523,008 | −52 % | **−44 %** |
| Correct | 22/30, 8/11 | 30/30, 11/11 | **30/30, 11/11** |

## 4. Findings

0. **Final answer (round 11).** `zcie claude setup` reduces tokens by 63 % and cost by 42 % (54 % with `--cache-5m`), and answered 30 of 30 tasks correctly, against 22 of 30 for the default Claude Code. Earlier rounds: The combination of the lean tool profile and selective ZCIE injection reduces tokens by 55–69 % and cost by 23–50 %, and answers all questions correctly (rounds 6–9). ZCIE through MCP alone (rounds 1–3) did not save tokens.
1. **Tokens (ZCIE alone).** With Claude Code, ZCIE alone did not reduce the total token use in a measurable way for these question types. Claude Code already finds code with `Grep -n` in one call, and prompt caching makes the re-read context cheap. The cost of a question is dominated by the cached context that each turn reads again (20k–100k tokens), not by the size of search results.
2. **MCP has a fixed overhead in Claude Code.** Deferred tool loading adds one turn per session. For short questions this overhead is larger than what ZCIE saves.
3. **Context injection is the right integration.** It adds no tool turns. It saved 20 % of the tokens per question and 17–21 % of the turns, and it was faster.
4. **Knowledge that is not in the repository is the main gain.** For earlier decisions that exist only in captured conversations, Claude Code without ZCIE answered **0 of 15** (the automatic check passed 2 answers, but both said "no record found" and only quoted the code default `B = 0.75`). With ZCIE it answered 15 of 15, mostly in one turn, with up to 68 % fewer tokens.
5. **ZCIE's own retrieval cost is small.** The injected capsule is 60–300 tokens; a full `zcie_query` answer is 160–900 tokens.

## 5. Recommendations

- Use the lean tool profile (`adapters/claude-code/lean-settings.json`). It is the largest single saving. Remove a tool from the deny list if you need it (for example `WebFetch`, `Task`).
- Use the selective context-injection hook (`inject-context.sh`) as the default ZCIE integration. Both are enabled in `.claude/settings.json` of this repository.
- Keep the MCP server for explicit deep lookups (`zcie_expand`, audit), not as the first step of each question.
- Do not set `ENABLE_TOOL_SEARCH=false`.
- Measure again on long coding sessions with edits (not only questions), on repositories larger than 1M lines, and with a model that has no prompt cache. In these cases the turn savings are expected to matter more, but this report does not show it.

## 6. Limits

- One model, one machine, one day. Two or three repetitions per cell; the variance between repetitions is up to ±20 % per task.
- Questions only, no editing tasks.
- The correctness check is a substring match. It gave 2 false positives (T13, condition A, rounds 3 and 4). The corrected accuracy of condition A is 22/28 (round 3) and 33/42 (round 4).

## Round 22 — project ID files (ADR-009), regression check

Same 15 tasks, 2 repetitions, conditions A (default Claude Code) and Z (`zcie claude setup`, auto profile).
Z finds the project from `.claude/zcie_id` (no `ZCIE_WORKSPACE`). Queries send `cross_project: true`.
File: `claude-ab/round22-project-id-single.jsonl`.

| Round | Condition | Runs | Correct | Cost (USD) | Tokens/run | Cache writes/run | Turns |
|-------|-----------|------|---------|-----------|------------|------------------|-------|
| 20 | A | 30 | 24 | 0.683 | 53,983 | 3,210 | 2.70 |
| 20 | Z | 30 | 30 | 0.372 | 19,077 | 1,664 | 2.20 |
| 22 | A | 30 | 22 | 1.009 | 51,729 | 6,144 | 2.57 |
| 22 | Z | 30 | 30 | 0.517 | 20,114 | 2,889 | 2.20 |

Result: no regression. Z against A in the same round: cost −46 % (round 20) and −49 % (round 22); tokens −65 % and −61 %; 30/30 correct in both rounds.
The absolute cost of both conditions is higher in round 22 because the prompt cache was cold (cache writes about 2× in both conditions).
