#!/usr/bin/env python3
"""A/B token benchmark: Claude Code with and without ZCIE (docs/13-test-plan.md section 7, extension).

Usage: run_ab.py <tasks.json> <out.jsonl> [repetitions] [parallel]
Each task: {"id", "cwd", "workspace", "prompt", "expect": [substrings]}.
Condition A: tools Read/Grep/Glob/Bash, no MCP.
Condition B: same tools plus the zcie MCP server.
Condition C: same tools as A plus the ZCIE context-injection hook (UserPromptSubmit), no MCP.
Condition L: same as A plus the lean tool set (benchmarks/claude-context/lean-settings.json).
Condition LZ: L plus the ZCIE context-injection hook.
Condition Z: the settings that `zcie claude setup` wrote to <cwd>/.claude/settings.local.json.
Condition Z5: Z plus FORCE_PROMPT_CACHING_5M=1 (5-minute cache writes).
Condition D: the ZCIE hooks of Z, no deny list, ENABLE_TOOL_SEARCH=true (all tools known; most by name only).
Condition H: the ZCIE hooks of Z, ENABLE_TOOL_SEARCH=true and only 6 non-deferrable heavy tools denied
(the model loads the other tools itself through ToolSearch).
A task with "copy": true runs in a fresh copy of its cwd; "check" is a shell command that must
succeed in that copy (used for editing tasks; Edit and Write are allowed there).
Conditions come from the environment variable AB_CONDITIONS (default "A,B").
"""
import concurrent.futures
import json
import os
import shutil
import subprocess
import sys
import tempfile
import time

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
MCP_BIN = os.path.join(ROOT, "target", "release", "zcie-mcp")
KEY_FILE = os.path.expanduser("~/.zcie/config/admin.key")
TOOLS = "Read Grep Glob LS Bash"
DENY = "Edit Write MultiEdit NotebookEdit Agent Task WebSearch WebFetch"
HINT = ("This project is indexed by ZCIE. Use the zcie_query and zcie_expand MCP tools first to find code, symbols, "
        "tasks and history. Read files only when the zcie evidence is not enough.")
TMP = tempfile.mkdtemp(prefix="zcie-ab-")
CLAUDE = os.environ.get("CLAUDE_BIN") or shutil.which("claude") or "claude"
CONDITIONS = os.environ.get("AB_CONDITIONS", "A,B").split(",")
HOOK = os.path.join(ROOT, "adapters", "claude-code", "hooks", "inject-context.sh")


LEAN = os.path.join(ROOT, "benchmarks", "claude-context", "lean-settings.json")


def hook_settings(workspace, lean=False):
    path = os.path.join(TMP, f"settings-{workspace}-{'lean' if lean else 'full'}.json")
    hook = {"type": "command", "command": HOOK, "timeout": 5}
    settings = {
        "env": {"ZCIE_WORKSPACE": workspace, "ZCIE_API_KEY_FILE": KEY_FILE},
        "hooks": {"UserPromptSubmit": [{"hooks": [hook]}]},
    }
    if lean:
        settings["permissions"] = json.load(open(LEAN))["permissions"]
    with open(path, "w") as handle:
        json.dump(settings, handle)
    return path


def mcp_config(workspace):
    path = os.path.join(TMP, f"mcp-{workspace}.json")
    if workspace is None:
        config = {"mcpServers": {}}
        path = os.path.join(TMP, "mcp-empty.json")
    else:
        env = {"ZCIE_URL": "http://127.0.0.1:47831", "ZCIE_API_KEY_FILE": KEY_FILE, "ZCIE_WORKSPACE": workspace}
        config = {"mcpServers": {"zcie": {"type": "stdio", "command": MCP_BIN, "args": [], "env": env}}}
    with open(path, "w") as handle:
        json.dump(config, handle)
    return path


def defer_settings(lean_file):
    """Makes the D settings from the Z settings: no deny list, deferred tool loading on."""
    settings = json.load(open(lean_file))
    settings.pop("permissions", None)
    settings.setdefault("env", {})["ENABLE_TOOL_SEARCH"] = "true"
    path = os.path.join(TMP, "settings-defer-" + str(abs(hash(lean_file))) + ".json")
    with open(path, "w") as handle:
        json.dump(settings, handle)
    return path


HYBRID_DENY = ["Skill", "Agent", "Workflow", "ScheduleWakeup", "ReportFindings", "ListAgents"]


def hybrid_settings(lean_file):
    """Makes the H settings from the Z settings: small deny list, deferred tool loading on."""
    settings = json.load(open(lean_file))
    settings["permissions"] = {"deny": HYBRID_DENY}
    settings.setdefault("env", {})["ENABLE_TOOL_SEARCH"] = "true"
    path = os.path.join(TMP, "settings-hybrid-" + str(abs(hash(lean_file))) + ".json")
    with open(path, "w") as handle:
        json.dump(settings, handle)
    return path


def run(task, condition, rep):
    cwd = task["cwd"]
    if task.get("copy"):
        cwd = tempfile.mkdtemp(prefix="zcie-ab-edit-")
        subprocess.run(["cp", "-R", task["cwd"] + "/.", cwd], check=True)
    record = run_in(task, condition, rep, cwd)
    if task.get("check") and "error" not in record:
        record["correct"] = subprocess.run(task["check"], shell=True, cwd=cwd).returncode == 0
    if task.get("copy"):
        shutil.rmtree(cwd, ignore_errors=True)
    return record


def run_in(task, condition, rep, cwd):
    with_zcie = condition == "B"
    tools = TOOLS + (" Edit Write" if task.get("copy") else "")
    deny = DENY.replace("Edit Write ", "") if task.get("copy") else DENY
    command = [
        CLAUDE, "-p", task["prompt"], "--output-format", "json", "--max-turns", "30",
        "--setting-sources", "user", "--strict-mcp-config",
        "--mcp-config", mcp_config(task["workspace"] if with_zcie else None),
        "--allowedTools", tools + (" mcp__zcie__zcie_query mcp__zcie__zcie_expand" if with_zcie else ""),
        "--disallowedTools", deny,
    ]
    if with_zcie:
        command += ["--append-system-prompt", HINT]
    if condition == "C":
        command += ["--settings", hook_settings(task["workspace"])]
    if condition == "L":
        command += ["--settings", LEAN]
    if condition == "LZ":
        command += ["--settings", hook_settings(task["workspace"], lean=True)]
    if condition in ("Z", "Z5"):
        command += ["--settings", os.path.join(cwd, ".claude", "settings.local.json")]
    if condition == "H":
        command += ["--settings", hybrid_settings(os.path.join(cwd, ".claude", "settings.local.json"))]
    if condition == "D":
        command += ["--settings", defer_settings(os.path.join(cwd, ".claude", "settings.local.json"))]
    started = time.time()
    env = dict(os.environ, DISABLE_AUTOUPDATER="1")
    if condition == "Z5":
        env["FORCE_PROMPT_CACHING_5M"] = "1"
    proc = None
    for _ in range(5):
        try:
            proc = subprocess.run(command, cwd=cwd, capture_output=True, text=True, stdin=subprocess.DEVNULL, timeout=900, env=env)
            break
        except FileNotFoundError:
            time.sleep(5)  # the claude binary is replaced during an update
    if proc is None:
        return {"task": task["id"], "condition": condition, "rep": rep, "error": "claude binary not found"}
    record = {"task": task["id"], "condition": condition, "rep": rep, "wall_s": round(time.time() - started, 1)}
    try:
        data = json.loads(proc.stdout)
    except json.JSONDecodeError:
        record.update(error=(proc.stdout + proc.stderr)[-500:])
        return record
    usage = data.get("usage", {})
    result = data.get("result", "") or ""
    record.update(
        input=usage.get("input_tokens", 0),
        cache_create=usage.get("cache_creation_input_tokens", 0),
        cache_read=usage.get("cache_read_input_tokens", 0),
        output=usage.get("output_tokens", 0),
        cost_usd=data.get("total_cost_usd"),
        turns=data.get("num_turns"),
        duration_ms=data.get("duration_ms"),
        correct=all(e.lower() in result.lower() for e in task["expect"]),
        is_error=data.get("is_error"),
        result=result[:400],
    )
    return record


def main():
    tasks_file, out_file = sys.argv[1], sys.argv[2]
    reps = int(sys.argv[3]) if len(sys.argv) > 3 else 1
    parallel = int(sys.argv[4]) if len(sys.argv) > 4 else 4
    tasks = json.load(open(tasks_file))
    done = set()
    if os.path.exists(out_file):
        for line in open(out_file):
            record = json.loads(line)
            if "error" not in record:
                done.add((record["task"], record["condition"], record["rep"]))
    jobs = [(t, c, r) for r in range(reps) for t in tasks for c in CONDITIONS if (t["id"], c, r) not in done]
    with open(out_file, "a") as out, concurrent.futures.ThreadPoolExecutor(parallel) as pool:
        for record in pool.map(lambda j: run(*j), jobs):
            out.write(json.dumps(record) + "\n")
            out.flush()
            print(record["task"], record["condition"], record.get("cost_usd"), record.get("correct"), flush=True)


if __name__ == "__main__":
    main()
