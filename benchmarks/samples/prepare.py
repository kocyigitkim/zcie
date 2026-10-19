#!/usr/bin/env python3
"""Connects the three sample projects to ZCIE and writes the multi-project benchmark tasks.

Usage: prepare.py <samples dir> <tasks out.json>
Steps for each project: `zcie claude setup` (makes the project, writes .claude/zcie_id, adds the hooks),
commit the ID file, wait until the code is indexed, then capture the decisions of an earlier session.
These decisions are only in the ZCIE memory, not in the files.
The tasks use the real project IDs, so the "ask by ID" tasks work on each computer.
"""
import json
import os
import subprocess
import sys
import time

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
ZCIE = os.path.join(ROOT, "target", "release", "zcie")
PROJECTS = {
    "ledger-rs": ("apply_late_fee", [
        "Decision: ledger-rs keeps all balances in Postgres 16. We rejected MySQL because it has no transactional DDL.",
    ]),
    "atlas-ts": ("resolveRoute", [
        "We decided to drop Express and use Fastify 4 for the HTTP layer of atlas-ts.",
        "Decision: in atlas-ts Redis 7 is only a route cache; routes.json stays the source of truth.",
    ]),
    "orbit-py": ("schedule_orbit_burn", [
        "Decision: orbit-py replaced Celery with RQ because the Celery workers leaked memory during long burns.",
    ]),
}
# Path help for every condition. It goes to the system prompt, so the ZCIE hooks do not see it.
SIBLINGS = "The user's other projects are sibling folders of the current folder: ../ledger-rs, ../atlas-ts and ../orbit-py."


def zcie(*args):
    env = {k: v for k, v in os.environ.items() if k != "ZCIE_WORKSPACE"}
    env.setdefault("ZCIE_API_KEY_FILE", os.path.expanduser("~/.zcie/config/admin.key"))
    return subprocess.run([ZCIE, *args], check=True, capture_output=True, text=True, env=env).stdout


def connect(samples, name, symbol, decisions):
    root = os.path.join(samples, name)
    zcie("claude", "setup", "--dir", root, "--name", name)
    subprocess.run(["git", "-C", root, "add", ".claude/zcie_id"], check=True)
    subprocess.run(["git", "-C", root, "-c", "user.name=Sample Dev", "-c", "user.email=dev@example.com",
                    "commit", "-q", "-m", "chore: add the ZCIE project ID"], check=True)
    project_id = open(os.path.join(root, ".claude", "zcie_id")).read().strip()
    for _ in range(120):
        found = json.loads(zcie("--project", project_id, "query", f"where is {symbol} defined"))
        if any(r["title"].endswith(symbol) for r in found["results"]):
            break
        time.sleep(0.5)
    for turn, text in enumerate(decisions, start=1):
        zcie("--project", project_id, "capture", "--type", "user.message", "--session", f"seed-{name}",
             "--turn", str(turn), text)
    return root, project_id


def task(task_id, kind, cwd, prompt, expect):
    return {"id": task_id, "kind": kind, "cwd": cwd, "workspace": os.path.basename(cwd), "prompt": prompt,
            "system": SIBLINGS, "expect": expect}


def tasks(roots, ids):
    ledger, atlas, orbit = roots["ledger-rs"], roots["atlas-ts"], roots["orbit-py"]
    return [
        task("L1-ledger-fee", "local", ledger, "What is the late fee rate and which function applies it? Give the function name, the file path and the value.", ["apply_late_fee", "late_fee.rs", "1.75"]),
        task("L2-atlas-route", "local", atlas, "Where is resolveRoute defined? Give the file path only.", ["router/resolve.ts"]),
        task("L3-orbit-retries", "local", orbit, "How many retries does an orbit burn get? Give the constant name and the value.", ["MAX_BURN_RETRIES", "7"]),
        task("L4-ledger-history", "local", ledger, "Which commit changed the late fee rate, and from which value to which value?", ["1.5", "1.75"]),
        task("X1-name-atlas-ttl", "cross-name", ledger, "In the atlas-ts project, what is the route cache TTL in seconds and in which file is it set?", ["420", "cache/policy.ts"]),
        task("X2-id-orbit-fn", "cross-id", atlas, f"In project {ids['orbit-py']}, which function schedules an orbit burn? Give the function name and the file path.", ["schedule_orbit_burn", "scheduler.py"]),
        task("X3-name-ledger-pool", "cross-name", orbit, "In ledger-rs, what is the maximum number of database connections?", ["24"]),
        task("M1-name-ledger-db", "memory-name", orbit, "Which database did we decide to use for ledger-rs, which database did we reject, and why?", ["postgres", "transactional ddl"]),
        task("M2-id-orbit-celery", "memory-id", ledger, f"Why did project {ids['orbit-py']} replace Celery, and with what?", ["RQ", "leak"]),
        task("M3-name-atlas-http", "memory-name", orbit, "Which HTTP framework did we decide to use in atlas-ts instead of Express?", ["fastify"]),
        task("G1-all-currency", "global", atlas, "Which of my projects define a function that normalizes currency codes? Name the projects and the functions.", ["ledger-rs", "atlas-ts", "normalize"]),
        task("G2-all-redis", "global", ledger, "Which of my projects uses Redis, and for what?", ["atlas-ts", "cache"]),
    ]


def main():
    samples, out = os.path.abspath(sys.argv[1]), sys.argv[2]
    roots, ids = {}, {}
    for name, (symbol, decisions) in PROJECTS.items():
        roots[name], ids[name] = connect(samples, name, symbol, decisions)
        print(f"{name}: {ids[name]} {roots[name]}")
    with open(out, "w") as handle:
        json.dump(tasks(roots, ids), handle, indent=1)
    print(f"wrote {out}")


if __name__ == "__main__":
    main()
