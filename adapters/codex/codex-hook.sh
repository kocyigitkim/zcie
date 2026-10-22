#!/bin/sh
# ZCIE hook for Codex CLI (ADR-026). Usage: codex-hook.sh <script>. Codex hooks take Claude Code input,
# so it runs the Claude Code hook script with the ZCIE environment and gives plain context text the
# Codex JSON form. Rule: exit 0.
dir="$(dirname "$0")"
PATH="$PATH:/opt/homebrew/bin:/usr/local/bin"; export PATH
command -v jq >/dev/null 2>&1 || exit 0
export ZCIE_APP=codex
export ZCIE_API_KEY_FILE="${ZCIE_API_KEY_FILE:-$dir/../config/claude-code.key}"
[ -n "${ZCIE_URL:-}" ] || ZCIE_URL="$(jq -r '.url // empty' "$dir/../config/client.json" 2>/dev/null)"
[ -n "${ZCIE_INJECT_BUDGET:-}" ] || ZCIE_INJECT_BUDGET="$(jq -r '.budget // empty' "$dir/../config/client.json" 2>/dev/null)"
[ -n "$ZCIE_URL" ] && export ZCIE_URL
[ -n "$ZCIE_INJECT_BUDGET" ] && export ZCIE_INJECT_BUDGET
input="$(cat)"
out="$(printf '%s' "$input" | sh "$dir/$1" 2>/dev/null)"
[ -n "$out" ] || exit 0
event="$(printf '%s' "$input" | jq -r '.hook_event_name // empty')"
case "$event" in
    SessionStart|UserPromptSubmit)
        printf '%s' "$out" | jq -e . >/dev/null 2>&1 && { printf '%s\n' "$out"; exit 0; }
        jq -n --arg e "$event" --arg c "$out" '{hookSpecificOutput: {hookEventName: $e, additionalContext: $c}}' ;;
esac
exit 0
