#!/usr/bin/env python3
"""Isolated, paired benchmark (ADR-019, D-067).

Usage: run.py <frozen home> <out.jsonl> <conditions> <reps> <parallel> <seed> <tasks.json> [<tasks.json> ...]
Conditions:
  A0   no hooks, default tools               A0P  no hooks, ZCIE tool profile `auto`
  U    ZCIE hooks, default tools             UP   ZCIE hooks and profile `auto`
Each U/UP trial runs in a copy-on-write clone of the frozen home (`cp -c`) with its own daemon on
its own port; the clone is deleted after the trial, so no trial changes another one. Claude Code
gets only the settings of the condition (`--setting-sources project` and `--settings <file>`);
the hooks of the user are not loaded. The model is fixed (`AB_MODEL`, default `sonnet`).
For each task and repetition the conditions run in an order that the seed selects.
Task files may use {samples} and {tantivy} in "cwd" (ZCIE_BENCH_SAMPLES, ZCIE_BENCH_TANTIVY).
"""
import concurrent.futures
import json
import os
import random
import shutil
import subprocess
import sys
import tempfile
import time

import common

sys.path.insert(0, os.path.join(common.ROOT, "benchmarks", "claude-ab"))
import run_ab  # noqa: E402  (parse_stream, the tool set of the earlier benchmarks)

MODEL = os.environ.get("AB_MODEL", "sonnet")
CLAUDE = run_ab.CLAUDE
PROFILE_DENY = ["Skill", "Agent", "Workflow", "ScheduleWakeup", "ReportFindings", "ListAgents"]
# The hooks of `zcie claude setup` (capture, inject and guards; no tool unlock in a benchmark).
HOOKS = [
    ("SessionStart", None, "session-start.sh"), ("UserPromptSubmit", None, "user-prompt-submit.sh"),
    ("PostToolUse", "Edit|Write|MultiEdit|NotebookEdit|Bash", "post-tool-use.sh"), ("SessionEnd", None, "stop.sh"),
    ("Stop", None, "assistant-message.sh"), ("UserPromptSubmit", None, "inject-context.sh"),
    ("SessionStart", "startup", "session-map.sh"), ("SessionStart", "startup", "session-context.sh"),
    ("UserPromptSubmit", None, "session-guard.sh"), ("PreToolUse", "Read", "read-guard.sh"),
    ("PostToolUse", "Edit|Write|MultiEdit|NotebookEdit|Bash", "verify-gate.sh"), ("Stop", None, "verify-gate.sh"),
]
# Time limits of `zcie claude setup` (apps/zcie/src/commands/claude/hooks.rs).
TIMEOUTS = {"verify-gate.sh": 150, "inject-context.sh": 10, "session-map.sh": 10, "read-guard.sh": 10}


def load_tasks(paths):
    folders = {"samples": os.environ.get("ZCIE_BENCH_SAMPLES", ""), "tantivy": os.environ.get("ZCIE_BENCH_TANTIVY", "")}
    tasks = []
    for path in paths:
        group = os.path.splitext(os.path.basename(path))[0]
        for task in json.load(open(path)):
            task["cwd"] = task["cwd"].format(**folders)
            task["group"] = "edit" if task.get("copy") else group
            tasks.append(task)
    return tasks


def settings(condition, port, home, workspace, folder):
    out = {"env": {}}
    if condition.endswith("P"):
        out["permissions"] = {"deny": PROFILE_DENY}
        out["env"]["ENABLE_TOOL_SEARCH"] = "true"
    if condition.startswith("U"):
        out["env"].update({"ZCIE_URL": f"http://127.0.0.1:{port}", "ZCIE_API_KEY_FILE": os.path.join(home, "config", "claude-code.key"),
                           "ZCIE_WORKSPACE": workspace, "ZCIE_INJECT_BUDGET": "1200"})
        hooks = {}
        for event, matcher, script in HOOKS:
            group = {"hooks": [{"type": "command", "command": os.path.join(common.HOOKS, script), "timeout": TIMEOUTS.get(script, 5)}]}
            if matcher:
                group["matcher"] = matcher
            hooks.setdefault(event, []).append(group)
        out["hooks"] = hooks
    else:
        out["disableAllHooks"] = True
    path = os.path.join(folder, "settings.json")
    json.dump(out, open(path, "w"))
    return path


def command(task, settings_file):
    tools = run_ab.TOOLS + (" Edit Write" if task.get("copy") else "")
    deny = run_ab.DENY.replace("Edit Write ", "") if task.get("copy") else run_ab.DENY
    out = [CLAUDE, "-p", task["prompt"], "--output-format", "stream-json", "--verbose", "--max-turns", "30",
           "--model", MODEL, "--setting-sources", "project", "--settings", settings_file,
           "--strict-mcp-config", "--mcp-config", '{"mcpServers": {}}', "--allowedTools", tools, "--disallowedTools", deny]
    if task.get("system"):
        out += ["--append-system-prompt", task["system"]]
    return out


def claude(args, cwd):
    """Runs one session. Tries again while the claude binary is replaced by an update."""
    env = dict(common.clean_env(), DISABLE_AUTOUPDATER="1")
    for _ in range(10):
        try:
            return subprocess.run(args, cwd=cwd, capture_output=True, text=True, stdin=subprocess.DEVNULL, timeout=900, env=env)
        except FileNotFoundError:
            time.sleep(10)
    raise RuntimeError("claude binary not found")


def done_jobs(out_file):
    """The (task, condition, rep) of the records without error in an earlier run of the same file."""
    if not os.path.exists(out_file):
        return set()
    rows = [json.loads(line) for line in open(out_file) if line.strip()]
    return {(r["task"], r["condition"], r["rep"]) for r in rows if "error" not in r}


def trial(job, frozen, manifest):
    task, condition, rep, order = job
    folder = tempfile.mkdtemp(prefix="zcie-iso-")
    cwd, proc = task["cwd"], None
    try:
        home, port = os.path.join(folder, "home"), common.free_port()
        workspace = manifest["projects"].get(task["workspace"], {}).get("id", task["workspace"])
        if condition.startswith("U"):
            subprocess.run(["cp", "-c", "-R", frozen, home], check=True)
            proc = common.start_daemon(home, port)
        if task.get("copy"):
            cwd = os.path.join(folder, "work")
            subprocess.run(["cp", "-c", "-R", task["cwd"], cwd], check=True)
        started = time.time()
        result = claude(command(task, settings(condition, port, home, workspace, folder)), cwd)
        record = {"task": task["id"], "group": task["group"], "condition": condition, "rep": rep, "order": order, "wall_s": round(time.time() - started, 1)}
        record.update(outcome(task, result, cwd))
        return record
    finally:
        if proc:
            common.stop_daemon(proc)
        shutil.rmtree(folder, ignore_errors=True)


def outcome(task, result, cwd):
    data, tools = run_ab.parse_stream(result.stdout)
    if data is None:
        return {"error": (result.stdout + result.stderr)[-500:]}
    usage, text = data.get("usage", {}), data.get("result", "") or ""
    out = dict(tools, input=usage.get("input_tokens", 0), cache_create=usage.get("cache_creation_input_tokens", 0),
               cache_read=usage.get("cache_read_input_tokens", 0), output=usage.get("output_tokens", 0),
               cost_usd=data.get("total_cost_usd"), turns=data.get("num_turns"), is_error=data.get("is_error"), result=text[:2000])
    out["correct"] = all(e.lower() in text.lower() for e in task.get("expect", []))
    # expect_any: one of several correct answers (a task with two valid files).
    if task.get("expect_any"):
        out["correct"] = out["correct"] and any(e.lower() in text.lower() for e in task["expect_any"])
    if task.get("check"):
        out["correct"] = subprocess.run(task["check"], shell=True, cwd=cwd).returncode == 0
    return out


def jobs(tasks, conditions, reps, seed):
    out = []
    for rep in range(reps):
        for task in tasks:
            order = list(conditions)
            random.Random(f"{seed}:{task['id']}:{rep}").shuffle(order)
            out.extend((task, condition, rep, position) for position, condition in enumerate(order))
    return out


def main():
    frozen, out_file, conditions = os.path.abspath(sys.argv[1]), sys.argv[2], sys.argv[3].split(",")
    reps, parallel, seed = int(sys.argv[4]), int(sys.argv[5]), sys.argv[6]
    manifest = json.load(open(os.path.join(frozen, "manifest.json")))
    before = common.tree_hash(frozen)
    if before != manifest["tree_hash"]:
        sys.exit("the frozen home differs from its manifest; make it again with snapshot.py")
    claude_version = subprocess.run([CLAUDE, "--version"], capture_output=True, text=True).stdout.strip()
    os.makedirs(os.path.dirname(os.path.abspath(out_file)), exist_ok=True)
    meta = {"seed": seed, "model": MODEL, "claude": claude_version, "zcie": manifest["zcie"], "store": manifest["tree_hash"]}
    with open(out_file, "a") as out, concurrent.futures.ThreadPoolExecutor(parallel) as pool:
        done = done_jobs(out_file)
        todo = [j for j in jobs(load_tasks(sys.argv[7:]), conditions, reps, seed) if (j[0]["id"], j[1], j[2]) not in done]
        for record in pool.map(lambda job: trial(job, frozen, manifest), todo):
            out.write(json.dumps(dict(record, **meta)) + "\n")
            out.flush()
            print(record["task"], record["condition"], record.get("correct"), record.get("error", "")[:80], flush=True)
    if common.tree_hash(frozen) != before:
        sys.exit("the frozen home changed during the run")
    print("frozen home unchanged:", before)


if __name__ == "__main__":
    main()
