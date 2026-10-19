"""Shared helpers of the isolated benchmark (ADR-019, D-067): daemon start and stop, HTTP calls,
free ports and the tree hash of a ZCIE home."""
import hashlib
import json
import os
import socket
import subprocess
import time
import urllib.request

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
ZCIED = os.path.join(ROOT, "target", "release", "zcied")
HOOKS = os.path.join(ROOT, "adapters", "claude-code", "hooks")
# Files of a home that change at each start (logs, locks, sockets); they are not part of the store.
VOLATILE = ("run", "telemetry", "LOG", "LOCK", "zcied.log", "manifest.json")


def clean_env():
    """The environment of the benchmark process without the ZCIE variables of the caller."""
    return {k: v for k, v in os.environ.items() if not k.startswith("ZCIE_")}


def free_port():
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        return sock.getsockname()[1]


def call(port, method, path, body=None, key=None, timeout=60):
    data = json.dumps(body).encode() if body is not None else None
    request = urllib.request.Request(f"http://127.0.0.1:{port}{path}", data=data, method=method)
    request.add_header("Content-Type", "application/json")
    if key:
        request.add_header("Authorization", f"Bearer {key}")
    with urllib.request.urlopen(request, timeout=timeout) as response:
        return json.loads(response.read())


def start_daemon(home, port, log_name="zcied.log"):
    env = dict(clean_env(), ZCIE_SERVER_HTTP_ADDR=f'"127.0.0.1:{port}"')
    log = open(os.path.join(home, log_name), "a")
    proc = subprocess.Popen([ZCIED, "--home", home], env=env, stdout=log, stderr=log)
    for _ in range(1200):
        try:
            call(port, "GET", "/v1/health", timeout=2)
            return proc
        except OSError:
            if proc.poll() is not None:
                raise RuntimeError(f"zcied stopped; see {home}/{log_name}")
            time.sleep(0.05)
    proc.kill()
    raise RuntimeError("zcied did not start")


def stop_daemon(proc):
    proc.terminate()
    try:
        proc.wait(timeout=120)
    except subprocess.TimeoutExpired:
        proc.kill()
        proc.wait()


def key(home, name="admin"):
    return open(os.path.join(home, "config", f"{name}.key")).read().strip()


def wait_idle(port, admin, timeout=1800):
    """Waits until the queues are empty and the search watermark reaches the canonical one."""
    deadline = time.time() + timeout
    while time.time() < deadline:
        health = call(port, "GET", "/v1/health", key=admin)
        queue = health.get("queue") or {}
        marks = health.get("watermarks") or {}
        busy = queue.get("interactive", 1) + queue.get("background", 1)
        if busy == 0 and marks.get("search_seq", 0) >= marks.get("canonical_seq", 0):
            time.sleep(1)
            return
        time.sleep(0.5)
    raise RuntimeError("zcied did not become idle")


def tree_hash(home):
    """BLAKE2 hash of the paths and contents of a home, without the volatile files."""
    digest = hashlib.blake2b(digest_size=16)
    for folder, dirs, files in sorted(os.walk(home)):
        dirs.sort()
        rel_folder = os.path.relpath(folder, home)
        if any(part in VOLATILE for part in rel_folder.split(os.sep)):
            continue
        for name in sorted(files):
            if name in VOLATILE or name.startswith("LOG"):
                continue
            path = os.path.join(folder, name)
            if os.path.islink(path):
                digest.update(f"{rel_folder}/{name}->{os.readlink(path)}".encode())
                continue
            digest.update(f"{rel_folder}/{name}".encode())
            with open(path, "rb") as handle:
                for block in iter(lambda: handle.read(1 << 20), b""):
                    digest.update(block)
    return digest.hexdigest()
