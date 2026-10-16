#!/bin/sh
# ZCIE session guard for the Claude Code UserPromptSubmit hook.
# Every turn re-reads the whole conversation. When the session grows past a size, the guard
# shows the user one short message (not sent to the model): start a new session with /clear,
# because ZCIE keeps the project memory. Rule: never block; every path exits 0.

. "$(dirname "$0")/common.sh"

ZCIE_SESSION_WARN_TOKENS="${ZCIE_SESSION_WARN_TOKENS:-60000}"
ZCIE_SESSION_STEP_TOKENS="${ZCIE_SESSION_STEP_TOKENS:-60000}"

command -v jq >/dev/null 2>&1 || exit 0
zcie_read_input
transcript="$(zcie_field '.transcript_path')"
session="$(zcie_field '.session_id')"
[ -f "$transcript" ] && [ -n "$session" ] || exit 0

# Transcript JSON has overhead; about 6 bytes per model token is a conservative estimate.
tokens=$(( $(wc -c < "$transcript" | tr -d ' ') / 6 ))
[ "$tokens" -ge "$ZCIE_SESSION_WARN_TOKENS" ] || exit 0
level=$(( (tokens - ZCIE_SESSION_WARN_TOKENS) / ZCIE_SESSION_STEP_TOKENS + 1 ))
marker="${TMPDIR:-/tmp}/zcie-guard-${session}.level"
last="$(cat "$marker" 2>/dev/null || printf '0')"
[ "$level" -gt "$last" ] || exit 0
printf '%s' "$level" > "$marker"

jq -n --arg k "$(( tokens / 1000 ))" '{
  systemMessage: ("ZCIE: this session is about " + $k + "k tokens and every turn reads it again. If the topic changed, start a new session with /clear. The project memory stays in ZCIE.")
}'
exit 0
