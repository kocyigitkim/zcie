#!/bin/sh
# Stop hook: send session.end.
. "$(dirname "$0")/common.sh"
zcie_require_tools
zcie_read_input
session="$(zcie_field '.session_id')"
[ -n "$session" ] || exit 0
zcie_send "session.end" "$session" "$(zcie_current_turn "$session")" "stop" ""
exit 0
