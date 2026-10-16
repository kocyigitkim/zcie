# 00 — Rules for the Implementing Agent

Status: Normative. Read this file first. Obey all rules in this file.

## 1. Purpose of this file

This file tells the implementing agent how to work on ZCIE.
The agent is an AI coding model (for example, Claude Sonnet 5.5).
A human engineer can use the same rules.

## 2. Work order (mandatory)

Do the work in this order. Do not change the order.

1. Read `docs/README.md`. Then read the documents in the given reading order.
2. Do the development tasks in `docs/12-development-plan.md`. Start at task `D-001`. Do the tasks in number order.
3. Do not write tests during development tasks. Do not run tests during development tasks.
4. After each development task, run `cargo build --workspace` and `cargo clippy --workspace`. Fix all errors. Fix all warnings.
5. When ALL development tasks are complete, start the test phase in `docs/13-test-plan.md`.
6. Write the tests. Run the tests. Fix the code until all tests pass.
7. When ALL tests pass, examine the acceptance criteria in `docs/14-acceptance-criteria.md`.
8. Record the result of each acceptance criterion in `docs/14-acceptance-criteria.md`.

> **Warning:** Do not start the test phase before all development tasks are complete.
> Do not examine the acceptance criteria before all tests pass.

## 3. Task procedure

For each development task, do these steps:

1. Read the task in `docs/12-development-plan.md`.
2. Read the documents that the task names in its "Read" field.
3. Change only the files that the task names in its "Files" field.
4. Do the steps in the task. Do the steps in the given order.
5. Make sure that each item in the "Done when" list is true.
6. Run `cargo build --workspace`. Run `cargo clippy --workspace -- -D warnings`.
7. Mark the task as complete in `docs/12-development-plan.md`. Change `[ ]` to `[x]`.

If a task is not clear, do not guess a large design. Select the simplest solution that agrees with the documents. Write the decision in `docs/adr/` as a new ADR file.

## 4. Code simplicity rules (mandatory)

The code must be simple. A new engineer must understand each function in one reading.

| ID | Rule |
|----|------|
| C-01 | Keep each function at 40 lines or fewer. |
| C-02 | Keep each source file at 400 lines or fewer. Split a larger file into modules. |
| C-03 | Do not write your own macros (`macro_rules!`, procedural macros). Use `derive` macros from approved crates only. |
| C-04 | Do not use generic type parameters if one concrete type is sufficient. |
| C-05 | Do not use trait objects or traits if only one implementation exists. Exception: the `VectorIndex` trait in `zcie-vector`. |
| C-06 | Do not use `unsafe` code. |
| C-07 | Do not use `unwrap()` or `expect()` outside of `main()` and tests. Return a `Result`. |
| C-08 | Use one error type per crate. Define it with `thiserror`. Apps use `anyhow`. |
| C-09 | Use clear names. Do not use abbreviations except the terms in `docs/02-glossary.md`. |
| C-10 | Write one doc comment (`///`) for each public function, struct and enum. Use one or two sentences. |
| C-11 | Do not optimize before a benchmark shows a problem. Write the simple version first. |
| C-12 | Use only the crates in the approved list (section 6). To add a crate, write an ADR first. |
| C-13 | Do not create a crate that the development plan does not name. |
| C-14 | Do not add a feature that the documents do not specify. |
| C-15 | Use `tracing` for logs. Do not use `println!` in library crates. |
| C-16 | Do not hold a lock across an `.await` point. |
| C-17 | Put all constants (limits, ports, sizes) in one `limits.rs` or in configuration. Do not put numbers inline. |

## 5. Documentation language rules (STE-100-based)

All documents and code comments use controlled, simplified technical English.
The rules come from ASD-STE100. This project does not claim formal STE-100 certification.

| ID | Rule |
|----|------|
| W-01 | Write procedural sentences with 20 words or fewer. |
| W-02 | Write descriptive sentences with 25 words or fewer. |
| W-03 | Write one instruction in one sentence. |
| W-04 | Use the imperative form for instructions ("Write the event."). |
| W-05 | Use the active voice. |
| W-06 | Use a word with one meaning only. Use the terms in `docs/02-glossary.md`. |
| W-07 | Do not use "-ing" forms as nouns or adjectives where an alternative exists. |
| W-08 | Use articles ("the", "a") where possible. |
| W-09 | Write a paragraph with six sentences or fewer. |
| W-10 | Write a warning or caution before the instruction it applies to. |
| W-11 | Use "SHALL" or "MUST" for a requirement. Use "MAY" for an option. Use "SHALL NOT" for a prohibition. |
| W-12 | Use numbered lists for steps. Use bulleted lists for items with no order. |

## 6. Approved crates

Use the latest stable version of each crate at the time of task `D-001`. Record the versions in `Cargo.lock`. Put shared versions in `[workspace.dependencies]`.

| Crate | Use |
|-------|-----|
| `serde`, `serde_json` | Serialization of all stored values and API bodies. |
| `thiserror` | Error types in library crates. |
| `anyhow` | Error handling in `apps/` only. |
| `tokio` | Async runtime. |
| `tracing`, `tracing-subscriber` | Logs. |
| `rocksdb` | Canonical store. |
| `blake3` | Content hashes. |
| `zstd` | Blob compression. |
| `getrandom` | Random bytes for API keys. |
| `ulid` | Unique IDs. |
| `time` | UTC timestamps (RFC 3339). |
| `axum` | HTTP gateway. |
| `reqwest` (blocking + json features) | HTTP client in the CLI, the MCP gateway and the Rust SDK. |
| `clap` (derive feature) | CLI argument parsing. |
| `toml` | Configuration file parsing. |
| `ignore` | File discovery with `.gitignore` rules. |
| `globset` | Exclusion globs (`security.exclude_globs`). |
| `notify` | File watcher. |
| `tree-sitter`, `tree-sitter-rust`, `tree-sitter-typescript`, `tree-sitter-python` | Syntax parsing. |
| `tantivy` | Lexical search. |
| `lru` | L0 cache. |
| `rmcp` | MCP protocol (stdio). |
| `regex` | Tier A extraction and secret redaction. |
| `tempfile` | Tests only (dev-dependency). |

Use the `git` command-line program for Git data. Do not use `git2` or `gix`. See `docs/adr/ADR-004-git-cli.md`.

## 7. When to stop and ask

Stop and ask the human if one of these conditions occurs:

- A task needs a crate that is not in section 6.
- A task conflicts with another document.
- A build error stays after three attempts to fix it.
- A task needs network access to a remote service other than crate download.
