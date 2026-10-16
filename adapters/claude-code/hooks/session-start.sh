#!/bin/sh
# SessionStart hook: send session.start.
. "$(dirname "$0")/common.sh"
zcie_require_tools
zcie_read_input
session="$(zcie_field '.session_id')"
[ -n "$session" ] || exit 0
zcie_send "session.start" "$session" "$(zcie_current_turn "$session")" "session-start" ""
exit 0
