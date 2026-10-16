#!/bin/sh
# Shared helpers for the ZCIE Claude Code hooks.
# Rule: a hook never blocks or fails the user session. Every path ends with exit 0.

ZCIE_URL="${ZCIE_URL:-http://127.0.0.1:47831}"

# Key: ZCIE_API_KEY, or the first line of the file in ZCIE_API_KEY_FILE.
if [ -z "${ZCIE_API_KEY:-}" ] && [ -n "${ZCIE_API_KEY_FILE:-}" ] && [ -r "$ZCIE_API_KEY_FILE" ]; then
    ZCIE_API_KEY="$(head -n 1 "$ZCIE_API_KEY_FILE" | tr -d '[:space:]')"
fi

# Read the hook input JSON from stdin once.
zcie_read_input() {
    ZCIE_INPUT="$(cat)"
    export ZCIE_INPUT
}

# Get one field from the hook input with a jq filter.
zcie_field() {
    printf '%s' "$ZCIE_INPUT" | jq -r "$1 // empty" 2>/dev/null
}

# Stop quietly if a required tool is missing.
zcie_require_tools() {
    command -v jq >/dev/null 2>&1 || exit 0
    command -v curl >/dev/null 2>&1 || exit 0
}

# Finds the project ID: ZCIE_WORKSPACE (override), or .claude/zcie_id in the hook cwd or a parent
# folder (ADR-009). Prints nothing if no project is found.
zcie_project_id() {
    if [ -n "${ZCIE_WORKSPACE:-}" ]; then
        printf '%s' "$ZCIE_WORKSPACE"
        return
    fi
    dir="$(zcie_field '.cwd')"
    [ -n "$dir" ] || dir="$(pwd)"
    while [ -n "$dir" ] && [ "$dir" != "/" ]; do
        if [ -f "$dir/.claude/zcie_id" ]; then
            head -n 1 "$dir/.claude/zcie_id" | tr -d '[:space:]'
            return
        fi
        dir="$(dirname "$dir")"
    done
}

# Kept for older hook scripts: the project of the hook.
zcie_workspace() {
    zcie_project_id
}

# Makes the project of the hook cwd if it has none: asks the daemon for an ID and writes
# <root>/.claude/zcie_id (root = Git root or cwd). Not in $HOME or /. Prints the ID.
zcie_ensure_project() {
    id="$(zcie_project_id)"
    if [ -n "$id" ]; then
        printf '%s' "$id"
        return
    fi
    cwd="$(zcie_field '.cwd')"
    [ -n "$cwd" ] || return 0
    root="$(git -C "$cwd" rev-parse --show-toplevel 2>/dev/null || printf '%s' "$cwd")"
    [ "$root" != "$HOME" ] && [ "$root" != "/" ] || return 0
    body="$(jq -n --arg n "$(basename "$root")" --arg r "$root" '{name: $n, root: $r}' 2>/dev/null)" || return 0
    id="$(curl --max-time 3 --silent -H "Content-Type: application/json" \
        -H "Authorization: Bearer ${ZCIE_API_KEY:-}" -X POST --data "$body" "${ZCIE_URL}/v1/projects" 2>/dev/null \
        | jq -r '.project_id // empty' 2>/dev/null)"
    [ -n "$id" ] || return 0
    mkdir -p "$root/.claude" 2>/dev/null && printf '%s\n' "$id" > "$root/.claude/zcie_id"
    printf '%s' "$id"
}

# Path of the turn counter file of a session.
zcie_turn_file() {
    printf '%s/zcie-%s.turn' "${TMPDIR:-/tmp}" "$1"
}

# Print the current turn number of a session (0 if none).
zcie_current_turn() {
    file="$(zcie_turn_file "$1")"
    if [ -f "$file" ]; then cat "$file"; else printf '0'; fi
}

# Increment and print the turn number of a session.
zcie_next_turn() {
    next=$(( $(zcie_current_turn "$1") + 1 ))
    printf '%s' "$next" > "$(zcie_turn_file "$1")"
    printf '%s' "$next"
}

# SHA-256 of the hook input, first 16 hex characters.
zcie_input_sha() {
    printf '%s' "$ZCIE_INPUT" | shasum -a 256 2>/dev/null | cut -c1-16
}

# Send one event. Arguments: type session turn hook payload
zcie_send() {
    event_type="$1"; session="$2"; turn="$3"; hook="$4"; payload="$5"
    [ -n "$(zcie_project_id)" ] || return 0
    key="claude-code:${session}:${turn}:${hook}:$(zcie_input_sha)"
    body="$(jq -n \
        --arg schema "zcie.event.v1" \
        --arg key "$key" \
        --arg workspace "$(zcie_project_id)" \
        --arg session "$session" \
        --arg turn "$turn" \
        --arg type "$event_type" \
        --arg payload "$payload" \
        '{schema: $schema, idempotency_key: $key, workspace: $workspace,
          source: {app: "claude-code", session_id: $session, turn_id: $turn},
          type: $type, payload: $payload}' 2>/dev/null)" || return 0
    curl --max-time 2 --silent --output /dev/null \
        -H "Content-Type: application/json" \
        -H "Authorization: Bearer ${ZCIE_API_KEY:-}" \
        -X POST --data "$body" "${ZCIE_URL}/v1/events" >/dev/null 2>&1 || true
}
