#!/bin/sh
# ZCIE session guard for the Claude Code UserPromptSubmit hook.
# Every turn re-reads the whole conversation. When the session grows past a size, the guard
# shows the user one short message (not sent to the model). When a new topic starts in a long
# session, the guard also writes the handoff note (ADR-033), so a new session can start from it.
# Rule: never block; every path exits 0.

. "$(dirname "$0")/common.sh"

ZCIE_SESSION_WARN_TOKENS="${ZCIE_SESSION_WARN_TOKENS:-60000}"
ZCIE_SESSION_STEP_TOKENS="${ZCIE_SESSION_STEP_TOKENS:-60000}"
ZCIE_HANDOFF_TOKENS="${ZCIE_HANDOFF_TOKENS:-150000}"

command -v jq >/dev/null 2>&1 || exit 0
zcie_read_input
transcript="$(zcie_field '.transcript_path')"
session="$(zcie_field '.session_id')"
[ -f "$transcript" ] && [ -n "$session" ] || exit 0

tokens="$(zcie_context_tokens "$transcript")"
[ "$tokens" -ge "$ZCIE_SESSION_WARN_TOKENS" ] || exit 0

# New topic: the prompt (4+ words) names none of the files or ranges of this session. Each
# matching word counts once; stop words and words under 4 letters do not count.
prompt="$(zcie_field '.prompt')"
if [ "$tokens" -ge "$ZCIE_HANDOFF_TOKENS" ] && zcie_is_user_text "$prompt" \
    && [ "$(printf '%s' "$prompt" | wc -w | tr -d ' ')" -ge 4 ]; then
    cwd="$(zcie_field '.cwd')"
    known="$(zcie_changed_paths "$transcript" "$cwd"; cut -f1 "$(zcie_work_file "$session")" 2>/dev/null)"
    overlap="$(jq -rn --arg p "$prompt" --arg f "$known" '
        ["about", "after", "again", "also", "because", "could", "does", "from", "have", "into", "just", "make", "need", "please", "should", "that", "their", "them", "then", "there", "they", "this", "what", "when", "where", "which", "with", "would", "your"] as $stop
        | ($f | ascii_downcase | [splits("[^a-z0-9]+")]) as $known
        | [($p | ascii_downcase | [scan("[a-z]{4,}")] | unique)[] | select(IN($stop[]) | not) | select(IN($known[]))] | length' 2>/dev/null)"
    topic_marker="${TMPDIR:-/tmp}/zcie-guard-${session}.topic"
    last_topic="$(cat "$topic_marker" 2>/dev/null || printf '0')"
    topic_level=$(( (tokens - ZCIE_HANDOFF_TOKENS) / 50000 + 1 ))
    if [ "$overlap" = 0 ] && [ "$topic_level" -gt "$last_topic" ]; then
        printf '%s' "$topic_level" > "$topic_marker"
        # Detached (ADR-033): the hook returns at once and the note is written in the background.
        handoff_in="${TMPDIR:-/tmp}/zcie-${session}.handoff-in"
        printf '%s' "$ZCIE_INPUT" > "$handoff_in"
        ( ZCIE_HANDOFF_WRITE=1 sh "$(dirname "$0")/session-handoff.sh" < "$handoff_in" >/dev/null 2>&1; rm -f "$handoff_in" ) >/dev/null 2>&1 &
        jq -n --arg k "$(( tokens / 1000 ))" '{
          systemMessage: ("ZCIE: this looks like a new topic and the session is " + $k + "k tokens, read again on every turn. A new session (/clear) costs less: ZCIE saved where you are and gives it to the new session.")
        }'
        exit 0
    fi
fi

level=$(( (tokens - ZCIE_SESSION_WARN_TOKENS) / ZCIE_SESSION_STEP_TOKENS + 1 ))
marker="${TMPDIR:-/tmp}/zcie-guard-${session}.level"
last="$(cat "$marker" 2>/dev/null || printf '0')"
[ "$level" -gt "$last" ] || exit 0
printf '%s' "$level" > "$marker"

jq -n --arg k "$(( tokens / 1000 ))" '{
  systemMessage: ("ZCIE: this session is about " + $k + "k tokens and every turn reads it again. If the topic changed, start a new session with /clear. The project memory stays in ZCIE.")
}'
exit 0
