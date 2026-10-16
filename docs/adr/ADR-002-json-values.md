# ADR-002 — JSON for stored values

- Status: Accepted
- Date: 2026-10-16

## Context
Stored values need a format. Binary formats (bincode, postcard) are smaller and faster. JSON is easy to read, easy to debug and easy to migrate. Simple code is a project rule (C-01 to C-17).

## Decision
All RocksDB values SHALL be JSON (`serde_json`). Each value SHALL have the field `"v"` with the schema version. Encode and decode values only in `zcie-store/src/codec.rs`.

## Consequences
- An engineer can read any value with a simple dump command.
- Values are larger than in a binary format. Large payloads are in the object store, so the effect is small.
- A later ADR can change the codec. Only `codec.rs` changes.
