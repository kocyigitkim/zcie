#!/bin/sh
# Stop hook (end of each assistant turn): send assistant.message with the final answer text of the
# turn (`last_assistant_message`; tool calls and tool output are not in it). A new session can then
# find what the assistant explained, decided or did (ADR-016). Rule: never block; exit 0.
. "$(dirname "$0")/common.sh"
zcie_require_tools
zcie_read_input
session="$(zcie_field '.session_id')"
[ -n "$session" ] || exit 0
# The first 30,000 characters: a long answer is indexed as passages; a longer one is mostly code.
answer="$(zcie_field '.last_assistant_message | if type == "string" then .[0:30000] else empty end')"
[ -n "$answer" ] || exit 0
zcie_send "assistant.message" "$session" "$(zcie_current_turn "$session")" "stop" "$answer"
exit 0
