#!/usr/bin/env python3
"""Runs the hooks of the 30-stage memory test without Claude: no model cost, the same hook order.

Usage: simulate.py <scenario.json> <project folder>
For each stage (a new session): SessionStart, UserPromptSubmit (capture), the context hook. For an
ask stage it checks that the capsule has each expected item. This checks what reaches the model,
not the answer of the model.
"""
import json
import os
import subprocess
import sys
import time
import unicodedata

HOOKS = os.path.expanduser("~/.zcie/hooks")


def fold(text):
    text = unicodedata.normalize("NFKD", text.lower())
    return "".join(c for c in text if not unicodedata.combining(c)).replace("ı", "i")


def code_text(project):
    out = []
    for folder, _, files in os.walk(project):
        if "/." in folder or "/gen" in folder:
            continue
        out += [open(os.path.join(folder, f), errors="ignore").read() for f in files if f.endswith((".rs", ".ts", ".py"))]
    return " ".join(out)


def hook(name, payload, env):
    return subprocess.run(["sh", os.path.join(HOOKS, name)], input=json.dumps(payload), capture_output=True, text=True, env=env).stdout


def main():
    scenario, project = json.load(open(sys.argv[1])), os.path.abspath(sys.argv[2])
    env = {k: v for k, v in os.environ.items() if k != "ZCIE_WORKSPACE"}
    env.setdefault("ZCIE_API_KEY_FILE", os.path.expanduser("~/.zcie/config/claude-code.key"))
    ok = asks = 0
    # Only facts are checked in the capsule: an expected item whose words occur in a teach stage or
    # in the code. Answer words that the model writes ("hayır", "no") are not in the capsule.
    corpus = fold(" ".join(s["prompt"] for s in scenario if s["kind"] == "teach") + " " + code_text(project))
    for stage in scenario:
        session = f"sim-{os.getpid()}-{stage['stage']}"
        base = {"session_id": session, "cwd": project}
        hook("session-start.sh", {**base, "hook_event_name": "SessionStart", "source": "startup"}, env)
        prompt = {**base, "prompt": stage["prompt"], "hook_event_name": "UserPromptSubmit"}
        capsule = hook("inject-context.sh", prompt, env)
        hook("user-prompt-submit.sh", prompt, env)
        if stage["kind"] == "ask":
            asks += 1
            text = fold(capsule)
            facts = [i if isinstance(i, list) else [i] for i in stage["expect"]]
            facts = [f for f in facts if any(fold(o) in corpus for o in f)]
            missing = [f[0] for f in facts if not any(fold(o) in text for o in f)]
            ok += not missing
            print(f"S{stage['stage']:02d} ask   capsule~{len(capsule)//4:4} tokens missing={missing}")
        time.sleep(1.5)  # the capture is processed before the next session
    print(f"capsule has the answer: {ok}/{asks}")


main()
