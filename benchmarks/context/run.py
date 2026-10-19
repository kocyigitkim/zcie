#!/usr/bin/env python3
"""Context-efficiency benchmark (ADR-017, D-055).

Usage:
  run.py run <tasks.json> <out.jsonl> <conditions> [reps] [parallel]
  run.py report <out.jsonl> [<out.jsonl> ...]
Task files may use {samples} and {tantivy} in "cwd"; the environment variables ZCIE_BENCH_SAMPLES and
ZCIE_BENCH_TANTIVY give the folders. Conditions are those of benchmarks/claude-ab/run_ab.py (U: the
installed ZCIE, A0: no hooks). The report gives, per condition: correct answers, the first Read on an
expected file, input tokens (input + cache create + cache read), output tokens, cost, turns and tool
calls by name.
"""
import json
import os
import sys

sys.path.insert(0, os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "claude-ab"))


def load_tasks(path):
    folders = {"samples": os.environ.get("ZCIE_BENCH_SAMPLES", ""), "tantivy": os.environ.get("ZCIE_BENCH_TANTIVY", "")}
    tasks = json.load(open(path))
    for task in tasks:
        task["cwd"] = task["cwd"].format(**folders)
    return tasks


def run(tasks_file, out_file, conditions, reps, parallel):
    os.environ["AB_CONDITIONS"] = conditions
    import run_ab  # reads AB_CONDITIONS at import
    resolved = os.path.join(run_ab.TMP, "tasks.json")
    json.dump(load_tasks(tasks_file), open(resolved, "w"))
    sys.argv = ["run_ab.py", resolved, out_file, str(reps), str(parallel)]
    run_ab.main()


def first_read_ok(record, tasks):
    expected = tasks.get(record["task"], [])
    path = record.get("first_read") or ""
    return any(e in path for e in expected)


def report(files):
    rows = [json.loads(line) for f in files for line in open(f) if line.strip()]
    expects = {}
    for f in files:
        meta = f + ".tasks.json"
        if os.path.exists(meta):
            expects.update({t["id"]: t["expect"] for t in json.load(open(meta))})
    by_condition = {}
    for row in rows:
        if "error" not in row:
            by_condition.setdefault(row["condition"], []).append(row)
    print("| condition | runs | correct | first read ok | input tokens/run | output/run | cost/run | turns/run | tool calls/run |")
    print("|---|---|---|---|---|---|---|---|---|")
    for condition, items in sorted(by_condition.items()):
        n = len(items)
        tokens = sum(r.get("input", 0) + r.get("cache_create", 0) + r.get("cache_read", 0) for r in items) / n
        calls = {}
        for r in items:
            for name, count in (r.get("tool_calls") or {}).items():
                calls[name] = calls.get(name, 0) + count
        calls_text = ", ".join(f"{k} {v / n:.2f}" for k, v in sorted(calls.items(), key=lambda kv: -kv[1]))
        first = sum(first_read_ok(r, expects) for r in items) if expects else "-"
        print(f"| {condition} | {n} | {sum(bool(r.get('correct')) for r in items)} | {first} | {tokens:,.0f} | {sum(r.get('output', 0) for r in items) / n:,.0f} | ${sum(r.get('cost_usd') or 0 for r in items) / n:.4f} | {sum(r.get('turns') or 0 for r in items) / n:.2f} | {calls_text} |")


def main():
    if sys.argv[1] == "run":
        tasks_file, out_file, conditions = sys.argv[2], sys.argv[3], sys.argv[4]
        reps = int(sys.argv[5]) if len(sys.argv) > 5 else 1
        parallel = int(sys.argv[6]) if len(sys.argv) > 6 else 4
        json.dump(load_tasks(tasks_file), open(out_file + ".tasks.json", "w"))
        run(tasks_file, out_file, conditions, reps, parallel)
    else:
        report(sys.argv[2:])


if __name__ == "__main__":
    main()
