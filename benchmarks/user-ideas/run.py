#!/usr/bin/env python3
"""User-ideas test: each question about the ideas of the user is a NEW Claude Code session in the
ZCIEngine project, with the memory of all earlier conversations (imported with `zcie import claude`).

Usage: run.py <questions.json> <project folder> <out.jsonl> <rep> [A]
With A, the sessions run without ZCIE (no hooks): the baseline.
The sessions only READ the memory: the settings keep the context hooks and drop the capture hooks,
so that the test questions do not become memory of the project. Grading is the same as in
benchmarks/memory-30/run.py (each expected item, case and accent independent).
"""
import importlib.util
import json
import os
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(os.path.dirname(HERE), "claude-ab"))
import run_ab  # noqa: E402

spec = importlib.util.spec_from_file_location("memory30", os.path.join(os.path.dirname(HERE), "memory-30", "run.py"))
memory30 = importlib.util.module_from_spec(spec)
spec.loader.exec_module(memory30)

# Hooks that only read: the context capsule and the session rules. Capture hooks are dropped.
READ_HOOKS = ("inject-context.sh", "session-context.sh")


def read_only_settings(project):
    settings = json.load(open(os.path.join(project, ".claude", "settings.local.json")))
    hooks = {}
    for event, groups in settings.get("hooks", {}).items():
        kept = [g for g in groups if all(h.get("command", "").endswith(READ_HOOKS) for h in g.get("hooks", []))]
        if kept:
            hooks[event] = kept
    settings["hooks"] = hooks
    handle, path = tempfile.mkstemp(prefix="zcie-ui-", suffix=".json")
    with os.fdopen(handle, "w") as out:
        json.dump(settings, out)
    return path


def main():
    questions, project, out, rep = sys.argv[1], os.path.abspath(sys.argv[2]), sys.argv[3], int(sys.argv[4])
    condition = sys.argv[5] if len(sys.argv) > 5 else "R"
    settings = read_only_settings(project)
    for question in json.load(open(questions)):
        task = {"id": f"Q{question['stage']:02d}", "cwd": project, "workspace": os.path.basename(project), "prompt": question["prompt"], "expect": [], "settings": settings}
        record = run_ab.run_in(task, condition, rep, project)
        ok, missing, found = memory30.check(record.get("result", ""), question)
        record.update(stage=question["stage"], kind="ask", correct=ok, missing=missing, forbidden_found=found)
        with open(out, "a") as handle:
            handle.write(json.dumps(record, ensure_ascii=False) + "\n")
        print(condition, rep, task["id"], ok, missing, record.get("cost_usd"), flush=True)


if __name__ == "__main__":
    main()
