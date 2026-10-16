# ADR-004 — Use the git command-line program

- Status: Accepted
- Date: 2026-10-16

## Context
ZCIE needs HEAD, commit lists, changed paths and the file list. The `git2` crate needs libgit2. The `gix` crate has a large API. The `git` program is on every developer computer.

## Decision
`zcie-git` SHALL run the `git` program with `std::process::Command`. It SHALL use machine-readable flags (`-z`, `--name-status`, `--format`). It SHALL parse the output with simple string functions.

## Consequences
- The code is short and easy to read.
- Each call starts a process. The cost is acceptable because Git calls are not on the query path.
- If `git` is not installed, the engine indexes the folder as a plain folder and marks Git features as unavailable.
