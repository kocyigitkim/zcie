#!/usr/bin/env node
// Launcher of the ZCIE command line. It runs the native `zcie` program of this platform from
// `native/<platform>-<arch>/` (with `zcied` and `zcie-mcp` next to it) and passes all arguments,
// the terminal and the exit code through.
"use strict";

const { chmodSync, existsSync, statSync } = require("node:fs");
const { homedir } = require("node:os");
const { join } = require("node:path");
const { spawnSync } = require("node:child_process");

const PLATFORM = `${process.platform}-${process.arch}`;
const NATIVE = join(__dirname, "..", "native", PLATFORM);
const PROGRAMS = ["zcie", "zcied", "zcie-mcp"];
// Commands that work before `zcie setup` (no hint for them).
const NO_HINT = new Set(["setup", "help", "--help", "-h", "--version", "-V", "uninstall"]);

function fail(message) {
  process.stderr.write(`zcie: ${message}\n`);
  process.exit(1);
}

/** npm can drop the executable bit (for example after a copy); set it again. */
function ensureExecutable(file) {
  if ((statSync(file).mode & 0o111) === 0) chmodSync(file, 0o755);
}

const exe = join(NATIVE, "zcie");
if (!existsSync(exe)) {
  fail(`this package has no build for ${PLATFORM}. Build ZCIE from source (cargo build --release) and use target/release/zcie.`);
}
for (const name of PROGRAMS) {
  const file = join(NATIVE, name);
  if (!existsSync(file)) fail(`${file} is missing; reinstall the package (npm install -g zcie).`);
  ensureExecutable(file);
}

const args = process.argv.slice(2);
const home = process.env.ZCIE_HOME || join(homedir(), ".zcie");
if (args.length > 0 && !NO_HINT.has(args[0]) && !existsSync(join(home, "config", "setup.toml"))) {
  process.stderr.write("zcie: not set up yet. Run: zcie setup\n");
}

const result = spawnSync(exe, args, { stdio: "inherit" });
if (result.error) fail(result.error.message);
process.exit(result.status === null ? 1 : result.status);
