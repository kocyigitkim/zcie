# ADR-033 — Read guard for Bash, subagent context, keyword engine, session handoff, LSP

- Status: Accepted
- Date: 2026-11-05
- Tasks: D-112 to D-119 of `docs/12-development-plan.md`
- Item h supersedes ADR-029 item 6 on the request of the user (2026-11-05).

## Context
Measured on the sessions of the user, 2026-10-27 to 2026-11-05. The total weighted cost is 134 M.
- Four sessions give 95 % of the cost.
- Subagents give 49 to 69 % of the cost.
- Subagents read files with Bash `cat`, `sed` and `grep`. These reads were 2.7 to 4.2 M characters per session. The Read guard did not see them.
- The main thread reads screenshots. There are 47 to 190 per session. Each costs about 3,000 tokens. They give 14 to 34 % of the main cost.
- Sessions stay open for 11 to 16 days. The median context is 190,000 to 320,000 tokens.
- Output is 19 to 35 % of the cost.
- Only 4 of 40 typed prompts got a capsule.
- The keyword classifier had a joint accuracy of 57.5 % on a held-out set.
- Auto mode refuses an Agent call or a Bash call whose input a hook changed (`updatedInput`).

## Decision
a. **Read guard for Bash (D-112).** The read guard matcher is `Read|Bash`. A whole-file `cat` of a text file is refused. A `sed -n` range wider than `ZCIE_READ_MAX_LINES` (default 400) is refused. The hook returns `permissionDecision: deny`. The message gives the window command and the symbol outline of the file. The hook does not rewrite the command, because auto mode refuses a rewritten command. A Read of a PNG or JPG larger than `ZCIE_IMAGE_MAX_PX` (default 1200; 0 turns it off) reads a copy scaled with `sips`. This works on macOS only.

b. **Subagent context (D-113).** The `PreToolUse` hook with matcher `Agent|Task` queues the task unchanged. The `SubagentStart` hook returns the capsule as `additionalContext`. The `SubagentStop` hook stores a note. The note has the read ranges and the first lines of the report. The next subagents get the last 5 notes. Each subagent context ends with: "End with a short report: at most 15 lines; write details into files, not into the report."
   Known limit: when two subagents start at the same time, their capsules can swap.

c. **Keyword engine (D-114).** The files are `crates/zcie-query/src/keywords.rs`, `keyword_lists.rs`, `classify.rs` and `intent.rs`. The engine does the following:
   - It folds Turkish letters before it matches.
   - It matches only at the start of a word.
   - An ambiguous word matches only as an exact word. The word `karar` is one example.
   - The words "neden" and "why" use context rules.
   - It allows one typing error. This applies to words of 5 letters or more.
   - It scores each class. It no longer takes the first match.
   - Each class and intent has a family of phrases.

   Measurement: the test data is in `crates/zcie-query/tests/data/keyword_cases*.json`. The joint accuracy on sets the engine never saw was:
   - 57.5 % before the change
   - 71.7 % after the first change
   - 84 % after the second change
   - 86 % after the third change
   - 90.0 % on the final set `unseen4` (200 cases). Class accuracy was 95.5 % and intent accuracy was 93.5 %.
   
   The earlier sets are now tuned to 100 %. They do not measure generalization. Write a new set only with a person who does not read the code.

d. **Session handoff (D-115).** `session-guard.sh` uses the real context size. It reads the usage of the last main-thread message. The context size is `ZCIE_HANDOFF_TOKENS` (default 150000). When the context is at least this size and the prompt has no overlap with the files of the session, the guard does two things. It shows a "new topic" message. It writes a handoff note in the background.
   - `PreCompact` writes the note.
   - `SessionStart` with matcher `clear|compact` prints the note once. The note is used only when it is younger than `ZCIE_HANDOFF_MAX_AGE_H` (default 48 hours).
   - Guard runtime limit: under 0.5 s on a 140 MB transcript. The measured value is 0.31 s.
   - `PreCompact` runtime limit: 10 s. The measured value is 3.75 s.

e. **Project card (D-116).** A general prompt of at least 6 words that gets no capsule gets the project card. It gets the card once per session.

f. **Usage explain and bar fields (D-117).** `zcie usage --explain` shows the cause of the cost. The session rows get these fields: `images`, `image_tokens_est`, `bash_read_chars`, `avg_context`, `output_share` and `cause`. The ZCIE Bar app shows the context size and the main cause.

g. **Memory prune (D-118).** `zcie memory prune [--all-projects] [--apply]` is a dry run by default. It removes two kinds of records:
   - Records with the prefix of a subagent report.
   - Decision records that have code fences, or that have 40 % or more non-letter characters.

h. **LSP (D-119).** The new crate is `crates/zcie-lsp`. The settings are `[lsp] enabled = true`, `idle_minutes = 10` and `timeout_ms = 3000`.
   - rust-analyzer and typescript-language-server start when a Callers query needs them.
   - Until a server is ready, the answer is `None`. Then the heuristic answer is used.
   - A server stops when it is idle.
   - Measured: a small Rust crate is ready in 2 to 3 s. rust-analyzer uses 550 MB RSS on a small crate.
   - TypeScript 7 has no `tsserver.js`. typescript-language-server needs TypeScript 5.x.

## Consequences
- The new hook registrations need `zcie claude setup` again. Or add the entries by hand. These are: `PreCompact`, `SessionStart` with `clear|compact`, `SubagentStart`, `SubagentStop`, and the read guard with `Read|Bash`.
- rust-analyzer uses a lot of memory while it runs. It stops after 10 idle minutes.
- A Bash refusal costs one more model turn.
- The keyword accuracy of 90.0 % is measured on one set of 200 cases. Real prompts can differ.
