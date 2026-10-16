#!/bin/sh
# PostToolUse hook: send file.changed for edit tools and command.run for Bash.
. "$(dirname "$0")/common.sh"
zcie_require_tools
zcie_read_input
session="$(zcie_field '.session_id')"
tool="$(zcie_field '.tool_name')"
[ -n "$session" ] || exit 0
turn="$(zcie_current_turn "$session")"
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
            '{command: $command, exit_code: null, output: $output}' 2>/dev/null)" || exit 0
        zcie_send "command.run" "$session" "$turn" "post-tool-use" "$payload"
        ;;
esac
exit 0
