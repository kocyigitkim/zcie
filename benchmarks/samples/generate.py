#!/usr/bin/env python3
"""Makes three sample projects for the multi-project benchmark (ADR-009).

Usage: generate.py <out dir>
Output: <out>/ledger-rs (Rust), <out>/atlas-ts (TypeScript), <out>/orbit-py (Python).
Each project is a Git repository with a short history. Each has a few "target" facts that the
benchmark asks about, and about 45 filler modules, so that a search without an index has real work.
The output is the same for each run (fixed random seed, fixed commit dates).
"""
import os
import random
import subprocess
import sys

WORDS = ["account", "batch", "cursor", "delta", "entry", "filter", "gauge", "header", "index", "journal",
         "kernel", "ledger", "metric", "node", "offset", "packet", "quota", "record", "signal", "token",
         "unit", "vector", "window", "yield", "zone", "anchor", "bucket", "channel", "digest", "event"]
VERBS = ["build", "check", "merge", "split", "scale", "parse", "render", "update", "verify", "collect"]
FILLER_MODULES = 45
FUNCTIONS_PER_MODULE = 6


def write(root, path, text):
    full = os.path.join(root, path)
    os.makedirs(os.path.dirname(full), exist_ok=True)
    with open(full, "w") as handle:
        handle.write(text.lstrip("\n"))


def git(root, *args, date="2026-09-01T10:00:00"):
    env = dict(os.environ, GIT_AUTHOR_DATE=date, GIT_COMMITTER_DATE=date, GIT_AUTHOR_NAME="Sample Dev",
               GIT_AUTHOR_EMAIL="dev@example.com", GIT_COMMITTER_NAME="Sample Dev", GIT_COMMITTER_EMAIL="dev@example.com")
    subprocess.run(["git", "-C", root, *args], check=True, env=env, stdout=subprocess.DEVNULL)


def commit(root, message, date):
    git(root, "add", "-A", date=date)
    git(root, "commit", "-q", "-m", message, date=date)


def camel(word):
    return word[0].upper() + word[1:]


def names(rng, index):
    noun = rng.choice(WORDS)
    return noun, [f"{rng.choice(VERBS)}_{rng.choice(WORDS)}_{index}_{n}" for n in range(FUNCTIONS_PER_MODULE)]


def rust_filler(rng, index):
    noun, functions = names(rng, index)
    body = [f"//! Generated module {index}: {noun} helpers.\n", f"/// A {noun} record.",
            "#[derive(Debug, Clone, PartialEq)]", f"pub struct {camel(noun)}{index} {{",
            "    pub id: u64,", "    pub amount_cents: i64,", "    pub label: String,", "}\n", f"impl {camel(noun)}{index} {{"]
    for name in functions:
        steps = rng.randint(2, 9)
        body += [f"    /// Returns the {name.split('_')[0]} value of the {noun}.", f"    pub fn {name}(&self, factor: i64) -> i64 {{",
                 "        let mut total = self.amount_cents;", f"        for step in 0..{steps} {{",
                 "            total += factor * step;", "        }", "        total", "    }\n"]
    return "\n".join(body) + "}\n"


def ts_filler(rng, index):
    noun, functions = names(rng, index)
    body = [f"// Generated module {index}: {noun} helpers.\n", f"export interface {camel(noun)}{index} {{",
            "  id: number;", "  amountCents: number;", "  label: string;", "}\n"]
    for name in functions:
        parts = name.split("_")
        ts_name = parts[0] + "".join(camel(p) for p in parts[1:])
        steps = rng.randint(2, 9)
        body += [f"/** Returns the {parts[0]} value of the {noun}. */",
                 f"export function {ts_name}(item: {camel(noun)}{index}, factor: number): number {{",
                 "  let total = item.amountCents;", f"  for (let step = 0; step < {steps}; step++) {{",
                 "    total += factor * step;", "  }", "  return total;", "}\n"]
    return "\n".join(body)


def py_filler(rng, index):
    noun, functions = names(rng, index)
    body = [f'"""Generated module {index}: {noun} helpers."""\n', "from dataclasses import dataclass\n\n",
            "@dataclass", f"class {camel(noun)}{index}:", f'    """A {noun} record."""\n', "    id: int",
            "    amount_cents: int", "    label: str\n"]
    for name in functions:
        steps = rng.randint(2, 9)
        body += [f"\ndef {name}(item: {camel(noun)}{index}, factor: int) -> int:",
                 f'    """Returns the {name.split("_")[0]} value of the {noun}."""', "    total = item.amount_cents",
                 f"    for step in range({steps}):", "        total += factor * step", "    return total\n"]
    return "\n".join(body)


def ledger(root, rng):
    write(root, "Cargo.toml", '[package]\nname = "ledger-rs"\nversion = "0.3.0"\nedition = "2021"\n')
    write(root, "README.md", "# ledger-rs\n\nInvoices, balances and late fees.\n")
    mods = "\n".join(f"pub mod m{i:02};" for i in range(FILLER_MODULES))
    write(root, "src/gen/mod.rs", mods + "\n")
    for i in range(FILLER_MODULES):
        write(root, f"src/gen/m{i:02}.rs", rust_filler(rng, i))
    write(root, "src/lib.rs", "pub mod gen;\n")
    commit(root, "chore: initial ledger skeleton", "2026-09-01T10:00:00")
    write(root, "src/lib.rs", "pub mod billing;\npub mod currency;\npub mod gen;\npub mod storage;\n")
    write(root, "src/billing/mod.rs", "pub mod late_fee;\n")
    write(root, "src/billing/late_fee.rs", LATE_FEE.replace("{RATE}", "1.5"))
    write(root, "src/currency.rs", RUST_CURRENCY)
    write(root, "src/storage/mod.rs", "pub mod pool;\n")
    write(root, "src/storage/pool.rs", RUST_POOL)
    commit(root, "feat: late fees, currency codes and the Postgres pool\n\nZCIE-Task: #14", "2026-09-03T09:30:00")
    write(root, "src/billing/late_fee.rs", LATE_FEE.replace("{RATE}", "1.75"))
    commit(root, "fix: raise the late fee from 1.5 to 1.75 percent\n\nZCIE-Task: #21", "2026-09-10T16:05:00")


def atlas(root, rng):
    write(root, "package.json", '{\n  "name": "atlas-ts",\n  "version": "1.4.0",\n  "dependencies": { "redis": "4.6.0", "fastify": "4.26.0" }\n}\n')
    write(root, "README.md", "# atlas-ts\n\nRoute resolution service.\n")
    for i in range(FILLER_MODULES):
        write(root, f"src/gen/feature{i:02}.ts", ts_filler(rng, i))
    write(root, "src/index.ts", 'export * from "./gen/feature00";\n')
    commit(root, "chore: initial atlas skeleton", "2026-09-02T11:00:00")
    write(root, "src/router/resolve.ts", TS_RESOLVE)
    write(root, "src/cache/policy.ts", TS_CACHE)
    write(root, "src/util/currency.ts", TS_CURRENCY)
    commit(root, "feat: route resolution with a Redis route cache\n\nZCIE-Task: #7", "2026-09-05T14:20:00")


def orbit(root, rng):
    write(root, "pyproject.toml", '[project]\nname = "orbit-py"\nversion = "0.9.0"\ndependencies = ["rq==1.16.2"]\n')
    write(root, "README.md", "# orbit-py\n\nSatellite manoeuvre scheduling.\n")
    write(root, "orbit/__init__.py", "")
    write(root, "orbit/gen/__init__.py", "")
    for i in range(FILLER_MODULES):
        write(root, f"orbit/gen/mod{i:02}.py", py_filler(rng, i))
    commit(root, "chore: initial orbit skeleton", "2026-09-02T08:00:00")
    write(root, "orbit/scheduler.py", PY_SCHEDULER)
    write(root, "orbit/queue.py", PY_QUEUE)
    commit(root, "feat: schedule orbit burns on an RQ queue\n\nZCIE-Task: #3", "2026-09-08T13:45:00")


LATE_FEE = '''
//! Late fees for unpaid invoices.

/// Monthly late fee rate in percent.
pub const LATE_FEE_PERCENT: f64 = {RATE};
/// Days after the due date with no fee.
pub const GRACE_DAYS: u32 = 12;

/// Returns the balance with the late fee for an invoice that is `days_late` days late.
pub fn apply_late_fee(balance_cents: i64, days_late: u32) -> i64 {
    if days_late <= GRACE_DAYS {
        return balance_cents;
    }
    let fee = (balance_cents as f64) * LATE_FEE_PERCENT / 100.0;
    balance_cents + fee.round() as i64
}
'''

RUST_CURRENCY = '''
//! Currency codes.

/// Returns the ISO 4217 code for a user value such as " eur" or "€".
pub fn normalize_currency(code: &str) -> String {
    match code.trim() {
        "€" => "EUR".to_string(),
        "$" => "USD".to_string(),
        other => other.to_uppercase(),
    }
}
'''

RUST_POOL = '''
//! Database connection pool.

/// Maximum open connections to the database.
pub const MAX_DB_CONNECTIONS: u32 = 24;

/// Returns the connection URL of the ledger database.
pub fn connection_url(host: &str) -> String {
    format!("postgres://ledger@{host}:5432/ledger")
}
'''

TS_RESOLVE = '''
export interface RouteTable {
  [prefix: string]: string;
}

/** Returns the handler of the longest prefix that matches the path. */
export function resolveRoute(path: string, table: RouteTable): string | undefined {
  let best: string | undefined;
  for (const prefix of Object.keys(table)) {
    if (path.startsWith(prefix) && (best === undefined || prefix.length > best.length)) {
      best = prefix;
    }
  }
  return best === undefined ? undefined : table[best];
}
'''

TS_CACHE = '''
import { createClient } from "redis";

/** Time to live of a cached route, in seconds. */
export const CACHE_TTL_SECONDS = 420;

/** Returns a Redis client for the route cache. */
export function cacheClient() {
  return createClient({ url: process.env.REDIS_URL });
}
'''

TS_CURRENCY = '''
/** Returns the ISO 4217 code for a user value such as " eur" or "€". */
export function normalizeCurrency(code: string): string {
  const value = code.trim();
  if (value === "€") return "EUR";
  if (value === "$") return "USD";
  return value.toUpperCase();
}
'''

PY_SCHEDULER = '''
"""Burn scheduling."""

MAX_BURN_RETRIES = 7


def schedule_orbit_burn(satellite_id: str, delta_v: float, queue) -> str:
    """Puts one burn on the queue and returns the job ID."""
    if delta_v <= 0:
        raise ValueError("delta_v must be positive")
    job = queue.enqueue("orbit.burn.execute", satellite_id, delta_v, retry=MAX_BURN_RETRIES)
    return job.id
'''

PY_QUEUE = '''
"""The job queue."""

from rq import Queue


def burn_queue(connection) -> Queue:
    """Returns the queue for burn jobs."""
    return Queue("burns", connection=connection)
'''


def main():
    out = os.path.abspath(sys.argv[1])
    rng = random.Random(20261016)
    for name, build in (("ledger-rs", ledger), ("atlas-ts", atlas), ("orbit-py", orbit)):
        root = os.path.join(out, name)
        if os.path.exists(root):
            subprocess.run(["rm", "-rf", root], check=True)
        os.makedirs(root)
        git(root, "init", "-q", "-b", "main")
        build(root, rng)
        lines = sum(1 for d, _, fs in os.walk(root) if ".git" not in d for f in fs for _ in open(os.path.join(d, f), errors="ignore"))
        print(f"{name}: {lines} lines")


if __name__ == "__main__":
    main()
