// Runs the OpenCode plugin against a running daemon: ZCIE_URL, ZCIE_API_KEY (admin), ZCIE_TEST_REPO.
import { test } from "node:test";
import assert from "node:assert/strict";
import { existsSync, mkdtempSync, readFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { ZciePlugin } from "../zcie.js";

const url = process.env.ZCIE_URL;
const key = process.env.ZCIE_API_KEY;
delete process.env.ZCIE_WORKSPACE;

async function api(method, path, body) {
  const response = await fetch(url + path, {
    method, headers: { "Content-Type": "application/json", Authorization: `Bearer ${key}` },
    body: body && JSON.stringify(body),
  });
  return response.json();
}

function message(text) {
  return { message: { id: "msg_test" }, parts: [{ type: "text", text }] };
}

test("the plugin makes the project, injects a capsule once per session and again after a compaction", async () => {
  const root = mkdtempSync(join(tmpdir(), "zcie-opencode-"));
  const hooks = await ZciePlugin({ directory: root, worktree: root });
  const id = readFileSync(join(root, ".claude", "zcie_id"), "utf8").trim();
  assert.match(id, /^proj_/);
  await api("POST", "/v1/repositories", { workspace: id, path: process.env.ZCIE_TEST_REPO });
  for (let i = 0; i < 300; i++) {
    const found = await api("POST", "/v1/query", { workspace: id, question: "where is attach_to_bone defined" });
    if (found.answer) break;
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  const first = message("where is attach_to_bone defined");
  await hooks["chat.message"]({ sessionID: "ses_1" }, first);
  assert.equal(first.parts.length, 2);
  assert.equal(first.parts[1].synthetic, true);
  assert.match(first.parts[1].text, /attach_to_bone/);
  const second = message("where is attach_to_bone defined");
  await hooks["chat.message"]({ sessionID: "ses_1" }, second);
  assert.equal(second.parts.length, 1, "the same refs are not injected again");
  await hooks.event({ event: { type: "session.compacted", properties: { sessionID: "ses_1" } } });
  const third = message("where is attach_to_bone defined");
  await hooks["chat.message"]({ sessionID: "ses_1" }, third);
  assert.equal(third.parts.length, 2, "a compaction resets the list");
  const chat = message("thanks, looks good");
  await hooks["chat.message"]({ sessionID: "ses_2" }, chat);
  assert.equal(chat.parts.length, 1, "chat gets no capsule");
  await hooks["tool.execute.after"]({ tool: "edit", sessionID: "ses_1", callID: "c1", args: { filePath: join(root, "a.rs") } }, {});
});

test("the plugin does nothing and does not fail when the daemon does not run", async () => {
  const saved = process.env.ZCIE_URL;
  process.env.ZCIE_URL = "http://127.0.0.1:9";
  const root = mkdtempSync(join(tmpdir(), "zcie-opencode-down-"));
  const started = Date.now();
  const hooks = await ZciePlugin({ directory: root, worktree: root });
  process.env.ZCIE_URL = saved;
  assert.deepEqual(hooks, {});
  assert.equal(existsSync(join(root, ".claude", "zcie_id")), false);
  assert.ok(Date.now() - started < 3000);
});

test("the gate rules match the Claude Code hook", () => {
  const { shouldInject } = ZciePlugin.internal;
  const base = { answer_kind: "evidence", class: "search", answer: null, results: [{ ref: "sym:a", kind: "symbol", title: "x" }] };
  assert.equal(shouldInject({ ...base, match_coverage: 0.75 }, "late fee rate"), true);
  assert.equal(shouldInject({ ...base, match_coverage: 0.5 }, "hello"), false);
  assert.equal(shouldInject({ ...base, answer_kind: "not_found" }, "x"), false);
});
