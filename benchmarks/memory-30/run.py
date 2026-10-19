#!/usr/bin/env python3
"""30-stage memory test: each stage is a NEW Claude Code session (no --resume, no shared context).

Usage: run.py <scenario.json> <project folder> <out.jsonl> <condition A|Z> <rep>
       run.py --regrade <scenario.json> <out.jsonl>
Teach stages tell a fact, a decision, a rule or a correction. Ask stages ask about earlier stages.
An ask stage is correct if the answer has each expected item (a string, or a list of alternatives)
and no forbidden string. Matching ignores case and Turkish dotted/dotless i differences.
"""
import json
import os
import re
import sys
import unicodedata

sys.path.insert(0, os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "claude-ab"))
import run_ab  # noqa: E402


def fold(text):
    """Lowercase, no accents, dotless i as i, and a decimal comma as a point (Turkish "2,25")."""
    text = unicodedata.normalize("NFKD", text.lower())
    text = "".join(c for c in text if not unicodedata.combining(c)).replace("ı", "i")
    return re.sub(r"(\d),(\d)", r"\1.\2", text)


def check(answer, stage):
    text = fold(answer)
    missing = []
    for item in stage.get("expect", []):
        options = item if isinstance(item, list) else [item]
        if not any(fold(o) in text for o in options):
            missing.append(options[0])
    found = [f for f in stage.get("forbid", []) if fold(f) in text]
    return not missing and not found, missing, found


def regrade(scenario, results):
    """Grades the answers of a finished run again (after a change of `check`)."""
    stages = {s["stage"]: s for s in json.load(open(scenario))}
    for line in open(results):
        record = json.loads(line)
        if record.get("kind") == "ask":
            ok, missing, found = check(record.get("result", ""), stages[record["stage"]])
            print(record["condition"], record["rep"], record["task"], ok, missing, found)


def main():
    if sys.argv[1] == "--regrade":
        return regrade(sys.argv[2], sys.argv[3])
    scenario, project, out, condition, rep = sys.argv[1], os.path.abspath(sys.argv[2]), sys.argv[3], sys.argv[4], int(sys.argv[5])
    for stage in json.load(open(scenario)):
        task = {"id": f"S{stage['stage']:02d}", "cwd": project, "workspace": os.path.basename(project), "prompt": stage["prompt"], "expect": []}
        record = run_ab.run_in(task, condition, rep, project)
        record.update(stage=stage["stage"], kind=stage["kind"])
        if stage["kind"] == "ask":
            ok, missing, found = check(record.get("result", ""), stage)
            record.update(correct=ok, missing=missing, forbidden_found=found)
        else:
            record.pop("correct", None)
        with open(out, "a") as handle:
            handle.write(json.dumps(record, ensure_ascii=False) + "\n")
        print(condition, rep, task["id"], stage["kind"], record.get("correct", "-"), record.get("cost_usd"), flush=True)


if __name__ == "__main__":
    main()
