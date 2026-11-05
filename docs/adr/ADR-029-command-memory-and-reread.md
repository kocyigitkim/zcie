# ADR-029 — Failed commands, error recall, command playbook, repeated reads, LSP

- Status: Accepted
- Date: 2026-10-23
- Tasks: D-097 to D-100 of `docs/12-development-plan.md`

## Context
- A study of 1,200 Claude Code and Mini-SWE-Agent trajectories (arXiv 2609.30725) found that skills written by developers cut cost by up to 41.7 %. Repeated failed attempts and repeated reads are large cost items.
- A test with Claude Code (2026-10-23, `claude -p`, model haiku) showed: a Bash command with exit code 0 fires `PostToolUse` (`tool_response.stdout`); a command with another exit code fires `PostToolUseFailure` only, with `error` = `"Exit code N\n<output>"`. ZCIE registers `PostToolUse` only, so it never stored a failed command.
- A CircleCI measurement found LSP navigation cut cost by 3 to 14 % against grep on one repository, with high memory use (rust-analyzer: several GB).

## Decision
1. **Failed commands (D-097).** `zcie claude setup` registers `PostToolUseFailure` (matcher `Bash`) with `post-tool-use.sh`. The script stores the failed command as `command.run` with its exit code and output.
2. **Error recall (D-097).** For a failed command, before it stores the event, the script takes the first line of the output that has `error`, `Error`, `ERROR`, `FAILED`, `panicked` or `not found` (not the `Exit code` line). It asks `/v1/query` with this line (search mode, 2 s limit). It shows at most 2 results whose text contains the first 60 characters of the line, as `additionalContext`: "ZCIE: this error was seen before; check what was done then". With no match it prints nothing. Setting: `ZCIE_ERROR_RECALL=false` turns it off.
3. **Command playbook (D-098).** For a Bash command with exit code 0 whose first word is a build, test or check tool (`cargo`, `npm`, `pnpm`, `yarn`, `npx`, `make`, `cmake`, `ctest`, `pytest`, `python -m pytest`, `go`, `dotnet`, `gradle`, `mvn`, `swift`, `hfpm`, `./scripts/…`) and whose command has `build`, `test`, `check`, `clippy`, `lint` or `run`, the script appends the command to `<zcie home>/run/playbook/<project id>.tsv`. The file keeps the 20 newest different commands. At session start, `session-context.sh` shows the 5 newest: "Commands that passed in this project before". The file is local and is never sent to the daemon.
4. **Repeated reads (D-099).** The read guard keeps `<tmp>/zcie-<session>.reads` (path, offset, limit, modification time). When the same Read comes again in the same session and the file did not change, the Read runs and gets the note: "You read this range before in this session and the file did not change." The Read is never blocked or shortened. After a context compaction a repeated read is correct; the note only informs.
5. **Turn-weighted median (D-101).** `zcie usage` takes the median cost per turn with each session weighted by its turns. Reason: in container-deploy the group with no ZCIE had an 81-turn session (1.54 M per turn) and a 3-turn part of a session (212 k per turn); the plain median (878 k) showed +73 % for ZCIE, the weighted median shows −4 %.
6. **LSP (D-100).** ZCIE does not add an LSP bridge now. The measured gain (3 to 14 % cost on one repository) is less than the gain of the other items, and the memory use is high. Measure again when an isolated benchmark task fails because a call site was not found.
   Superseded by ADR-033 item h (2026-11-05, user decision).

## Consequences
- The user must run `zcie claude setup` again (or add the entry by hand) to get the `PostToolUseFailure` hook. The other changes need only `zcie doctor --fix`.
- Error recall adds one query (at most 2 s) to each failed command.
- The playbook is a heuristic: a command that passed once can fail later. The session note says "passed before", not "passes".
