#!/bin/sh
# PostToolUse hook: send file.changed for edit tools and command.run for Bash.
# PostToolUseFailure hook (Bash, ADR-029): store the failed command with its exit code, and show
# earlier results that have the same error line. A passed build or test command goes to the playbook.
. "$(dirname "$0")/common.sh"
zcie_require_tools
zcie_read_input
session="$(zcie_field '.session_id')"
tool="$(zcie_field '.tool_name')"
[ -n "$session" ] || exit 0
turn="$(zcie_current_turn "$session")"

# Shows earlier results with the first error line of a failed command (at most 2, 2 s limit).
recall_error() {
    [ "${ZCIE_ERROR_RECALL:-true}" = "true" ] || return 0
    line="$(printf '%s\n' "$1" | grep -v '^Exit code' | grep -m1 -E 'error|Error|ERROR|FAILED|panicked|not found' | cut -c1-200)"
    [ -n "$line" ] || return 0
    # zsh "=word" expansion noise (e.g. "(eval):61: === not found") has no error text to search.
    printf '%s' "$line" | grep -qE '^\(eval\):[0-9]+: =+ not found$' && return 0
    needle="$(printf '%s' "$line" | sed 's/^[[:space:]]*//' | cut -c1-60)"
    body="$(jq -n --arg w "$(zcie_project_id)" --arg q "$line" '{workspace: $w, question: $q, mode: "search", token_budget: 800, max_results: 8}' 2>/dev/null)" || return 0
    found="$(printf '%s' "$body" | curl --max-time 2 --silent -H "Content-Type: application/json" \
        -H "Authorization: Bearer ${ZCIE_API_KEY:-}" -X POST --data-binary @- "${ZCIE_URL}/v1/query" 2>/dev/null \
        | jq -r --arg n "$needle" '[.results[]? | select(((.title // "") + " " + (.snippet // "")) | ascii_downcase | contains($n | ascii_downcase))]
            | .[:2][] | "- [" + .kind + "] " + (((.snippet // .title) | gsub("\\s+"; " "))[:400])' 2>/dev/null)"
    [ -n "$found" ] || return 0
    zcie_stat error_recall '{"found": true}'
    jq -n --arg note "ZCIE: this error was seen before; check what was done then:
$found" '{hookSpecificOutput: {hookEventName: "PostToolUseFailure", additionalContext: $note}}' 2>/dev/null
}

# Adds a passed build, test or check command to the playbook of the project (20 newest, no repeats).
add_playbook() {
    file="$(zcie_playbook_file)"
    [ -n "$file" ] || return 0
    first="$(printf '%s' "$1" | head -n1 | cut -c1-300)"
    printf '%s' "$first" | grep -qE '^(cargo|npm|pnpm|yarn|npx|make|cmake|ctest|pytest|python3? -m pytest|go|dotnet|gradle|\./gradlew|mvn|swift|hfpm|\./scripts/)( |$)' || return 0
    printf '%s' "$first" | grep -qE '(^| |:|/)(build|test|tests|check|clippy|lint|run)( |$|:)' || return 0
    mkdir -p "$(dirname "$file")" 2>/dev/null || return 0
    { grep -vxF "$first" "$file" 2>/dev/null; printf '%s\n' "$first"; } | tail -n 20 > "$file.tmp" 2>/dev/null && mv "$file.tmp" "$file" 2>/dev/null
    return 0
}

case "$(zcie_field '.hook_event_name')" in
    PostToolUseFailure)
        [ "$tool" = "Bash" ] || exit 0
        command="$(zcie_field '.tool_input.command')"
        error="$(zcie_field '.error')"
        code="$(printf '%s\n' "$error" | sed -n '1s/^Exit code \([0-9][0-9]*\).*/\1/p')"
        # The query runs before the event is stored, so it never finds this failure itself.
        recall_error "$error"
        payload="$(jq -n --arg command "$command" --arg output "$(printf '%s' "$error" | cut -c1-4000)" --arg code "$code" \
            '{command: $command, exit_code: (if $code == "" then null else ($code | tonumber) end), output: $output}' 2>/dev/null)" || exit 0
        zcie_send "command.run" "$session" "$turn" "post-tool-use-failure" "$payload"
        exit 0
        ;;
esac
case "$tool" in
    Edit|Write|MultiEdit|NotebookEdit)
        path="$(zcie_field '.tool_input.file_path // .tool_input.notebook_path')"
        [ -n "$path" ] || exit 0
        payload="$(jq -n --arg path "$path" '{path: $path}' 2>/dev/null)" || exit 0
        zcie_send "file.changed" "$session" "$turn" "post-tool-use" "$payload"
        ;;
    Bash)
        command="$(zcie_field '.tool_input.command')"
        output="$(zcie_field '.tool_response.stdout' | cut -c1-4000)"
        payload="$(jq -n --arg command "$command" --arg output "$output" \
            '{command: $command, exit_code: 0, output: $output}' 2>/dev/null)" || exit 0
        zcie_send "command.run" "$session" "$turn" "post-tool-use" "$payload"
        add_playbook "$command"
        ;;
esac
exit 0
