#!/bin/sh
# ZCIE handoff (ADR-033). PreCompact writes a short extractive note of where the session is. A
# SessionStart after /clear or a compaction prints the note of the project once per session, so
# the next session starts where this one was. session-guard.sh writes the note too (ZCIE_HANDOFF_WRITE=1)
# when a new topic starts in a long session. Rule: never block; exit 0.

. "$(dirname "$0")/common.sh"
command -v jq >/dev/null 2>&1 || exit 0
zcie_read_input
session="$(zcie_field '.session_id')"
transcript="$(zcie_field '.transcript_path')"
cwd="$(zcie_field '.cwd')"
file="$(zcie_handoff_file)"
[ -n "$session" ] && [ -n "$file" ] || exit 0
event="$(zcie_field '.hook_event_name')"

if [ "${ZCIE_HANDOFF_WRITE:-}" = 1 ] || [ "$event" = PreCompact ]; then
    [ -f "$transcript" ] || exit 0
    mkdir -p "$(dirname "$file")" 2>/dev/null || exit 0
    {
        printf 'ZCIE handoff from the previous session (%s, %s). Continue from here; check files before you rely on them.\n' \
            "$(printf '%s' "$session" | cut -c1-8)" "$(date -u '+%Y-%m-%d %H:%M UTC')"
        echo
        echo "Last requests of the user:"
        # One line per prompt (newlines become spaces); the prompt filter comes after the cut.
        zcie_main_records "$transcript" | jq -r 'select(.type == "user") | (.message.content | if type == "string" then . else [.[]? | select(.type == "text") | .text] | join(" ") end) | gsub("\n"; " ") | .[0:200]' 2>/dev/null \
            | while IFS= read -r prompt; do
                [ -n "$prompt" ] || continue
                case "$prompt" in "[Image"*) continue ;; esac
                zcie_is_user_text "$prompt" && printf -- '- %s\n' "$prompt"
            done | tail -n 5
        echo
        echo "Files changed:"
        zcie_changed_files "$transcript" "$cwd" | sed 's/^/- /'
        echo
        echo "Code in focus:"
        cut -f1 "$(zcie_work_file "$session")" 2>/dev/null | head -n 15 | sed 's/^/- /'
        echo
        echo "Last answer:"
        zcie_main_records "$transcript" | jq -r 'select(.type == "assistant") | [.message.content[]? | select(.type == "text") | .text] | join(" ") | gsub("\n"; " ") | select(length > 0) | .[0:600]' 2>/dev/null | tail -n 1
    } > "$file.tmp" && mv "$file.tmp" "$file"
    exit 0
fi

# SessionStart after /clear or a compaction: print the note once per session (ADR-033).
[ "$event" = SessionStart ] || exit 0
source="$(zcie_field '.source')"
case "$source" in clear|compact) ;; *) exit 0 ;; esac
[ -s "$file" ] || exit 0
[ -n "$(find "$file" -mmin "-$(( ${ZCIE_HANDOFF_MAX_AGE_H:-48} * 60 ))" 2>/dev/null)" ] || exit 0
marker="${TMPDIR:-/tmp}/zcie-handoff-${session}.shown"
[ -e "$marker" ] && exit 0
# After a /clear, the note of this same session is not new: print only the note of another session.
prefix="($(printf '%s' "$session" | cut -c1-8),"
if [ "$source" = clear ]; then
    case "$(head -n 1 "$file")" in *"$prefix"*) exit 0 ;; esac
fi
: > "$marker" 2>/dev/null
cat "$file"
exit 0
