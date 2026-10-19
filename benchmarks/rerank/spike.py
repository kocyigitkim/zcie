#!/usr/bin/env python3
"""Reranker spike (ADR-017, D-064; decision in docs/adr/ADR-018-reranker.md).

Usage: spike.py <model dir> <set> <url> <key file> [workspace]
  <model dir>  a cross-encoder folder with tokenizer.json and onnx/<file>.onnx
               (cross-encoder/mmarco-mMiniLMv2-L12-H384-v1, file model_qint8_arm64.onnx)
  <set>        "vector": the labelled questions of benchmarks/vector/eval.py (each in its own project)
               "ideas":  the questions of benchmarks/user-ideas/questions.json (workspace argument)
The script asks the daemon for the top 30 results of each question, reranks them with the
cross-encoder (question, title + snippet), and prints hit@1, hit@5 and MRR before and after, and
the p50/p95 time of the reranking. It only reads: it sends queries, never events.
Needs a Python with onnxruntime, tokenizers and numpy.
"""
import ast
import glob
import json
import os
import sys
import time
import unicodedata
import urllib.request

import numpy as np
import onnxruntime as ort
from tokenizers import Tokenizer

HERE = os.path.dirname(os.path.abspath(__file__))
TOP = 30
MAX_TOKENS = 256
# Reciprocal rank fusion constant of the blended order (the search order and the reranker order).
RRF_K = 10


def vector_questions():
    """Reads QUESTIONS of benchmarks/vector/eval.py without running it (the file runs main())."""
    tree = ast.parse(open(os.path.join(HERE, "..", "vector", "eval.py")).read())
    for node in tree.body:
        if isinstance(node, ast.Assign) and getattr(node.targets[0], "id", "") == "QUESTIONS":
            return [(project, question, [expected]) for _, project, question, expected in ast.literal_eval(node.value)]
    raise RuntimeError("QUESTIONS not found")


def idea_questions(workspace):
    out = []
    for row in json.load(open(os.path.join(HERE, "..", "user-ideas", "questions.json"))):
        expect = [e if isinstance(e, list) else [e] for e in row["expect"]]
        out.append((workspace, row["prompt"], expect))
    return out


def fold(text):
    return "".join(c for c in unicodedata.normalize("NFKD", text.lower()) if not unicodedata.combining(c))


def hit(item, expected):
    """True if the item has the expected text. `expected`: a string, or groups (one of each group)."""
    text = fold(item["title"] + " " + (item.get("snippet") or ""))
    if all(isinstance(e, str) for e in expected):
        return all(fold(e) in text for e in expected)
    return any(fold(alt) in text for group in expected for alt in group)


def rank_of(items, expected):
    for index, item in enumerate(items[:10]):
        if hit(item, expected):
            return index + 1
    return None


class Reranker:
    def __init__(self, folder):
        self.tokenizer = Tokenizer.from_file(os.path.join(folder, "tokenizer.json"))
        self.tokenizer.enable_truncation(MAX_TOKENS)
        self.tokenizer.enable_padding()
        model = sorted(glob.glob(os.path.join(folder, "onnx", "*.onnx")))[0]
        options = ort.SessionOptions()
        options.intra_op_num_threads = 4
        self.session = ort.InferenceSession(model, options, providers=["CPUExecutionProvider"])
        self.inputs = {i.name for i in self.session.get_inputs()}

    def scores(self, question, texts):
        batch = self.tokenizer.encode_batch([(question, t) for t in texts])
        feed = {"input_ids": np.array([e.ids for e in batch], dtype=np.int64),
                "attention_mask": np.array([e.attention_mask for e in batch], dtype=np.int64)}
        if "token_type_ids" in self.inputs:
            feed["token_type_ids"] = np.array([e.type_ids for e in batch], dtype=np.int64)
        return self.session.run(None, feed)[0][:, 0]


def query(url, key, workspace, question):
    body = json.dumps({"workspace": workspace, "question": question, "token_budget": 4000, "max_results": TOP}).encode()
    request = urllib.request.Request(url + "/v1/query", data=body, method="POST")
    request.add_header("Content-Type", "application/json")
    request.add_header("Authorization", "Bearer " + key)
    with urllib.request.urlopen(request, timeout=60) as response:
        return json.loads(response.read())["results"]


def metrics(ranks):
    n = len(ranks)
    return {"n": n, "hit@1": sum(r == 1 for r in ranks), "hit@5": sum(bool(r) and r <= 5 for r in ranks),
            "mrr": round(sum(1 / r for r in ranks if r) / n, 3)}


def main():
    folder, which, url = sys.argv[1], sys.argv[2], sys.argv[3]
    key = open(sys.argv[4]).read().strip()
    questions = vector_questions() if which == "vector" else idea_questions(sys.argv[5])
    reranker = Reranker(folder)
    reranker.scores("warm up", ["warm up"])
    before, after, blended, times = [], [], [], []
    for workspace, question, expected in questions:
        items = query(url, key, workspace, question)
        start = time.perf_counter()
        scores = reranker.scores(question, [i["title"] + "\n" + (i.get("snippet") or "") for i in items]) if items else []
        times.append((time.perf_counter() - start) * 1000)
        order = sorted(range(len(items)), key=lambda i: -scores[i])
        reranked = [items[i] for i in order]
        position = {i: p for p, i in enumerate(order)}
        fused = sorted(range(len(items)), key=lambda i: -(1 / (RRF_K + i) + 1 / (RRF_K + position[i])))
        before.append(rank_of(items, expected))
        after.append(rank_of(reranked, expected))
        blended.append(rank_of([items[i] for i in fused], expected))
        print(f"{str(before[-1]):5} -> {str(after[-1]):5} blend {str(blended[-1]):5} {len(items):2} items {times[-1]:6.1f} ms  {question[:60]}")
    times.sort()
    print(json.dumps({"set": which, "before": metrics(before), "after": metrics(after), "blended": metrics(blended),
                      "rerank_ms": {"p50": round(times[len(times) // 2], 1), "p95": round(times[int(len(times) * 0.95) - 1], 1)}}, indent=1))


main()
