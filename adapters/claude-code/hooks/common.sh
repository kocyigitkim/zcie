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
# folder (ADR-009). The search stops at a Git repository root: a repository inside another
# project folder is its own project. Prints nothing if no project is found.
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
        [ -e "$dir/.git" ] && return
        dir="$(dirname "$dir")"
    done
}

# Prints the project root of the hook cwd (the rules of `zcie usage`, ADR-022): the folder with
# .claude/zcie_id, else the root of the main Git worktree, else the cwd.
zcie_project_root() {
    zcie_root_start="$(zcie_field '.cwd')"
    [ -n "$zcie_root_start" ] || return 0
    zcie_root_dir="$zcie_root_start"
    while [ -n "$zcie_root_dir" ] && [ "$zcie_root_dir" != "/" ]; do
        if [ -f "$zcie_root_dir/.claude/zcie_id" ]; then
            printf '%s' "$zcie_root_dir"
            return
        fi
        [ -e "$zcie_root_dir/.git" ] && break
        zcie_root_dir="$(dirname "$zcie_root_dir")"
    done
    zcie_root_git="$(git -C "$zcie_root_start" rev-parse --path-format=absolute --git-common-dir 2>/dev/null)"
    case "$zcie_root_git" in
        */.git) printf '%s' "${zcie_root_git%/.git}" ;;
        *) printf '%s' "$zcie_root_start" ;;
    esac
}

# Kept for older hook scripts: the project of the hook.
zcie_workspace() {
    zcie_project_id
}

# True when a folder is the root of a temporary folder ($TMPDIR, /tmp, /var/tmp and their /private forms).
zcie_is_temp_root() {
    folder="${1%/}"
    temp="${TMPDIR:-/tmp}"
    temp="${temp%/}"
    for t in "$temp" "/private$temp" "/tmp" "/private/tmp" "/var/tmp" "/private/var/tmp"; do
        [ "$folder" = "$t" ] && return 0
    done
    return 1
}

# Makes the project of the hook cwd if it has none: asks the daemon for an ID and writes
# <root>/.claude/zcie_id (root = Git root or cwd). Not in $HOME, /, or the root of a temporary
# folder (an ID file there would give every temporary folder below it the same project). Prints the ID.
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
    zcie_is_temp_root "$root" && return 0
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

# Print the path of the file with the refs that the context hook injected in a session.
zcie_seen_file() {
    printf '%s/zcie-%s.seen' "${TMPDIR:-/tmp}" "$1"
}

# Print the path of the file with the working set of a session: the source ranges of the last
# delivered evidence ("path:start-end", one for each line; ADR-019).
zcie_work_file() {
    printf '%s/zcie-%s.work' "${TMPDIR:-/tmp}" "$1"
}

# Print the time in milliseconds (for ZCIE_HOOK_TIMING_FILE), or nothing.
zcie_now_ms() {
    perl -MTime::HiRes=time -e 'printf "%d", time * 1000' 2>/dev/null
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

# Appends one statistics line to the hook statistics of ADR-022: ZCIE_STATS_FILE, else
# <zcie home>/stats/hooks.jsonl when the hook runs from an installed ZCIE home (it has a config
# folder). Arguments: kind, fields (a JSON object). The line has the time, the session, the folder,
# the kind and the fields; never file content. A failure never stops the hook.
zcie_stat() {
    zcie_stat_home="$(dirname "$0")/.."
    zcie_stat_file="${ZCIE_STATS_FILE:-}"
    [ -n "$zcie_stat_file" ] || { [ -d "$zcie_stat_home/config" ] && zcie_stat_file="$zcie_stat_home/stats/hooks.jsonl"; }
    [ -n "$zcie_stat_file" ] || return 0
    mkdir -p "$(dirname "$zcie_stat_file")" 2>/dev/null || return 0
    jq -nc --arg k "$1" --argjson f "$2" --arg s "$(zcie_field '.session_id')" --arg c "$(zcie_field '.cwd')" \
        '{ts: (now | todate), kind: $k, session: $s, cwd: $c} + $f' >> "$zcie_stat_file" 2>/dev/null || true
}

# Returns 0 if the prompt is the words of the user. Notifications and command output that the
# client puts in the prompt are not ("<task-notification>", "<bash-stdout>", "Caveat: ...").
zcie_is_user_text() {
    case "$1" in
        "<task-notification>"*|"<local-command"*|"<command-name>"*|"<command-message>"*|"Caveat:"*|"[Request interrupted"*|"<bash-"*) return 1 ;;
    esac
    return 0
}
