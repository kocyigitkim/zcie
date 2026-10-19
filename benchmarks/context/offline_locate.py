#!/usr/bin/env python3
"""Localization check with no model (ADR-017): asks the daemon each task of locate.json and checks
that the expected file is the first file (and in the first 3) of the answer.

Usage: offline_locate.py <locate.json> [url]
ZCIE_BENCH_SAMPLES and ZCIE_BENCH_TANTIVY give the folders; ZCIE_API_KEY_FILE (default
~/.zcie/config/admin.key, or ZCIE_HOME/config/admin.key) gives the key.
"""
import json
import os
import re
import sys
import urllib.request

tasks_file = sys.argv[1]
url = sys.argv[2] if len(sys.argv) > 2 else "http://127.0.0.1:47831"
home = os.environ.get("ZCIE_HOME", os.path.expanduser("~/.zcie"))
key = open(os.environ.get("ZCIE_API_KEY_FILE", os.path.join(home, "config", "admin.key"))).read().split()[0]
folders = {"samples": os.environ.get("ZCIE_BENCH_SAMPLES", ""), "tantivy": os.environ.get("ZCIE_BENCH_TANTIVY", "")}
first = top3 = 0
tasks = json.load(open(tasks_file))
for task in tasks:
    cwd = task["cwd"].format(**folders)
    project = open(os.path.join(cwd, ".claude", "zcie_id")).read().strip()
    body = json.dumps({"workspace": project, "question": task["prompt"], "token_budget": 1200}).encode()
    request = urllib.request.Request(url + "/v1/query", data=body, method="POST")
    request.add_header("Content-Type", "application/json")
    request.add_header("Authorization", "Bearer " + key)
    response = json.loads(urllib.request.urlopen(request, timeout=30).read())
    answer = response.get("answer") or ""
    found = re.search(r"best first: (.*?)\. The symbols", answer)
    files = found.group(1).split(", ") if found else []
    expected = task["expect"] or task.get("expect_any", [])
    is_first = bool(files) and any(e in files[0] for e in expected)
    in_top3 = any(e in f for e in expected for f in files[:3])
    first += is_first
    top3 += in_top3
    print(f"{task['id']:18} first={'yes' if is_first else 'no ':3} top3={'yes' if in_top3 else 'no ':3} {files[:3]}")
print(f"first file correct: {first}/{len(tasks)}   in top 3: {top3}/{len(tasks)}")
