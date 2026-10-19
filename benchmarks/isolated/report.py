#!/usr/bin/env python3
"""Report of the isolated benchmark (ADR-019, D-067).

Usage: report.py <out.jsonl> [baseline condition, default A0]
For each task group (code, edit, locate, memory, …) and condition: runs, correct runs, mean input
tokens (input + cache creation + cache read), mean file-search calls (Read, Grep, Glob), mean turns.
For each condition other than the baseline: the mean paired difference of the tokens and of the
file-search calls to the baseline (same task, same repetition) with a 95 % bootstrap interval.
Unlike groups are never combined.
"""
import collections
import json
import random
import statistics
import sys

SEARCH = ("Read", "Grep", "Glob")


def tokens(r):
    return r.get("input", 0) + r.get("cache_create", 0) + r.get("cache_read", 0)


def searches(r):
    return sum((r.get("tool_calls") or {}).get(name, 0) for name in SEARCH)


def interval(values, rounds=2000):
    if len(values) < 2:
        return (float("nan"), float("nan"))
    rng = random.Random(7)
    means = sorted(statistics.mean(rng.choices(values, k=len(values))) for _ in range(rounds))
    return means[int(0.025 * rounds)], means[int(0.975 * rounds) - 1]


def main():
    rows = [json.loads(line) for line in open(sys.argv[1]) if line.strip()]
    rows = [r for r in rows if "error" not in r]
    base = sys.argv[2] if len(sys.argv) > 2 else "A0"
    groups = collections.defaultdict(list)
    for r in rows:
        groups[r["group"]].append(r)
    for group, items in sorted(groups.items()):
        print(f"\n## {group}\n")
        print("| condition | runs | correct | input tokens/run | tokens per correct run | file-search calls/run | turns/run |")
        print("|---|---|---|---|---|---|---|")
        by_condition = collections.defaultdict(list)
        for r in items:
            by_condition[r["condition"]].append(r)
        for condition, runs in sorted(by_condition.items()):
            correct = sum(bool(r.get('correct')) for r in runs)
            per_success = sum(map(tokens, runs)) / correct if correct else float("nan")
            print(f"| {condition} | {len(runs)} | {correct} | {statistics.mean(map(tokens, runs)):,.0f} | {per_success:,.0f} "
                  f"| {statistics.mean(map(searches, runs)):.2f} | {statistics.mean(r.get('turns') or 0 for r in runs):.2f} |")
        pairs = {(r["task"], r["rep"]): r for r in by_condition.get(base, [])}
        for condition, runs in sorted(by_condition.items()):
            if condition == base:
                continue
            matched = [(r, pairs[(r["task"], r["rep"])]) for r in runs if (r["task"], r["rep"]) in pairs]
            if not matched:
                continue
            token_diff = [tokens(r) - tokens(b) for r, b in matched]
            search_diff = [searches(r) - searches(b) for r, b in matched]
            base_mean = statistics.mean(tokens(b) for _, b in matched)
            low, high = interval(token_diff)
            slow, shigh = interval(search_diff)
            print(f"\n{condition} − {base} ({len(matched)} pairs): tokens {statistics.mean(token_diff):+,.0f} "
                  f"({statistics.mean(token_diff) / base_mean:+.1%}; 95 % interval {low:+,.0f} to {high:+,.0f}); "
                  f"file-search calls {statistics.mean(search_diff):+.2f} (95 % interval {slow:+.2f} to {shigh:+.2f})")


if __name__ == "__main__":
    main()
