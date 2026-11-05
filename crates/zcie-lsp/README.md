# crates/zcie-lsp

Language-server enrichment for "who calls X" (ADR-033 item h).

- Starts rust-analyzer or typescript-language-server on the first Callers query. It stops after `[lsp] idle_minutes` without a query.
- Returns `None` until the server is ready (`timeout_ms`, default 3000 ms). The caller then uses its heuristic answer.
- Limits: rust-analyzer uses about 550 MB RSS on a small crate. TypeScript 7 has no `tsserver.js`, so typescript-language-server needs TypeScript 5.x.
- Settings: `[lsp]` in `docs/15-configuration.md`. Tests skip with a line when a server is missing.

Read `docs/README.md` before you change this folder.
