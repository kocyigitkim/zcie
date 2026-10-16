# ADR-006 — Two MCP tools only

- Status: Accepted
- Date: 2026-10-16

## Context
Each MCP tool definition adds tokens to the model context in each session. Many tools also make tool selection harder for the model.

## Decision
The MCP gateway SHALL expose two tools by default: `zcie_query` and `zcie_expand`. Capture SHALL use client hooks and the HTTP API, not a model tool call. Administrative operations SHALL use the CLI or the HTTP API.

## Consequences
- The tool-list cost is small and constant.
- The model cannot write memory through MCP by default. This is intentional.
