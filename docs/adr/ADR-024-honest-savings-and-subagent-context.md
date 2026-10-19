# ADR-024 — Honest savings, ZCIE context for subagents, watched indexes

- Status: Accepted
- Date: 2026-10-19
- Requirement of the user: "Add the indexing, add ZCIE context to the subagents, and make the report honest."

## Context
The report showed +117 % cost with ZCIE for hellfurylanguage. The analysis of the transcripts (each model call, subagents included) showed:
1. One session with ZCIE (3 turns) had 6 general-purpose subagents that coded whole features: 95 to 285 model calls each, contexts up to 950k tokens, 94 % of the cost of the session. With 2 sessions in the group, the median was set by this one task.
2. ZCIE added about 1,000 tokens to the sessions (capsules and the session note).
3. The subagents got no ZCIE context (no UserPromptSubmit hook in a subagent) and searched with `grep`, `sed` and `cat`. The project was memory only (`indexed=false`): its code index was a stale snapshot.
4. A plugin (ECC `llm-summary.js`) starts `claude -p` sessions that summarize the sessions. They cost tokens and were not shown.

## Decision
1. **Session list.** Each project lists its sessions: start, ZCIE, turns, cost, cost per turn, subagents and their share of the cost (`usage/honest.rs`).
2. **Outliers.** With 3 sessions or more in a project, a session that costs 3 times the median cost per turn of the project or more is an outlier. It is listed and marked, and the saving leaves it out (`saving.excluded`).
3. **Confidence.** `low` with fewer than 3 sessions in a group or a baseline of all projects, `medium` with fewer than 10, else `high`. The app shows it next to the saving.
4. **Automated sessions.** The report gives the number and the cost of the sessions that a program started in the window.
5. **Subagent context.** `subagent-context.sh` (PreToolUse, matcher `Agent|Task`) runs the context hook with the task of the subagent and its own session key, and adds the capsule to the task (`updatedInput.prompt`). Budget 2,000 tokens. Only a real capsule is added. The hook records `subagent_context` in the hook statistics.
6. **Watched indexes.** The projects that the user works in are added with `zcie project add` (hellfurylanguage and its nested repositories, ZCIEngine).

## Consequences
- A project with few sessions still shows a value (decision of 2026-10-19), but with low confidence and with its sessions listed.
- A subagent task is longer by the capsule (at most about 2,000 tokens), once for each subagent.
