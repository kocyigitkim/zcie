#!/usr/bin/env python3
"""End-to-end latency benchmark. Uncached measurements change the token budget per call (it is part of the cache key).

 against a running zcied (docs/13-test-plan.md section 7).

Usage: bench.py <url> <api-key> <workspace> <corpus-folder> [iterations]
All timings include the loopback HTTP round trip (one persistent connection).
"""
import http.client
import json
import os
import statistics
import sys
import time
import urllib.parse

URL, KEY, WS, CORPUS = sys.argv[1:5]
ITER = int(sys.argv[5]) if len(sys.argv) > 5 else 1000
HOST = urllib.parse.urlparse(URL)
CONN = http.client.HTTPConnection(HOST.hostname, HOST.port)
HEADERS = {"Authorization": f"Bearer {KEY}", "Content-Type": "application/json"}


def call(method, path, body=None):
    CONN.request(method, path, body=json.dumps(body) if body is not None else None, headers=HEADERS)
    response = CONN.getresponse()
    data = response.read()
    if response.status != 200:
        raise RuntimeError(f"{path}: {response.status} {data[:200]}")
    return json.loads(data)


def query(question, budget=250, mode="auto"):
    return call("POST", "/v1/query", {"workspace": WS, "question": question, "mode": mode, "token_budget": budget})


def measure(name, fn, iterations=ITER):
    samples = []
    for index in range(iterations):
        start = time.perf_counter()
        fn(index)
        samples.append((time.perf_counter() - start) * 1000)
    samples.sort()
    pick = lambda q: samples[min(len(samples) - 1, int(q * len(samples)))]
    print(f"| {name} | {pick(0.50):.2f} | {pick(0.95):.2f} | {pick(0.99):.2f} | {len(samples)} |")
    return samples


def main():
    first = time.perf_counter()
    cold = query("where is compute_6_3 defined", budget=251)
    print(f"cold first query: {(time.perf_counter() - first) * 1000:.2f} ms, answer: {cold.get('answer')}")
    sym_ref = cold["results"][0]["ref"]
    print("| Operation | p50 ms | p95 ms | p99 ms | n |")
    print("|---|---|---|---|---|")
    measure("L0 cache hit (same query)", lambda i: query("where is compute_6_3 defined", budget=251))
    measure("Exact entity lookup (GET /v1/entities)", lambda i: call("GET", "/v1/entities/" + urllib.parse.quote(sym_ref, safe="")))
    measure("Symbol lookup (distinct names)", lambda i: query(f"where is compute_{(i % 295) * 3}_{i % 30} defined", budget=300 + i))
    measure("Lexical + metadata search", lambda i: query(f"render part widget {i % 300} size", budget=300 + i, mode="search"))
    measure("Hybrid retrieval with graph expansion", lambda i: query(f"engine compute value {i % 300}", budget=300 + i))
    measure("Graph neighborhood (task query)", lambda i: query("#89", budget=300 + i))
    measure("Durable capture (small event)", lambda i: call("POST", "/v1/events", {
        "schema": "zcie.event.v1", "idempotency_key": f"bench:{time.time_ns()}:{i}", "workspace": WS,
        "source": {"app": "bench", "session_id": "bench"}, "type": "user.message", "payload": f"benchmark message {i}"}))
    measure_file_change()


def measure_file_change(iterations=20):
    path = os.path.join(CORPUS, "src", "part0", "file_0.rs")
    def change(index):
        name = f"bench_marker_{index}_{time.time_ns()}"
        with open(path, "a") as handle:
            handle.write(f"\npub fn {name}() {{}}\n")
        deadline = time.time() + 10
        while time.time() < deadline:
            if query(f"where is {name} defined").get("answer"):
                return
            time.sleep(0.01)
        raise RuntimeError("file change was not indexed in 10 s")
    measure("Small file change -> current index (incl. 50-200 ms debounce)", change, iterations)


main()
