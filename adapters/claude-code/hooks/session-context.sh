#!/bin/sh
# ZCIE session context for the Claude Code SessionStart hook. It prints short working rules and a
# compact catalog of the tools that the ZCIE profile disabled, once per session (stdout of
# SessionStart goes to the model context, and it is cached). Rule: never block; exit 0.

cat <<'TEXT'
ZCIE token rules for this session:
- "ZCIE context" blocks in prompts are verified facts from the local project index and earlier sessions. Use them before you search.
- Answer briefly. Cite file:line instead of repeating code. Do not restate the question or the plan.
- Read only the line ranges you need (Read with offset/limit). For large files, a ZCIE outline gives symbol line ranges.
TEXT

settings="${ZCIE_SETTINGS_FILE:-}"
command -v jq >/dev/null 2>&1 && [ -r "$settings" ] || exit 0
denied="$(jq -r '(.permissions.deny // [])[] | select(test("^[A-Za-z]+$"))' "$settings" 2>/dev/null)"
[ -n "$denied" ] || exit 0

describe() {
    case "$1" in
        WebFetch) echo "fetch a URL and read the page" ;;
        WebSearch) echo "search the web" ;;
        Agent|Task) echo "start a subagent with its own context" ;;
        TaskStop) echo "stop a background task" ;;
        Skill) echo "run an installed skill" ;;
        NotebookEdit) echo "edit Jupyter notebook cells" ;;
        LSP) echo "language-server lookups (definitions, references)" ;;
        CronCreate|CronDelete|CronList) echo "scheduled prompts" ;;
        ScheduleWakeup) echo "resume this session later" ;;
        Monitor) echo "watch a background condition" ;;
        Workflow) echo "multi-agent workflow scripts" ;;
        SendMessage|ListAgents) echo "message other agent sessions" ;;
        RemoteTrigger) echo "trigger remote agents" ;;
        PushNotification) echo "send a phone notification" ;;
        EnterWorktree|ExitWorktree) echo "switch to a git worktree" ;;
        DesignSync) echo "design sync" ;;
        ReportFindings) echo "report code-review findings" ;;
        *) echo "built-in tool" ;;
    esac
}

echo "Disabled tools (exist, but off to save tokens; you cannot enable them yourself). If a task needs one, tell the user the tool name; they enable it with \`zcie claude tools enable <Name>\`, or by naming the need in a message (for example a URL or \"search the web\"):"
for tool in $denied; do
    printf -- '- %s: %s\n' "$tool" "$(describe "$tool")"
done
exit 0
