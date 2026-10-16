#!/bin/sh
# ZCIE context injection for the Claude Code UserPromptSubmit hook.
# It queries the local index with the prompt and prints a short capsule. Claude Code adds
# hook stdout to the model context, so the model can answer without extra tool turns.
# It captures nothing. Rule: never block the session; every path ends with exit 0.

. "$(dirname "$0")/common.sh"

ZCIE_INJECT_BUDGET="${ZCIE_INJECT_BUDGET:-500}"

zcie_require_tools
zcie_read_input
prompt="$(zcie_field '.prompt')"
[ -n "$prompt" ] || exit 0

body="$(jq -n --arg w "$(zcie_workspace)" --arg q "$(printf '%s' "$prompt" | cut -c1-1900)" \
    --argjson b "$ZCIE_INJECT_BUDGET" '{workspace: $w, question: $q, token_budget: $b}' 2>/dev/null)" || exit 0
response="$(curl --max-time 2 --silent \
    -H "Content-Type: application/json" \
    -H "Authorization: Bearer ${ZCIE_API_KEY:-}" \
    -X POST --data "$body" "${ZCIE_URL}/v1/query" 2>/dev/null)" || exit 0

# Inject only when the capsule is useful: an exact answer, a task/history/memory class, or a
# symbol whose name is in the prompt. Generic chat gets no capsule (no extra cache write).
printf '%s' "$response" | jq -r --arg prompt "$prompt" '
  ($prompt | ascii_downcase) as $p
  | ($p | .[0:60]) as $head
  # Drop echoes of the prompt itself (the capture hook can store it first).
  | .results |= map(select((.title | ascii_downcase | contains($head)) | not))
  # Keep one copy of a decision: drop an event whose text is also a memory record.
  | ([.results[] | select(.kind == "memory") | .title[0:60]]) as $memories
  | .results |= map(select(.kind != "event" or ((.title | sub("^[a-z.]+: "; "")[0:60]) as $t | ($memories | index($t)) == null)))
  # A symbol counts only if its name looks like an identifier (snake_case, CamelCase or a path),
  # so plain words such as "claude" or "index" do not trigger a capsule.
  | def named_symbol: [.results[] | select(.kind == "symbol") | .title | split(".") | last | split("::") | last
        | select(length >= 4 and test("_|[a-z][A-Z]")) | ascii_downcase]
      | any(.[]; . as $name | ($p | contains($name)));
  select(.answer_kind != null and .answer_kind != "not_found" and (.results | length) > 0)
  | select(.answer != null or (.class | IN("task", "implementation", "timeline", "exact_ref")) or named_symbol)
  | "ZCIE context (local index of this project and of earlier sessions; refs are cited, statuses are verified/inferred/candidate):",
    (if .answer then "answer: " + .answer else empty end),
    (.results[:5][] | "- " + .kind + " " + .title
        + (if .path then " — " + .path + (if .range then ":" + (.range[0]|tostring) + "-" + (.range[1]|tostring) else "" end) else "" end)
        + (if .status then " [" + .status + "]" else "" end)
        + (if .snippet and (.kind == "memory" or .kind == "event") then "\n    full text: " + .snippet else "" end)),
    "Use these facts directly. Read files only if you need more detail."
' 2>/dev/null
exit 0
