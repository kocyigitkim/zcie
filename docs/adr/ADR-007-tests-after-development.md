# ADR-007 — Tests after all development tasks

- Status: Accepted
- Date: 2026-10-16

## Context
The project owner requires a fixed order of work. All development tasks come first. Then all tests. Then the acceptance check.

## Decision
The implementing agent SHALL NOT write or run tests during development tasks. Each development task SHALL end with `cargo build` and `cargo clippy` only. The test phase (`docs/13-test-plan.md`) SHALL start after the last development task. The acceptance phase (`docs/14-acceptance-criteria.md`) SHALL start after all tests pass.

## Consequences
- Development tasks are short and focused.
- Some defects are found late. The test phase includes time to fix them.
- Each development task has a clear "Done when" list. This list replaces early tests as the progress check.
