// Runs against a running daemon: ZCIE_URL, ZCIE_API_KEY (admin) and ZCIE_TEST_REPO (a fixture repository).
import { test } from "node:test";
import assert from "node:assert/strict";
import { mkdtempSync, mkdirSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { Client, ZcieError, findProjectId } from "../src/index.js";

const env = process.env;
const client = new Client({ endpoint: env.ZCIE_URL, apiKey: env.ZCIE_API_KEY, project: "sdk-js" });

async function waitFor(check) {
  for (let i = 0; i < 300; i++) {
    if (await check()) return;
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  assert.fail("condition not reached");
}

test("health, project, repository, query and expand", async () => {
  assert.equal((await client.health()).status, "ok");
  const project = await client.ensureProject({ name: "sdk-js" });
  assert.match(project.project_id, /^proj_/);
  await client.registerRepository(env.ZCIE_TEST_REPO);
  let found;
  await waitFor(async () => {
    found = await client.query("where is attach_to_bone defined", { tokenBudget: 250 });
    return found.answer !== null;
  });
  assert.equal(found.class, "symbol_definition");
  const ref = found.results.find((r) => r.kind === "symbol").ref;
  const expanded = await client.expand([ref], "source", 1000);
  assert.equal(expanded.items[0].status, "ok");
  assert.match(expanded.items[0].content, /attach_to_bone/);
});

test("capture is idempotent and the memory is found", async () => {
  const input = { type: "user.message", sessionId: "s-js", turnId: "1", idempotencyKey: "js:1", payload: "Decision: we use quaternions for bones." };
  const first = await client.capture(input);
  const again = await client.capture(input);
  assert.equal(again.event_id, first.event_id);
  assert.equal(again.duplicate, true);
  await waitFor(async () => (await client.query("quaternions for bones", { mode: "memory" })).results.length > 0);
});

test("errors carry status and code", async () => {
  const bad = new Client({ endpoint: env.ZCIE_URL, apiKey: "zcie_wrong", project: "sdk-js" });
  await assert.rejects(bad.health().then(() => bad.listProjects()), (error) => error instanceof ZcieError && error.status === 401);
  const down = new Client({ endpoint: "http://127.0.0.1:9", apiKey: "x", timeoutMs: 2000 });
  await assert.rejects(down.health(), (error) => error instanceof ZcieError && error.status === 0);
});

test("findProjectId and fromEnv read .claude/zcie_id", () => {
  const root = mkdtempSync(join(tmpdir(), "zcie-js-"));
  mkdirSync(join(root, ".claude"));
  mkdirSync(join(root, "src", "deep"), { recursive: true });
  writeFileSync(join(root, ".claude", "zcie_id"), "proj_01M52Y8GKM6NJ53WCSMFY4W55A\n");
  assert.equal(findProjectId(join(root, "src", "deep")), "proj_01M52Y8GKM6NJ53WCSMFY4W55A");
  assert.equal(Client.fromEnv({ ZCIE_API_KEY: "k" }, join(root, "src")).project, "proj_01M52Y8GKM6NJ53WCSMFY4W55A");
  assert.equal(Client.fromEnv({ ZCIE_API_KEY: "k", ZCIE_WORKSPACE: "other" }, root).project, "other");
});
