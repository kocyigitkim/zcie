# ADR-003 — One daemon is the single writer

- Status: Accepted
- Date: 2026-10-16

## Context
RocksDB permits one read-write process per database. Many clients (Claude Code, OpenCode, IDE, CLI) want access at the same time. If each client opens the database, the second open fails.

## Decision
Only the daemon `zcied` SHALL open the database. The daemon SHALL hold an exclusive lock on `run/daemon.lock`. All clients SHALL use the HTTP gateway or the MCP gateway. The MCP gateway SHALL call the HTTP gateway.

## Consequences
- The access policy, the cache and the write order have one owner.
- Each client request has one extra local HTTP call. The cost is small on loopback.
- If the daemon does not run, clients receive a connection error. Adapters SHALL ignore this error and SHALL NOT block the user.
