#!/bin/sh
# ZCIE read guard for the Claude Code PreToolUse hook (matcher: Read|Bash).
# Read: a large file with no offset/limit is limited to a window of lines, and the model gets the
# symbol outline of the file from ZCIE, so it can read only the part it needs.
# Bash: a whole-file print of a large file (`cat <file>`, or `sed -n 'a,bp' <file>` with a range over
# the limit) is refused with the command for the window and the outline. The Bash input is not
# changed: auto mode refuses a changed Bash input. Any other command runs unchanged.
# Read of a large image (*.png|*.jpg|*.jpeg): a copy scaled down with sips (ZCIE_IMAGE_MAX_PX).
# The window starts 20 lines above the first range of the session working set in this file
# (the code that the capsule showed, ADR-028); with no such range it starts at line 1.
# Rule: never block the session; on any problem the Read or Bash runs unchanged. Every path exits 0.

. "$(dirname "$0")/common.sh"

ZCIE_READ_MAX_LINES="${ZCIE_READ_MAX_LINES:-400}"
ZCIE_READ_HEAD_LINES="${ZCIE_READ_HEAD_LINES:-250}"
ZCIE_IMAGE_MAX_PX="${ZCIE_IMAGE_MAX_PX:-1200}"

zcie_require_tools
zcie_read_input
tool="$(zcie_field '.tool_name')"
case "$tool" in Read|Bash) ;; *) exit 0 ;; esac
cwd="$(zcie_field '.cwd')"
reread=""
from=""
to=""
prefix=""

# Prints the symbol outline of a file (path relative to the project root) from the ZCIE index.
outline_of() {
    body="$(jq -n --arg w "$(zcie_project_id)" --arg q "$1" \
        '{workspace: $w, question: $q, mode: "search", token_budget: 2000, max_results: 40}' 2>/dev/null)"
    printf '%s' "$body" | curl --max-time 2 --silent -H "Content-Type: application/json" \
        -H "Authorization: Bearer ${ZCIE_API_KEY:-}" -X POST --data-binary @- "${ZCIE_URL}/v1/query" 2>/dev/null \
        | jq -r --arg rel "$1" '[.results[]? | select(.kind == "symbol" and .path == $rel and .range != null)]
        | sort_by(.range[0]) | .[:40][] | "  " + .title + " L" + (.range[0]|tostring) + "-" + (.range[1]|tostring)' 2>/dev/null
}

# Prints the first line of the read window of a file (arguments: file, line count) (ADR-028):
# 20 lines above the first working-set range of the file, else 1. The window stays inside the file.
window_of() {
    start="$(awk -F'\t' -v f="$1" '{ r = $1; p = r; sub(/:[0-9]+-[0-9]+$/, "", p); n = length(f) - length(p)
        if (n > 0 && substr(f, n) == "/" p) { sub(/^.*:/, "", r); sub(/-.*$/, "", r); print r; exit } }' \
        "$(zcie_work_file "$(zcie_field '.session_id')")" 2>/dev/null)"
    offset=1
    case "$start" in ''|*[!0-9]*) ;; *) [ "$start" -gt 20 ] && offset=$((start - 20)) ;; esac
    # Keep a full window near the end of the file.
    [ $((offset + ZCIE_READ_HEAD_LINES - 1)) -gt "$2" ] && offset=$(($2 - ZCIE_READ_HEAD_LINES + 1))
    [ "$offset" -lt 1 ] && offset=1
    printf '%s' "$offset"
}

# Reads a large image as a copy scaled to ZCIE_IMAGE_MAX_PX (sips, macOS) and ends the hook with it.
# Any failure returns, and the Read runs unchanged.
scale_image() {
    img="$1"
    [ "$ZCIE_IMAGE_MAX_PX" -gt 0 ] 2>/dev/null || return 0
    command -v sips >/dev/null 2>&1 || return 0
    dims="$(sips -g pixelWidth -g pixelHeight "$img" 2>/dev/null | awk '/pixelWidth/ { w = $2 } /pixelHeight/ { h = $2 } END { print w, h }')"
    w="${dims%% *}"
    h="${dims#* }"
    case "$w" in ''|*[!0-9]*) return 0 ;; esac
    case "$h" in ''|*[!0-9]*) return 0 ;; esac
    big="$w"
    [ "$h" -gt "$w" ] && big="$h"
    [ "$ZCIE_IMAGE_MAX_PX" -lt "$big" ] || return 0
    # The copy is reused: its name has the path and the modification time of the image.
    key="$(printf '%s' "$img$(stat -c %Y "$img" 2>/dev/null || stat -f %m "$img" 2>/dev/null)" | cksum | cut -d' ' -f1)"
    out="${TMPDIR:-/tmp}/zcie-img-$key.${img##*.}"
    [ -s "$out" ] || sips -Z "$ZCIE_IMAGE_MAX_PX" "$img" --out "$out" >/dev/null 2>&1 || return 0
    [ -s "$out" ] || return 0
    zcie_stat image_scaled '{}'
    note="ZCIE: the image was scaled to $ZCIE_IMAGE_MAX_PX px to save tokens (original ${w}x${h}: $img). Read the original only if you need fine detail."
    [ -n "$reread" ] && note="$reread $note"
    jq -n --argjson input "$(zcie_field '.tool_input')" --arg f "$out" --arg note "$note" '{
      hookSpecificOutput: {hookEventName: "PreToolUse", updatedInput: ($input + {file_path: $f}), additionalContext: $note}
    }' 2>/dev/null
    exit 0
}

# Prints only the repeated-read note (if any) and ends the hook.
finish() {
    [ -n "$reread" ] && jq -n --arg note "$reread" '{hookSpecificOutput: {hookEventName: "PreToolUse", additionalContext: $note}}' 2>/dev/null
    exit 0
}

if [ "$tool" = "Bash" ]; then
    cmd="$(zcie_field '.tool_input.command')"
    base="$cwd"
    # One leading `cd <dir> && ` is allowed; the dir has no spaces or shell characters.
    case "$cmd" in
        "cd "*" && "*)
            dir="${cmd#cd }"
            dir="${dir%% && *}"
            case "$dir" in ''|*[!A-Za-z0-9_./-]*) exit 0 ;; esac
            case "$dir" in /*) base="$dir" ;; *) base="$cwd/$dir" ;; esac
            prefix="cd $dir && "
            cmd="${cmd#"$prefix"}" ;;
    esac
    # Plain words only: no pipes, redirections, `;`, `&`, `$`, globs, quotes or newlines.
    # Single quotes around the sed range are allowed and removed here.
    plain="$(printf '%s' "$cmd" | tr -d "'")"
    [ "$(printf '%s' "$plain" | tr -dc 'A-Za-z0-9_./, -')" = "$plain" ] || exit 0
    set -f
    set -- $plain
    set +f
    case "$1" in
        cat) [ $# -eq 2 ] || exit 0
            arg="$2" ;;
        sed) [ $# -eq 4 ] && [ "$2" = "-n" ] || exit 0
            arg="$4"
            case "$3" in [0-9]*,[0-9]*p) ;; *) exit 0 ;; esac
            from="${3%%,*}"
            to="${3#*,}"
            to="${to%p}"
            case "$from" in ''|*[!0-9]*) exit 0 ;; esac
            case "$to" in ''|*[!0-9]*) exit 0 ;; esac
            [ "$from" -ge 1 ] && [ "$to" -ge "$from" ] || exit 0
            [ $((to - from + 1)) -gt "$ZCIE_READ_MAX_LINES" ] || exit 0 ;;
        *) exit 0 ;;
    esac
    case "$arg" in /*) file="$arg" ;; *) file="$base/$arg" ;; esac
    [ -f "$file" ] || exit 0
else
    file="$(zcie_field '.tool_input.file_path')"
    [ -f "$file" ] || exit 0

    # A repeated Read of the same range of an unchanged file in one session gets a note (ADR-029).
    # The Read still runs: after a context compaction it is correct.
    # A subagent has its own context: its reads are kept apart when the input names the agent.
    reads="${TMPDIR:-/tmp}/zcie-$(zcie_field '.session_id')$(zcie_field '.agent_id // empty' | sed 's/^/-/').reads"
    mtime="$(stat -c %Y "$file" 2>/dev/null || stat -f %m "$file" 2>/dev/null)"
    read_key="$(printf '%s\t%s\t%s\t%s' "$file" "$(zcie_field '.tool_input.offset')" "$(zcie_field '.tool_input.limit')" "$mtime")"
    if [ -n "$mtime" ] && [ -n "$(zcie_field '.session_id')" ]; then
        if grep -qxF "$read_key" "$reads" 2>/dev/null; then
            reread="ZCIE: you read this range before in this session and the file did not change."
            zcie_stat reread '{}'
        else
            printf '%s\n' "$read_key" >> "$reads" 2>/dev/null
        fi
    fi

    [ -z "$(zcie_field '.tool_input.offset')" ] || finish
    [ -z "$(zcie_field '.tool_input.limit')" ] || finish
    case "$file" in
        *.png|*.jpg|*.jpeg) scale_image "$file"; finish ;;
        # Images, PDFs and notebooks are not read as lines; a binary file has no line limit.
        *.gif|*.webp|*.bmp|*.ico|*.pdf|*.ipynb) finish ;;
    esac
fi

LC_ALL=C grep -Iq . "$file" 2>/dev/null || finish
lines="$(wc -l < "$file" 2>/dev/null | tr -d ' ')"
[ -n "$lines" ] && [ "$lines" -gt "$ZCIE_READ_MAX_LINES" ] || finish

rel="${file#"$cwd"/}"
if [ -n "$from" ]; then
    [ "$from" -le "$lines" ] || exit 0
    offset="$from"
else
    offset="$(window_of "$file" "$lines")"
fi
last=$((offset + ZCIE_READ_HEAD_LINES - 1))
[ "$last" -gt "$lines" ] && last="$lines"
[ -n "$to" ] && [ "$last" -gt "$to" ] && last="$to"
outline="$(outline_of "$rel")"

if [ "$tool" = "Bash" ]; then
    newcmd="${prefix}sed -n '$offset,${last}p' $arg"
    note="ZCIE read guard: $rel has $lines lines. Print only the part you need, for example: $newcmd"
else
    note="ZCIE read guard: $rel has $lines lines; only lines $offset-$last were read. Read only the part you need with offset/limit; the outline below gives the line ranges. With the zcie MCP server, one zcie_expand call reads several ranges, for example refs: [\"$rel:300-380\"]."
fi
if [ -n "$outline" ]; then
    note="$note Symbol outline (from the ZCIE index):
$outline"
fi
[ -n "$reread" ] && note="$reread $note"

if [ "$tool" = "Bash" ]; then
    zcie_stat read_guard "{\"lines\": $lines, \"read\": $((last - offset + 1)), \"offset\": $offset, \"tool\": \"bash\"}"
    jq -n --arg note "$note" '{hookSpecificOutput: {hookEventName: "PreToolUse", permissionDecision: "deny", permissionDecisionReason: $note}}' 2>/dev/null
    exit 0
fi
zcie_stat read_guard "{\"lines\": $lines, \"read\": $ZCIE_READ_HEAD_LINES, \"offset\": $offset}"
jq -n --argjson input "$(zcie_field '.tool_input' )" --argjson limit "$ZCIE_READ_HEAD_LINES" --argjson offset "$offset" --arg note "$note" '{
  hookSpecificOutput: {
    hookEventName: "PreToolUse",
    updatedInput: ($input + {limit: $limit} + (if $offset > 1 then {offset: $offset} else {} end)),
    additionalContext: $note
  }
}' 2>/dev/null
exit 0
