#!/usr/bin/env python3
"""Vector search evaluation (ADR-011): the same labelled questions with vector search off and on.

Usage: eval.py <samples dir> <model folder> <work dir> [port]
<samples dir> comes from benchmarks/samples/generate.py. A separate daemon (own home, own port) indexes
the three sample projects. Each question has the title (or a text part) of the expected result.
Metrics: hit@1, hit@5 and the mean reciprocal rank (MRR) over the first 10 results.
"""
import json
import os
import shutil
import subprocess
import sys
import time
import urllib.request

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
SAMPLES, MODEL, WORK = (os.path.abspath(a) for a in sys.argv[1:4])
PORT = int(sys.argv[4]) if len(sys.argv) > 4 else 47996
URL, HOME = f"http://127.0.0.1:{PORT}", os.path.join(WORK, "home")

DECISIONS = {
    "ledger-rs": "Decision: ledger-rs keeps all balances in Postgres 16. We rejected MySQL because it has no transactional DDL.",
    "atlas-ts": "We decided to drop Express and use Fastify 4 for the HTTP layer of atlas-ts.",
    "orbit-py": "Decision: orbit-py replaced Celery with RQ because the Celery workers leaked memory during long burns.",
}
# (group, project, question, expected title part)
QUESTIONS = [
    ("paraphrase", "ledger-rs", "how much do we charge customers who pay their bills too late", "late_fee"),
    ("paraphrase", "ledger-rs", "penalty percentage for overdue invoices", "LATE_FEE_PERCENT"),
    ("paraphrase", "ledger-rs", "how many days can a payment slip before we charge extra", "GRACE_DAYS"),
    ("paraphrase", "ledger-rs", "turn the euro sign into an ISO code", "normalize_currency"),
    ("paraphrase", "ledger-rs", "largest number of open sessions to the database server", "MAX_DB_CONNECTIONS"),
    ("paraphrase", "ledger-rs", "build the address string for the postgres server", "connection_url"),
    ("paraphrase", "atlas-ts", "choose the handler for a request path by the longest prefix", "resolveRoute"),
    ("paraphrase", "atlas-ts", "how long do we keep a cached route", "CACHE_TTL_SECONDS"),
    ("paraphrase", "atlas-ts", "create a client for the key value store", "cacheClient"),
    ("paraphrase", "atlas-ts", "turn a dollar symbol into a currency code", "normalizeCurrency"),
    ("paraphrase", "orbit-py", "plan a thruster firing for a satellite", "schedule_orbit_burn"),
    ("paraphrase", "orbit-py", "how often do we try again when a manoeuvre fails", "MAX_BURN_RETRIES"),
    ("paraphrase", "orbit-py", "get the work queue for engine firings", "burn_queue"),
    ("memory", "orbit-py", "why did we stop using the old background task runner", "Celery"),
    ("memory", "atlas-ts", "which web framework did we pick for the server", "Fastify"),
    ("memory", "ledger-rs", "which relational store holds the account balances", "Postgres"),
    # Hard: no content word is shared with the code, the doc comments or the decisions.
    ("hard", "ledger-rs", "surcharge for tardy remittance", "late_fee"),
    ("hard", "ledger-rs", "standardize a money symbol", "normalize_currency"),
    ("hard", "ledger-rs", "cap on simultaneous links to storage", "MAX_DB_CONNECTIONS"),
    ("hard", "atlas-ts", "expiry of memoized navigation entries", "CACHE_TTL_SECONDS"),
    ("hard", "orbit-py", "propulsion maneuver for a spacecraft", "schedule_orbit_burn"),
    ("hard", "orbit-py", "attempts limit for propulsion jobs", "MAX_BURN_RETRIES"),
    ("hard", "orbit-py", "reason the job processors were swapped", "Celery"),
    ("hard", "ledger-rs", "which sql engine did we turn down", "MySQL"),
    # Turkish questions about English code and decisions (the language of the user).
    ("turkish", "ledger-rs", "geç ödeyen müşterilerden ne kadar ücret alıyoruz", "late_fee"),
    ("turkish", "ledger-rs", "vadesi geçmiş faturalar için ceza yüzdesi", "LATE_FEE_PERCENT"),
    ("turkish", "ledger-rs", "veritabanı sunucusuna en fazla kaç bağlantı açılır", "MAX_DB_CONNECTIONS"),
    ("turkish", "atlas-ts", "önbellekteki rota ne kadar süre tutulur", "CACHE_TTL_SECONDS"),
    ("turkish", "orbit-py", "uydu için itici ateşlemesi planla", "schedule_orbit_burn"),
    ("turkish", "orbit-py", "eski arka plan görev çalıştırıcısını neden bıraktık", "Celery"),
    ("turkish", "atlas-ts", "sunucu için hangi web çatısını seçtik", "Fastify"),
    ("turkish", "ledger-rs", "hangi sql motorunu reddettik", "MySQL"),
    ("exact", "ledger-rs", "where is apply_late_fee defined", "apply_late_fee"),
    ("exact", "atlas-ts", "resolveRoute longest prefix", "resolveRoute"),
    ("exact", "orbit-py", "MAX_BURN_RETRIES", "MAX_BURN_RETRIES"),
    ("exact", "ledger-rs", "late fee percent", "LATE_FEE_PERCENT"),
]


def call(method, path, body=None, key=None):
    req = urllib.request.Request(URL + path, data=json.dumps(body).encode() if body is not None else None, method=method)
    req.add_header("Content-Type", "application/json")
    if key:
        req.add_header("Authorization", f"Bearer {key}")
    with urllib.request.urlopen(req, timeout=30) as response:
        return json.loads(response.read())


def start(vector):
    env = dict(os.environ, ZCIE_SERVER_HTTP_ADDR=f'"127.0.0.1:{PORT}"', ZCIE_VECTOR_ENABLED="true" if vector else "false")
    env.pop("ZCIE_API_KEY", None)
    log = open(os.path.join(WORK, "zcied.log"), "a")
    proc = subprocess.Popen([os.path.join(ROOT, "target/release/zcied"), "--home", HOME], env=env, stdout=log, stderr=log)
    for _ in range(600):
        try:
            call("GET", "/v1/health")
            return proc
        except OSError:
            time.sleep(0.05)
    raise RuntimeError("daemon did not start")


def wait_idle(key):
    for _ in range(1200):
        queue = call("GET", "/v1/health", key=key).get("queue") or {}
        if queue.get("interactive", 1) + queue.get("background", 1) == 0:
            time.sleep(2)
            return
        time.sleep(0.25)


def setup(key):
    for name, decision in DECISIONS.items():
        root = os.path.join(SAMPLES, name)
        call("POST", "/v1/projects", {"name": name, "root": root, "index": True}, key)  # ADR-015: index on request only
        call("POST", "/v1/events", {"schema": "zcie.event.v1", "idempotency_key": f"eval:{name}", "workspace": name,
                                    "source": {"app": "eval", "session_id": f"eval-{name}", "turn_id": "1"}, "type": "user.message", "payload": decision}, key)
    wait_idle(key)


def rank_of(results, expected):
    for index, item in enumerate(results[:10]):
        if expected.lower() in (item["title"] + " " + (item.get("snippet") or "")).lower():
            return index + 1
    return None


def evaluate(key, vector):
    rows = []
    for group, project, question, expected in QUESTIONS:
        response = call("POST", "/v1/query", {"workspace": project, "question": question, "token_budget": 2000, "max_results": 10}, key)
        rows.append({"vector": vector, "group": group, "project": project, "question": question, "expected": expected,
                     "rank": rank_of(response["results"], expected), "class": response["class"]})
    return rows


def summary(rows):
    out = {}
    for group in ["paraphrase", "hard", "memory", "turkish", "exact", "all"]:
        sel = [r for r in rows if group in ("all", r["group"])]
        out[group] = {"n": len(sel), "hit@1": sum(r["rank"] == 1 for r in sel), "hit@5": sum(bool(r["rank"]) and r["rank"] <= 5 for r in sel),
                      "mrr": round(sum(1 / r["rank"] for r in sel if r["rank"]) / len(sel), 3)}
    return out


def main():
    shutil.rmtree(WORK, ignore_errors=True)
    os.makedirs(os.path.join(HOME, "models"))
    os.symlink(MODEL, os.path.join(HOME, "models", os.path.basename(MODEL)))
    rows = []
    for vector in (False, True):
        proc = start(vector)
        key = open(os.path.join(HOME, "config", "admin.key")).read().strip()
        if not vector:
            setup(key)
        else:
            time.sleep(3)  # the background fill of the vector index
        rows += evaluate(key, vector)
        proc.terminate()
        proc.wait(timeout=60)
    with open(os.path.join(WORK, "rows.jsonl"), "w") as handle:
        handle.writelines(json.dumps(r) + "\n" for r in rows)
    print(json.dumps({"off": summary([r for r in rows if not r["vector"]]), "on": summary([r for r in rows if r["vector"]])}, indent=1))
    for off, on in zip([r for r in rows if not r["vector"]], [r for r in rows if r["vector"]]):
        print(f"{off['group']:10} off={str(off['rank']):5} on={str(on['rank']):5} {off['question']}")


main()
