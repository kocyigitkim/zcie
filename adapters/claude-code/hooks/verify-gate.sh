#!/bin/sh
# ZCIE verification gate (ADR-020, D-081). PostToolUse (edit tools): record the changed file.
# Stop: if the turn changed code, run the check of the project. A failed check blocks the stop with
# the errors, so that Claude fixes them in the same turn (at most ZCIE_CHECK_MAX_BLOCKS times).
# A pass, a timeout, a missing tool or no check command never blocks. Every path exits 0.

. "$(dirname "$0")/common.sh"

ZCIE_CHECK_TIMEOUT="${ZCIE_CHECK_TIMEOUT:-120}"
ZCIE_CHECK_MAX_BLOCKS="${ZCIE_CHECK_MAX_BLOCKS:-3}"
ZCIE_CHECK_LINES="${ZCIE_CHECK_LINES:-60}"
# Hooks run in a shell with no login profile: add the usual folders of the check programs.
PATH="$PATH:$HOME/.cargo/bin:/opt/homebrew/bin:/usr/local/bin"
export PATH

command -v jq >/dev/null 2>&1 || exit 0
zcie_read_input
session="$(zcie_field '.session_id')"
[ -n "$session" ] || exit 0
changed_file="${TMPDIR:-/tmp}/zcie-$session.changed"
blocks_file="${TMPDIR:-/tmp}/zcie-$session.blocks"

case "$(zcie_field '.hook_event_name')" in
    PostToolUse)
        # A Bash command can change files too (sed -i, mv): the Stop step then asks Git.
        if [ "$(zcie_field '.tool_name')" = "Bash" ]; then
            printf '@bash\n' >> "$changed_file" 2>/dev/null
            exit 0
        fi
        path="$(zcie_field '.tool_input.file_path // .tool_input.notebook_path')"
        [ -n "$path" ] && printf '%s\n' "$path" >> "$changed_file" 2>/dev/null
        exit 0
        ;;
    Stop) ;;
    *) exit 0 ;;
esac
[ -s "$changed_file" ] || exit 0

cwd="$(zcie_field '.cwd')"
[ -n "$cwd" ] || cwd="$(pwd)"
root="$(git -C "$cwd" rev-parse --show-toplevel 2>/dev/null || printf '%s' "$cwd")"

# After a Bash command: the changed and new files of the Git working tree are checked too.
if grep -q '^@bash$' "$changed_file" 2>/dev/null && git -C "$root" rev-parse >/dev/null 2>&1; then
    { git -C "$root" diff --name-only HEAD 2>/dev/null; git -C "$root" ls-files --others --exclude-standard 2>/dev/null; } \
        | while IFS= read -r rel; do printf '%s/%s\n' "$root" "$rel"; done >> "$changed_file"
fi

# Prints the folder of the nearest file $2 above the file $1, not above the root $3 (a crate or a
# TypeScript project of a monorepo has its own Cargo.toml or tsconfig.json).
nearest() {
    dir="$(dirname "$1")"
    while [ -n "$dir" ] && [ "$dir" != "/" ]; do
        if [ -f "$dir/$2" ]; then
            printf '%s' "$dir"
            return
        fi
        [ "$dir" = "$3" ] && return
        dir="$(dirname "$dir")"
    done
}

# Runs one check command in a folder with the time limit. perl alarm: macOS has no `timeout`.
run_check() {
    (cd "$1" && perl -e 'alarm shift @ARGV; exec @ARGV' "$ZCIE_CHECK_TIMEOUT" sh -c "$2" 2>&1)
}

# Prints "kind|target" for each check: one for each folder with a Cargo.toml or a tsconfig.json
# above a changed file, one for each changed Python file.
targets() {
    sort -u "$changed_file" | while IFS= read -r file; do
        case "$file" in
            *.rs) dir="$(nearest "$file" Cargo.toml "$root")"; [ -n "$dir" ] && printf 'cargo|%s\n' "$dir" ;;
            *.ts|*.tsx) dir="$(nearest "$file" tsconfig.json "$root")"; [ -n "$dir" ] && printf 'tsc|%s\n' "$dir" ;;
            *.py) [ -f "$file" ] && printf 'py|%s\n' "$file" ;;
        esac
    done | sort -u
}

output=""
status=0
check=""
missing=""
if [ -f "$root/.claude/zcie_verify" ]; then
    # The command of the project (one line), run in the root.
    check="$(head -n 1 "$root/.claude/zcie_verify")"
    output="$(run_check "$root" "$check")"
    code=$?
    # A timeout or a missing program (126 or more) is not an error of the code.
    [ "$code" -ne 0 ] && [ "$code" -lt 126 ] && status=1
    [ "$code" -eq 127 ] && missing="$check"
else
    list="$(targets)"
    if [ -z "$list" ]; then
        rm -f "$changed_file" "$blocks_file"
        exit 0
    fi
    for item in $(printf '%s\n' "$list" | tr ' ' '\001'); do
        item="$(printf '%s' "$item" | tr '\001' ' ')"
        kind="${item%%|*}"
        target="${item#*|}"
        case "$kind" in
            cargo) one="cargo check --quiet --message-format short"; part="$(run_check "$target" "$one")" ;;
            tsc) one="npx --no-install tsc --noEmit --pretty false"; part="$(run_check "$target" "$one")" ;;
            *) one="python3 -m py_compile"; part="$(python3 -m py_compile "$target" 2>&1)" ;;
        esac
        code=$?
        [ "$code" -eq 127 ] && missing="$one"
        if [ "$code" -ne 0 ] && [ "$code" -lt 126 ]; then
            status=1
            check="$one"
            output="$output$part
"
        fi
    done
fi

if [ "$status" -eq 0 ]; then
    if [ -n "$missing" ]; then zcie_stat gate '{"result": "skipped"}'; else zcie_stat gate '{"result": "pass"}'; fi
    rm -f "$changed_file" "$blocks_file"
    # The check could not run: say it, so that "no error" is not taken for "checked".
    [ -n "$missing" ] && jq -n --arg c "$missing" '{systemMessage: ("ZCIE check skipped: the program of \"" + $c + "\" was not found. The code changes of this turn were not checked.")}' 2>/dev/null
    exit 0
fi

blocks="$(cat "$blocks_file" 2>/dev/null || printf '0')"
if [ "$blocks" -ge "$ZCIE_CHECK_MAX_BLOCKS" ]; then
    zcie_stat gate '{"result": "fail"}'
    rm -f "$changed_file" "$blocks_file"
    jq -n --arg c "$check" '{systemMessage: ("ZCIE check (" + $c + ") still fails after the fixes of this turn. Review the errors before you use the change.")}' 2>/dev/null
    exit 0
fi
zcie_stat gate '{"result": "block"}'
printf '%s' "$((blocks + 1))" > "$blocks_file"
errors="$(printf '%s' "$output" | head -n "$ZCIE_CHECK_LINES")"
jq -n --arg c "$check" --arg e "$errors" \
    '{decision: "block", reason: ("ZCIE check failed (" + $c + ") after your code changes. Fix these errors, then finish the turn:\n" + $e)}' 2>/dev/null
exit 0
