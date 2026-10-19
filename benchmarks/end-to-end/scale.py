#!/usr/bin/env python3
"""Scale benchmark: index a generated corpus of N lines and measure (docs/13-test-plan.md section 7).

Usage: scale.py <lines> <work dir> [port] [iterations]
With ZCIE_SCALE_MODEL=<model folder>, the daemon also runs the vector search (ADR-011).
Steps: generate the corpus, start a separate zcied (own home, own port), register the corpus,
measure the initial index time, storage and memory, the restart time, then run bench.py.
The daemon of the user (~/.zcie) is not touched.
"""
import json
import os
import shutil
import subprocess
import sys
import time
import urllib.request

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
ZCIED = os.path.join(ROOT, "target", "release", "zcied")
LINES, WORK = int(sys.argv[1]), os.path.abspath(sys.argv[2])
PORT = int(sys.argv[3]) if len(sys.argv) > 3 else 47999
ITER = sys.argv[4] if len(sys.argv) > 4 else "1000"
URL = f"http://127.0.0.1:{PORT}"
HOME, CORPUS = os.path.join(WORK, "home"), os.path.join(WORK, "corpus")
WS = "scale"


def request(method, path, body=None, key=None):
    data = json.dumps(body).encode() if body is not None else None
    req = urllib.request.Request(URL + path, data=data, method=method)
    req.add_header("Content-Type", "application/json")
    if key:
        req.add_header("Authorization", f"Bearer {key}")
    with urllib.request.urlopen(req, timeout=30) as response:
        return json.loads(response.read())


def start():
    env = dict(os.environ, ZCIE_SERVER_HTTP_ADDR=f'"127.0.0.1:{PORT}"')
    env.pop("ZCIE_API_KEY", None)
    log = open(os.path.join(WORK, "zcied.log"), "a")
    proc = subprocess.Popen([ZCIED, "--home", HOME], env=env, stdout=log, stderr=log, stdin=subprocess.DEVNULL)
    started = time.time()
    while time.time() - started < 120:
        try:
            request("GET", "/v1/health")
            return proc, time.time() - started
        except OSError:
            time.sleep(0.05)
    raise RuntimeError("zcied did not start")


def wait_indexed(key, last_symbol):
    started = time.time()
    while time.time() - started < 3600:
        answer = request("POST", "/v1/query", {"workspace": WS, "question": f"where is {last_symbol} defined", "token_budget": 250}, key)
        health = request("GET", "/v1/health", key=key)
        queue = health.get("queue") or {}
        if answer.get("answer") and queue.get("interactive", 1) + queue.get("background", 1) == 0:
            return time.time() - started, health
        time.sleep(0.5)
    raise RuntimeError("index did not finish in 1 hour")


def folder_mb(path):
    total = sum(os.path.getsize(os.path.join(d, f)) for d, _, files in os.walk(path) for f in files)
    return round(total / 1e6, 1)


def rss_mb(pid):
    out = subprocess.run(["ps", "-o", "rss=", "-p", str(pid)], capture_output=True, text=True).stdout.strip()
    return round(int(out or 0) / 1024, 1)


def last_symbol():
    files = sorted((f for d, _, fs in os.walk(os.path.join(CORPUS, "src")) for f in fs), key=lambda f: int(f.split("_")[1].split(".")[0]))
    return f"compute_{files[-1].split('_')[1].split('.')[0]}_29"


def main():
    shutil.rmtree(WORK, ignore_errors=True)
    os.makedirs(WORK)
    if os.environ.get("ZCIE_SCALE_MODEL"):
        os.makedirs(os.path.join(HOME, "models"))
        os.symlink(os.environ["ZCIE_SCALE_MODEL"], os.path.join(HOME, "models", "potion-base-8M"))
    print(subprocess.run([sys.executable, os.path.join(ROOT, "benchmarks/corpus-generator/generate.py"), CORPUS, str(LINES)], capture_output=True, text=True, check=True).stdout.strip())
    proc, _ = start()
    key = open(os.path.join(HOME, "config", "admin.key")).read().strip()
    request("POST", "/v1/repositories", {"workspace": WS, "path": CORPUS}, key)
    index_s, health = wait_indexed(key, last_symbol())
    instance = os.path.join(HOME, "instances", "local")
    report = {"lines": LINES, "initial_index_s": round(index_s, 1), "rss_after_index_mb": rss_mb(proc.pid),
              "storage_mb": {name: folder_mb(os.path.join(instance, name)) for name in os.listdir(instance) if os.path.isdir(os.path.join(instance, name))},
              "watermarks": health.get("watermarks")}
    proc.terminate()
    proc.wait(timeout=120)
    proc, restart_s = start()
    report["restart_s"] = round(restart_s, 2)
    time.sleep(5)
    report["rss_after_restart_mb"] = rss_mb(proc.pid)
    report["vector_fill"] = [line.strip()[-80:] for line in open(os.path.join(WORK, "zcied.log")) if "vector index filled" in line]
    print(json.dumps(report, indent=1))
    bench = subprocess.run([sys.executable, os.path.join(ROOT, "benchmarks/end-to-end/bench.py"), URL, key, WS, CORPUS, ITER], capture_output=True, text=True)
    print(bench.stdout, bench.stderr[-2000:])
    report["rss_after_bench_mb"] = rss_mb(proc.pid)
    print(json.dumps({"rss_after_bench_mb": report["rss_after_bench_mb"]}))
    proc.terminate()
    proc.wait(timeout=120)


main()
