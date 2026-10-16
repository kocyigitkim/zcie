# ADR-005 — Search indexes are derived

- Status: Accepted
- Date: 2026-10-16

## Context
RocksDB and Tantivy are two separate stores. No atomic transaction covers both. A crash can occur between the two writes.

## Decision
RocksDB SHALL be the canonical store. Tantivy (and the OPTIONAL vector index) SHALL be derived views. Each derived index SHALL keep its own watermark. On startup the daemon SHALL replay the missed records or rebuild the index.

## Consequences
- A crash never loses canonical data.
- Search can lag behind capture. The query path uses the live overlay for read-your-writes.
- The command `zcie rebuild-search` can delete and rebuild the search index at any time.
