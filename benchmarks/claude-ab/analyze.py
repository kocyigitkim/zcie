#!/usr/bin/env python3
"""Summarizes run_ab.py results as Markdown tables. Usage: analyze.py <results.jsonl>"""
import json
import statistics
import sys
from collections import defaultdict

rows = [json.loads(line) for line in open(sys.argv[1]) if line.strip()]
ok = [r for r in rows if "error" not in r]
groups = defaultdict(list)
for r in ok:
    r["total"] = r["input"] + r["cache_create"] + r["cache_read"] + r["output"]
    r["fresh"] = r["input"] + r["cache_create"]
    groups[(r["task"], r["condition"])].append(r)

def mean(values):
    return statistics.mean(values) if values else 0.0

def fmt(n):
    return f"{n:,.0f}"

tasks = sorted({r["task"] for r in ok}, key=lambda t: int(t.split("-")[0][1:]))
print("| Task | Cond | Total tokens | Fresh input (in + cache write) | Cache read | Output | Cost $ | Turns | Time s | Correct |")
print("|---|---|---:|---:|---:|---:|---:|---:|---:|---:|")
for task in tasks:
    for cond in ("A", "B", "C"):
        g = groups[(task, cond)]
        if not g:
            continue
        correct = sum(r["correct"] for r in g)
        print(f"| {task} | {cond} | {fmt(mean([r['total'] for r in g]))} | {fmt(mean([r['fresh'] for r in g]))} | "
              f"{fmt(mean([r['cache_read'] for r in g]))} | {fmt(mean([r['output'] for r in g]))} | "
              f"{mean([r['cost_usd'] or 0 for r in g]):.4f} | {mean([r['turns'] or 0 for r in g]):.1f} | "
              f"{mean([r['duration_ms'] or 0 for r in g]) / 1000:.1f} | {correct}/{len(g)} |")

print("\n| Task | Total tokens saved | Cost saved | Turns A → B | Time A → B s |")
print("|---|---:|---:|---|---|")
summary = {"A": defaultdict(float), "B": defaultdict(float)}
for task in tasks:
    a = groups[(task, "A")]
    b = groups[(task, "B")] or groups[(task, "C")]
    if not a or not b:
        continue
    ta, tb = mean([r["total"] for r in a]), mean([r["total"] for r in b])
    ca, cb = mean([r["cost_usd"] or 0 for r in a]), mean([r["cost_usd"] or 0 for r in b])
    print(f"| {task} | {100 * (ta - tb) / ta:.0f}% | {100 * (ca - cb) / ca:.0f}% | "
          f"{mean([r['turns'] for r in a]):.1f} → {mean([r['turns'] for r in b]):.1f} | "
          f"{mean([r['duration_ms'] for r in a]) / 1000:.1f} → {mean([r['duration_ms'] for r in b]) / 1000:.1f} |")

print("\n| Condition | Runs | Total tokens (sum) | Fresh input (sum) | Output (sum) | Cost $ (sum) | Mean turns | Mean time s | Correct |")
print("|---|---:|---:|---:|---:|---:|---:|---:|---:|")
for cond in ("A", "B", "C"):
    g = [r for r in ok if r["condition"] == cond]
    print(f"| {cond} | {len(g)} | {fmt(sum(r['total'] for r in g))} | {fmt(sum(r['fresh'] for r in g))} | "
          f"{fmt(sum(r['output'] for r in g))} | {sum(r['cost_usd'] or 0 for r in g):.3f} | "
          f"{mean([r['turns'] for r in g]):.1f} | {mean([r['duration_ms'] for r in g]) / 1000:.1f} | "
          f"{sum(r['correct'] for r in g)}/{len(g)} |")
errors = [r for r in rows if "error" in r]
if errors:
    print(f"\nErrors: {len(errors)}")
