#!/bin/sh
# ZCIE tool relock for the Claude Code SessionEnd hook: denies again the tools that
# tool-unlock.sh enabled in this session. Rule: never block; every path exits 0.

. "$(dirname "$0")/common.sh"

command -v jq >/dev/null 2>&1 || exit 0
settings="${ZCIE_SETTINGS_FILE:-}"
[ -n "$settings" ] && [ -w "$settings" ] || exit 0
zcie_read_input
session="$(zcie_field '.session_id')"
record="${TMPDIR:-/tmp}/zcie-unlocked-${session}"
[ -f "$record" ] || exit 0
tools="$(jq -s 'add | unique' "$record" 2>/dev/null)"
rm -f "$record"
[ -n "$tools" ] || exit 0
tmp="$settings.zcie-tmp"
jq --argjson t "$tools" '.permissions.deny = (((.permissions.deny // []) + $t) | unique)' "$settings" > "$tmp" 2>/dev/null \
    && mv "$tmp" "$settings" || rm -f "$tmp"
exit 0
