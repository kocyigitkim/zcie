#!/bin/sh
# SessionStart hook: send session.start.
. "$(dirname "$0")/common.sh"
zcie_require_tools
zcie_read_input
session="$(zcie_field '.session_id')"
[ -n "$session" ] || exit 0
# The project root of the folder, for `zcie usage` (ADR-022): the menu bar app reads no project
# folder (macOS asks before a program reads Desktop, Documents or Downloads).
zcie_stat session "$(jq -nc --arg r "$(zcie_project_root)" --arg i "$(zcie_project_id)" '{root: $r, project_id: $i}' 2>/dev/null || printf '{}')"
# After a compaction or a clear the model lost the injected capsules: inject them again when needed.
case "$(zcie_field '.source')" in
    compact|clear) rm -f "$(zcie_seen_file "$session")" 2>/dev/null ;;
esac
# Without the daemon nothing is recorded. Say it once per session (SessionStart output goes to
# the model context), so that the gap in the memory is not silent.
if ! curl --max-time 1 --silent --fail "${ZCIE_URL}/v1/health" >/dev/null 2>&1; then
    printf '%s\n' "ZCIE: the local memory daemon does not run, so this session is not recorded. Tell the user once: run \`zcie daemon start\` (or \`zcie daemon install\` to start it at login)."
    exit 0
fi
# The only hook that makes a project (SessionStart runs first and alone; ADR-009).
zcie_ensure_project >/dev/null
zcie_send "session.start" "$session" "$(zcie_current_turn "$session")" "session-start" ""
exit 0
