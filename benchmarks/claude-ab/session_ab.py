#!/usr/bin/env python3
"""Long-session A/C benchmark: all questions in one Claude Code session (--resume), context grows.

Usage: session_ab.py <tasks.json> <out.jsonl> <condition A|C|L|LZ|Z|D> <rep>
All tasks must share one cwd and workspace. Each line of the output is one question of the session.
"""
import json
import os
import subprocess
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import run_ab  # noqa: E402  (reuses CLAUDE, tool lists and hook settings)

TASKS, OUT, CONDITION, REP = sys.argv[1], sys.argv[2], sys.argv[3], int(sys.argv[4])


def ask(task, session_id):
    command = [
        run_ab.CLAUDE, "-p", task["prompt"], "--output-format", "json", "--max-turns", "30",
        "--setting-sources", "user", "--strict-mcp-config", "--mcp-config", run_ab.mcp_config(None),
        "--allowedTools", run_ab.TOOLS, "--disallowedTools", run_ab.DENY,
    ]
    if CONDITION == "C":
        command += ["--settings", run_ab.hook_settings(task["workspace"])]
    if CONDITION == "L":
        command += ["--settings", run_ab.LEAN]
    if CONDITION == "LZ":
        command += ["--settings", run_ab.hook_settings(task["workspace"], lean=True)]
    if CONDITION == "Z":
        command += ["--settings", os.path.join(task["cwd"], ".claude", "settings.local.json")]
    if CONDITION == "H":
        command += ["--settings", run_ab.hybrid_settings(os.path.join(task["cwd"], ".claude", "settings.local.json"))]
    if CONDITION == "D":
        command += ["--settings", run_ab.defer_settings(os.path.join(task["cwd"], ".claude", "settings.local.json"))]
    if session_id:
        command += ["--resume", session_id]
    env = dict(os.environ, DISABLE_AUTOUPDATER="1")
    proc = subprocess.run(command, cwd=task["cwd"], capture_output=True, text=True, stdin=subprocess.DEVNULL, timeout=900, env=env)
    return json.loads(proc.stdout)


def main():
    tasks = json.load(open(TASKS))
    session_id = None
    with open(OUT, "a") as out:
        for index, task in enumerate(tasks):
            data = ask(task, session_id)
            session_id = data.get("session_id", session_id)
            usage = data.get("usage", {})
            result = data.get("result", "") or ""
            record = {
                "condition": CONDITION, "rep": REP, "step": index + 1, "task": task["id"],
                "input": usage.get("input_tokens", 0), "cache_create": usage.get("cache_creation_input_tokens", 0),
                "cache_read": usage.get("cache_read_input_tokens", 0), "output": usage.get("output_tokens", 0),
                "cost_usd": data.get("total_cost_usd"), "turns": data.get("num_turns"),
                "correct": all(e.lower() in result.lower() for e in task["expect"]),
                "model_usage": data.get("modelUsage", {}),
            }
            out.write(json.dumps(record) + "\n")
            out.flush()
            print(CONDITION, REP, record["step"], record["task"], record["cost_usd"], record["correct"], flush=True)


main()
