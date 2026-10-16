# ADR-001 — Rust for the core engine

- Status: Accepted
- Date: 2026-10-16

## Context
The engine does file scans, hashing, parsing, indexing, graph traversal and caching. These operations are performance-critical. The engine runs for a long time as a daemon. Memory safety and predictable latency are necessary.

## Decision
The core engine SHALL be written in Rust with the Tokio runtime. SDKs and adapters MAY use other languages. They SHALL use the HTTP API only.

## Consequences
- RocksDB, Tantivy, Tree-sitter and BLAKE3 have mature Rust libraries.
- The first build of `rocksdb` compiles C++ code. It needs Xcode Command Line Tools (macOS) or clang (Linux). The first build takes several minutes.
- Non-Rust clients cannot link the engine directly. They use HTTP.
