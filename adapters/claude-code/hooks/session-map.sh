#!/bin/sh
# SessionStart hook (startup): print the repository map of the project once (ADR-017). The map
# names the most used files and their main symbols, so the model goes to the right file without a
# file scan. A project with no indexed folder gets no map. Rule: never block; exit 0.
. "$(dirname "$0")/common.sh"
zcie_require_tools
zcie_read_input
project="$(zcie_project_id)"
[ -n "$project" ] || exit 0
body="$(jq -n --arg w "$project" '{workspace: $w, question: "", mode: "map", token_budget: 600}' 2>/dev/null)" || exit 0
map="$(curl --max-time 3 --silent -H "Content-Type: application/json" -H "Authorization: Bearer ${ZCIE_API_KEY:-}" \
    -X POST --data "$body" "${ZCIE_URL}/v1/query" 2>/dev/null | jq -r '.answer // empty' 2>/dev/null)"
[ -n "$map" ] || exit 0
printf '%s\n%s\n' "ZCIE repository map (the most used files of this project and their main symbols; use it to go to the right file):" "$map"
exit 0
