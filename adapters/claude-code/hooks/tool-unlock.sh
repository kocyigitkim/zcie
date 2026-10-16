#!/bin/sh
# ZCIE tool unlock for the Claude Code UserPromptSubmit hook.
# The lean profile denies rarely used tools. When a prompt needs one (a URL, "search the web",
# "subagent", a notebook, a schedule), this hook removes the deny rule from the settings file in
# ZCIE_SETTINGS_FILE for this session. Claude Code reloads settings at once. The SessionEnd hook
# tool-relock.sh denies the tools again. Rule: never block; every path exits 0.

. "$(dirname "$0")/common.sh"

command -v jq >/dev/null 2>&1 || exit 0
settings="${ZCIE_SETTINGS_FILE:-}"
[ -n "$settings" ] && [ -w "$settings" ] || exit 0
zcie_read_input
prompt="$(zcie_field '.prompt' | tr '[:upper:]' '[:lower:]')"
session="$(zcie_field '.session_id')"
[ -n "$prompt" ] && [ -n "$session" ] || exit 0

tools=""
case "$prompt" in *http://*|*https://*|*internet*|*"web'de"*|*"webde"*|*"search the web"*|*"web search"*|*online*|*araştır*|*google*|*"look up"*) tools="$tools WebFetch WebSearch" ;; esac
case "$prompt" in *subagent*|*"alt ajan"*|*"sub-agent"*|*"in parallel"*|*paralel*) tools="$tools Task TaskStop" ;; esac
case "$prompt" in *notebook*|*.ipynb*|*jupyter*) tools="$tools NotebookEdit" ;; esac
case "$prompt" in *cron*|*zamanla*|*schedule*|*"her gün"*|*"every day"*) tools="$tools CronCreate CronList CronDelete" ;; esac
case "$prompt" in *skill*) tools="$tools Skill" ;; esac
[ -n "$tools" ] || exit 0

list="$(printf '%s\n' $tools | jq -R . | jq -s .)"
denied="$(jq -c --argjson t "$list" '[(.permissions.deny // [])[] | select(. as $d | $t | index($d))]' "$settings" 2>/dev/null)"
[ -n "$denied" ] && [ "$denied" != "[]" ] || exit 0
tmp="$settings.zcie-tmp"
jq --argjson t "$denied" '.permissions.deny -= $t' "$settings" > "$tmp" 2>/dev/null && mv "$tmp" "$settings" || { rm -f "$tmp"; exit 0; }
printf '%s\n' "$denied" >> "${TMPDIR:-/tmp}/zcie-unlocked-${session}"
jq -n --argjson t "$denied" '{systemMessage: ("ZCIE: enabled " + ($t | join(", ")) + " for this session.")}'
exit 0
