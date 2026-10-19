#!/bin/sh
# UserPromptSubmit hook: start a new turn and send user.message with the prompt text.
. "$(dirname "$0")/common.sh"
zcie_require_tools
zcie_read_input
session="$(zcie_field '.session_id')"
prompt="$(zcie_field '.prompt')"
[ -n "$session" ] && [ -n "$prompt" ] || exit 0
zcie_is_user_text "$prompt" || exit 0
turn="$(zcie_next_turn "$session")"
zcie_send "user.message" "$session" "$turn" "user-prompt-submit" "$prompt"
exit 0
