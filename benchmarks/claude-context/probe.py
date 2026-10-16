#!/usr/bin/env python3
"""Measures the fixed context that Claude Code sends on each turn, per configuration.

Usage: probe.py <cwd> <out.jsonl>
Each variant runs a one-turn prompt two times; the second run shows the warm-cache state.
"""
import json
import os
import shutil
import subprocess
import sys

CWD, OUT = sys.argv[1], sys.argv[2]
CLAUDE = os.environ.get("CLAUDE_BIN") or shutil.which("claude") or "claude"
PROMPT = "Reply with only the word: ok"
LEAN_TOOLS = ["--tools", "Read,Grep,Glob,Bash,Edit,Write"]
VARIANTS = {
    "V0-default": [],
    "V1-no-mcp-connectors": ["--strict-mcp-config"],
    "V2-V1+no-skills": ["--strict-mcp-config", "--disable-slash-commands"],
    "V3-V2+lean-tools": ["--strict-mcp-config", "--disable-slash-commands"] + LEAN_TOOLS,
    "V4-V3+no-dynamic-sections": ["--strict-mcp-config", "--disable-slash-commands", "--exclude-dynamic-system-prompt-sections"] + LEAN_TOOLS,
    "V5-V4+no-user-settings": ["--strict-mcp-config", "--disable-slash-commands", "--exclude-dynamic-system-prompt-sections", "--setting-sources", ""] + LEAN_TOOLS,
    "V6-bare": ["--bare"],
    "V7-safe-mode": ["--safe-mode"],
    "V8-floor-custom-system-prompt": ["--strict-mcp-config", "--disable-slash-commands", "--system-prompt", "You are a coding assistant."] + LEAN_TOOLS,
}


def run(args):
    command = [CLAUDE, "-p", PROMPT, "--output-format", "json", "--max-turns", "1"] + args
    proc = subprocess.run(command, cwd=CWD, capture_output=True, text=True, stdin=subprocess.DEVNULL, timeout=300)
    try:
        data = json.loads(proc.stdout)
    except json.JSONDecodeError:
        return {"error": (proc.stdout + proc.stderr)[-300:]}
    usage = data.get("usage", {})
    context = usage.get("input_tokens", 0) + usage.get("cache_creation_input_tokens", 0) + usage.get("cache_read_input_tokens", 0)
    return {"context": context, "cache_read": usage.get("cache_read_input_tokens", 0),
            "cache_create": usage.get("cache_creation_input_tokens", 0), "cost_usd": data.get("total_cost_usd"),
            "result": (data.get("result") or "")[:40]}


with open(OUT, "w") as out:
    for name, args in VARIANTS.items():
        for attempt in (1, 2):
            record = {"variant": name, "attempt": attempt, **run(args)}
            out.write(json.dumps(record) + "\n")
            out.flush()
            print(record, flush=True)
