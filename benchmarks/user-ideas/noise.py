#!/usr/bin/env python3
"""Counts everyday prompts that get a ZCIE capsule (they should get none).

Usage: noise.py <everyday.json> <project folder>
"""
import json
import os
import subprocess
import sys

env = {k: v for k, v in os.environ.items() if k != "ZCIE_WORKSPACE"}
env.setdefault("ZCIE_API_KEY_FILE", os.path.expanduser("~/.zcie/config/claude-code.key"))
prompts, project = json.load(open(sys.argv[1])), os.path.abspath(sys.argv[2])
hits = 0
for i, prompt in enumerate(prompts):
    payload = json.dumps({"session_id": f"noise-{os.getpid()}-{i}", "cwd": project, "prompt": prompt})
    out = subprocess.run(["sh", os.path.expanduser("~/.zcie/hooks/inject-context.sh")], input=payload, capture_output=True, text=True, env=env).stdout
    hits += bool(out.strip())
    print(f"{'CAPSULE' if out.strip() else 'none   '} ~{len(out) // 4:4} tokens  {prompt[:60]}")
print(f"capsules: {hits}/{len(prompts)}")
