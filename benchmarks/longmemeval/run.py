#!/usr/bin/env python3
"""LongMemEval retrieval test (ADR-028, D-096).

Usage: run.py <longmemeval json> <out.jsonl> [limit] [top k, default 6]
For each question: a new project in a fresh ZCIE home gets the history sessions of the question
as user.message and assistant.message events. Then `/v1/query` asks the question. A hit means
that the answer text is in the top k results (title or snippet). This measures retrieval only:
no model reads the results. Questions whose answer is not a short text (abstention questions,
answers longer than 60 characters) are skipped.
"""
import collections
import json
import os
import shutil
import sys
import tempfile

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "isolated"))
import common  # noqa: E402

MAX_ANSWER_CHARS = 60


def usable(entry):
    answer = str(entry.get("answer", ""))
    return answer and len(answer) <= MAX_ANSWER_CHARS and not entry["question_id"].endswith("_abs")


def store_history(port, admin, workspace, entry):
    for s, (session_id, session) in enumerate(zip(entry["haystack_session_ids"], entry["haystack_sessions"])):
        for turn, message in enumerate(session, 1):
            kind = "user.message" if message["role"] == "user" else "assistant.message"
            event = {"schema": "zcie.event.v1", "idempotency_key": f"lme:{entry['question_id']}:{s}:{turn}", "workspace": workspace,
                     "source": {"app": "longmemeval", "session_id": str(session_id), "turn_id": str(turn)},
                     "type": kind, "payload": message["content"]}
            common.call(port, "POST", "/v1/events", event, admin)


def main():
    data = [e for e in json.load(open(sys.argv[1])) if usable(e)]
    out_path = sys.argv[2]
    limit = int(sys.argv[3]) if len(sys.argv) > 3 else len(data)
    top = int(sys.argv[4]) if len(sys.argv) > 4 else 6
    home = tempfile.mkdtemp(prefix="zcie-lme-")
    port = common.free_port()
    # The history has assistant turns; a new home refuses them unless capture is on.
    proc = common.start_daemon(home, port, extra_env={"ZCIE_CAPTURE_ASSISTANT_MESSAGES": "true"})
    hits = collections.Counter()
    totals = collections.Counter()
    try:
        admin = common.key(home)
        with open(out_path, "w") as out:
            for entry in data[:limit]:
                info = common.call(port, "POST", "/v1/projects", {"name": f"lme-{entry['question_id']}", "index": False}, admin)
                store_history(port, admin, info["project_id"], entry)
                common.wait_idle(port, admin)
                body = {"workspace": info["project_id"], "question": entry["question"], "token_budget": 2000, "max_results": top}
                response = common.call(port, "POST", "/v1/query", body, admin)
                text = " ".join((r.get("title") or "") + " " + (r.get("snippet") or "") for r in response.get("results", [])[:top])
                hit = str(entry["answer"]).lower() in text.lower()
                kind = entry.get("question_type", "unknown")
                totals[kind] += 1
                hits[kind] += hit
                out.write(json.dumps({"question_id": entry["question_id"], "type": kind, "hit": hit}) + "\n")
                print(entry["question_id"], kind, hit, flush=True)
    finally:
        common.stop_daemon(proc)
        shutil.rmtree(home, ignore_errors=True)
    print(f"\n| question type | questions | answer in top {top} |\n|---|---|---|")
    for kind in sorted(totals):
        print(f"| {kind} | {totals[kind]} | {hits[kind]} ({hits[kind] / totals[kind]:.0%}) |")
    total = sum(totals.values())
    if total:
        print(f"| all | {total} | {sum(hits.values())} ({sum(hits.values()) / total:.0%}) |")


if __name__ == "__main__":
    main()
