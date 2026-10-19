#!/usr/bin/env python3
"""Makes the frozen ZCIE home of the isolated benchmark (ADR-019, D-067).

Usage: snapshot.py <out home> <samples dir> <tantivy dir> [model folder]
A fresh daemon indexes the benchmark projects with the project IDs of their `.claude/zcie_id`
files (the tasks name some projects by ID) and stores the seed decisions below. Then it stops.
The home is never changed again: each trial runs in a copy-on-write clone of it.
`manifest.json` has the projects, the ZCIE version and the tree hash of the home.
"""
import json
import os
import shutil
import subprocess
import sys

import common

# Decisions of earlier sessions that the memory tasks ask about. They are only in the memory,
# not in the files of the projects.
SEED = {
    "ledger-rs": ["Decision: ledger-rs keeps all balances in Postgres 16. We rejected MySQL because it has no transactional DDL."],
    "atlas-ts": [
        "We decided to drop Express and use Fastify 4 for the HTTP layer of atlas-ts.",
        "Decision: in atlas-ts Redis 7 is only a route cache; routes.json stays the source of truth.",
    ],
    "orbit-py": ["Decision: orbit-py replaced Celery with RQ because the Celery workers leaked memory during long burns."],
    "tantivy": [
        "We decided to keep `LogMergePolicy` but raise `min_num_segments` to 8, because the default triggered too many small merges in our ingestion tests.",
        "Never call IndexWriter::commit from the query path; it blocks readers during search. Commit only from the background writer thread.",
        "BM25 parametrelerini değiştirmemeye karar verdik: K1=1.2 ve B=0.75 olduğu gibi kalıyor (`tantivy/src/query/bm25.rs:9`).",
        "We decided to raise DEFAULT_LEVEL_LOG_SIZE in the log merge policy from 0.75 to 1.5, because our segments were merged too often.",
    ],
}


def project_id(root):
    with open(os.path.join(root, ".claude", "zcie_id")) as handle:
        return handle.read().strip()


def seed(port, admin, name, workspace, decisions):
    for turn, text in enumerate(decisions, 1):
        event = {"schema": "zcie.event.v1", "idempotency_key": f"seed:{name}:{turn}", "workspace": workspace,
                 "source": {"app": "seed", "session_id": f"seed-{name}", "turn_id": str(turn)},
                 "type": "user.message", "payload": text}
        common.call(port, "POST", "/v1/events", event, admin)


def hook_key(home):
    """Makes the read/write key of the hooks (`zcie setup` makes it in a real installation)."""
    zcie = os.path.join(common.ROOT, "target", "release", "zcie")
    out = subprocess.run([zcie, "key", "create", "--home", home, "--name", "claude-code"], capture_output=True, text=True,
                         check=True, env=common.clean_env()).stdout
    key = next(line.strip() for line in out.splitlines() if line.strip() and " " not in line.strip())
    path = os.path.join(home, "config", "claude-code.key")
    with open(path, "w") as handle:
        handle.write(key + "\n")
    os.chmod(path, 0o600)


def main():
    home, samples, tantivy = (os.path.abspath(a) for a in sys.argv[1:4])
    model = sys.argv[4] if len(sys.argv) > 4 else os.path.expanduser("~/.zcie/models/potion-multilingual-128M")
    shutil.rmtree(home, ignore_errors=True)
    os.makedirs(os.path.join(home, "models"))
    os.symlink(os.path.abspath(model), os.path.join(home, "models", os.path.basename(model)))
    roots = {name: os.path.join(samples, name) for name in ("ledger-rs", "atlas-ts", "orbit-py")}
    roots["tantivy"] = tantivy
    port = common.free_port()
    proc = common.start_daemon(home, port)
    try:
        admin = common.key(home)
        projects = {}
        for name, root in roots.items():
            info = common.call(port, "POST", "/v1/projects", {"project_id": project_id(root), "name": name, "root": root, "index": True}, admin)
            projects[name] = {"id": info["project_id"], "root": root}
        common.wait_idle(port, admin)
        for name, decisions in SEED.items():
            seed(port, admin, name, projects[name]["id"], decisions)
        common.wait_idle(port, admin)
    finally:
        common.stop_daemon(proc)
    hook_key(home)
    version = subprocess.run([common.ZCIED, "--version"], capture_output=True, text=True).stdout.strip()
    manifest = {"projects": projects, "zcie": version, "model": os.path.basename(model), "tree_hash": common.tree_hash(home)}
    with open(os.path.join(home, "manifest.json"), "w") as handle:
        json.dump(manifest, handle, indent=1)
    print(json.dumps(manifest, indent=1))


if __name__ == "__main__":
    main()
