#!/bin/sh
# Minimal custom client: register a repository, capture one message, run one query.
# Usage: ZCIE_API_KEY=zcie_... ./example.sh /path/to/repo
set -u

ZCIE_URL="${ZCIE_URL:-http://127.0.0.1:47831}"
WORKSPACE="${ZCIE_WORKSPACE:-example}"
REPO_PATH="${1:-$(pwd)}"
AUTH="Authorization: Bearer ${ZCIE_API_KEY:?set ZCIE_API_KEY}"
JSON="Content-Type: application/json"

echo "1. Register repository $REPO_PATH"
curl --silent --max-time 10 -H "$AUTH" -H "$JSON" -X POST "$ZCIE_URL/v1/repositories" \
    --data "$(jq -n --arg ws "$WORKSPACE" --arg path "$REPO_PATH" '{workspace: $ws, path: $path}')"
echo

echo "2. Capture one user message"
curl --silent --max-time 10 -H "$AUTH" -H "$JSON" -X POST "$ZCIE_URL/v1/events" \
    --data "$(jq -n --arg ws "$WORKSPACE" '{
        schema: "zcie.event.v1",
        idempotency_key: "custom-example:session-1:turn-1",
        workspace: $ws,
        source: {app: "custom-example", session_id: "session-1", turn_id: "turn-1"},
        type: "user.message",
        payload: "Fix #1: make the example query work."}')"
echo

echo "3. Query"
curl --silent --max-time 10 -H "$AUTH" -H "$JSON" -X POST "$ZCIE_URL/v1/query" \
    --data "$(jq -n --arg ws "$WORKSPACE" '{workspace: $ws, question: "what did I ask before?", token_budget: 250}')"
echo
exit 0
