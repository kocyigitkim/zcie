#!/bin/sh
# ZCIE read guard for the Claude Code PreToolUse hook (matcher: Read).
# A Read of a large file with no offset/limit is limited to the first lines, and the model gets
# the symbol outline of the file from ZCIE, so it can read only the part it needs.
# Rule: never block the session; on any problem the Read runs unchanged. Every path exits 0.

. "$(dirname "$0")/common.sh"

ZCIE_READ_MAX_LINES="${ZCIE_READ_MAX_LINES:-400}"
ZCIE_READ_HEAD_LINES="${ZCIE_READ_HEAD_LINES:-250}"

zcie_require_tools
zcie_read_input
[ "$(zcie_field '.tool_name')" = "Read" ] || exit 0
[ -z "$(zcie_field '.tool_input.offset')" ] || exit 0
[ -z "$(zcie_field '.tool_input.limit')" ] || exit 0
file="$(zcie_field '.tool_input.file_path')"
[ -f "$file" ] || exit 0
# Images, PDFs and notebooks are not read as lines; a binary file has no line limit.
case "$file" in
    *.png|*.jpg|*.jpeg|*.gif|*.webp|*.bmp|*.ico|*.pdf|*.ipynb) exit 0 ;;
esac
LC_ALL=C grep -Iq . "$file" 2>/dev/null || exit 0
lines="$(wc -l < "$file" 2>/dev/null | tr -d ' ')"
[ -n "$lines" ] && [ "$lines" -gt "$ZCIE_READ_MAX_LINES" ] || exit 0

cwd="$(zcie_field '.cwd')"
rel="${file#"$cwd"/}"
project="$(zcie_project_id)"
body="$(jq -n --arg w "$project" --arg q "$rel" \
    '{workspace: $w, question: $q, mode: "search", token_budget: 2000, max_results: 40}' 2>/dev/null)"
outline="$(curl --max-time 2 --silent -H "Content-Type: application/json" \
    -H "Authorization: Bearer ${ZCIE_API_KEY:-}" -X POST --data "$body" "${ZCIE_URL}/v1/query" 2>/dev/null \
    | jq -r --arg rel "$rel" '[.results[]? | select(.kind == "symbol" and .path == $rel and .range != null)]
        | sort_by(.range[0]) | .[:40][] | "  " + .title + " L" + (.range[0]|tostring) + "-" + (.range[1]|tostring)' 2>/dev/null)"

note="ZCIE read guard: $rel has $lines lines; only lines 1-$ZCIE_READ_HEAD_LINES were read. Read only the part you need with offset/limit; the outline below gives the line ranges. With the zcie MCP server, one zcie_expand call reads several ranges, for example refs: [\"$rel:300-380\"]."
if [ -n "$outline" ]; then
    note="$note Symbol outline (from the ZCIE index):
$outline"
fi
zcie_stat read_guard "{\"lines\": $lines, \"read\": $ZCIE_READ_HEAD_LINES}"
jq -n --argjson input "$(zcie_field '.tool_input' )" --argjson limit "$ZCIE_READ_HEAD_LINES" --arg note "$note" '{
  hookSpecificOutput: {
    hookEventName: "PreToolUse",
    updatedInput: ($input + {limit: $limit}),
    additionalContext: $note
  }
}' 2>/dev/null
exit 0
