// ZCIE TypeScript/JavaScript SDK. It wraps the HTTP API of the local daemon
// (docs/10-api-and-gateways.md section 3). No dependencies: it uses the global `fetch`
// (Node 18 or later, Bun, Deno, browsers). Types are in index.d.ts.

import { existsSync, readFileSync } from "node:fs";
import { homedir } from "node:os";
import { dirname, join } from "node:path";

/** Default daemon address. */
export const DEFAULT_ENDPOINT = "http://127.0.0.1:47831";
/** Event schema of capture requests. */
export const EVENT_SCHEMA = "zcie.event.v1";
/** Relative path of the project ID file (ADR-009). */
export const ID_FILE = ".claude/zcie_id";

/** An error answer of the daemon, or a transport error (status 0). */
export class ZcieError extends Error {
  constructor(status, code, message) {
    super(`${code} (${status}): ${message}`);
    this.name = "ZcieError";
    this.status = status;
    this.code = code;
  }
}

/**
 * Finds `.claude/zcie_id` in `start` or a parent folder. Returns the project ID or undefined.
 * The search stops at a Git repository root: a repository inside another project folder is its own project.
 */
export function findProjectId(start = process.cwd()) {
  let folder = start;
  for (;;) {
    try {
      const id = readFileSync(join(folder, ID_FILE), "utf8").split("\n")[0].trim();
      if (/^(proj|ws)_[0-9A-Z]{26}$/.test(id)) return id;
    } catch {
      // No ID file in this folder.
    }
    if (existsSync(join(folder, ".git"))) return undefined;
    const parent = dirname(folder);
    if (parent === folder) return undefined;
    folder = parent;
  }
}

function readKey(env) {
  if (env.ZCIE_API_KEY) return env.ZCIE_API_KEY;
  const files = [env.ZCIE_API_KEY_FILE, join(homedir(), ".zcie", "config", "admin.key")].filter(Boolean);
  for (const file of files) {
    try {
      return readFileSync(file, "utf8").split("\n")[0].trim();
    } catch {
      // Try the next file.
    }
  }
  return "";
}

/** A client of one daemon and one project. */
export class Client {
  /**
   * @param {{endpoint?: string, apiKey?: string, project?: string, timeoutMs?: number}} [options]
   */
  constructor(options = {}) {
    this.endpoint = (options.endpoint ?? DEFAULT_ENDPOINT).replace(/\/$/, "");
    this.apiKey = options.apiKey ?? "";
    this.project = options.project ?? "default";
    this.timeoutMs = options.timeoutMs ?? 30_000;
  }

  /**
   * Makes a client from the environment: ZCIE_URL, ZCIE_API_KEY or ZCIE_API_KEY_FILE (default:
   * ~/.zcie/config/admin.key), ZCIE_WORKSPACE or the `.claude/zcie_id` of the current folder.
   */
  static fromEnv(env = process.env, cwd = process.cwd()) {
    const project = env.ZCIE_WORKSPACE || findProjectId(cwd) || "default";
    return new Client({ endpoint: env.ZCIE_URL || DEFAULT_ENDPOINT, apiKey: readKey(env), project });
  }

  /** Sends one request. Throws ZcieError on a non-2xx answer. */
  async request(method, path, body) {
    const headers = { "Content-Type": "application/json" };
    if (this.apiKey) headers.Authorization = `Bearer ${this.apiKey}`;
    let response;
    try {
      response = await fetch(this.endpoint + path, {
        method,
        headers,
        body: body === undefined ? undefined : JSON.stringify(body),
        signal: AbortSignal.timeout(this.timeoutMs),
      });
    } catch (error) {
      throw new ZcieError(0, "transport", String(error?.message ?? error));
    }
    const text = await response.text();
    if (!response.ok) {
      let parsed;
      try {
        parsed = JSON.parse(text).error;
      } catch {
        parsed = undefined;
      }
      throw new ZcieError(response.status, parsed?.code ?? "http_error", parsed?.message ?? text);
    }
    return text ? JSON.parse(text) : null;
  }

  health() {
    return this.request("GET", "/v1/health");
  }

  /** Captures one event. `idempotencyKey` makes a retry safe. */
  capture({ type, sessionId, payload, turnId, idempotencyKey, app = "sdk-js", repositoryId }) {
    const key = idempotencyKey ?? `${app}:${sessionId}:${turnId ?? Date.now()}:${Math.random().toString(36).slice(2)}`;
    return this.request("POST", "/v1/events", {
      schema: EVENT_SCHEMA,
      idempotency_key: key,
      workspace: this.project,
      repository_id: repositoryId,
      source: { app, session_id: sessionId, turn_id: turnId },
      type,
      payload,
    });
  }

  registerRepository(path, project) {
    return this.request("POST", "/v1/repositories", { workspace: this.project, path, project });
  }

  /** Runs a query. `options`: mode, tokenBudget, maxResults, scope ("project" | "global"), crossProject, cursor. */
  query(question, options = {}) {
    return this.request("POST", "/v1/query", {
      workspace: options.project ?? this.project,
      question,
      mode: options.mode,
      token_budget: options.tokenBudget,
      max_results: options.maxResults,
      cursor: options.cursor,
      scope: options.scope,
      cross_project: options.crossProject ?? true,
    });
  }

  expand(refs, level = "evidence", tokenBudget, withRelated) {
    const body = { workspace: this.project, refs, level, token_budget: tokenBudget };
    if (withRelated && withRelated.length) body.with = withRelated;
    return this.request("POST", "/v1/expand", body);
  }

  getEntity(ref) {
    return this.request("GET", `/v1/entities/${encodeURIComponent(ref)}`);
  }

  getTimeline(ref) {
    return this.request("GET", `/v1/timeline/${encodeURIComponent(ref)}`);
  }

  createTask(title, sourceEventId, externalRef) {
    return this.request("POST", "/v1/tasks", { workspace: this.project, title, source_event_id: sourceEventId, external_ref: externalRef });
  }

  link({ from, relation, to, evidence = "user_confirmation", sourceEventId, note }) {
    return this.request("POST", "/v1/links", { workspace: this.project, from, relation, to, evidence, source_event_id: sourceEventId, note });
  }

  correctMemory(memoryId, text, sourceEventId) {
    return this.request("POST", `/v1/memory/${encodeURIComponent(memoryId)}/correct`, { text, source_event_id: sourceEventId });
  }

  forget(selector, reason) {
    return this.request("POST", "/v1/forget", { workspace: this.project, selector, reason });
  }

  rebuildSearch() {
    return this.request("POST", "/v1/admin/rebuild-search");
  }

  ensureProject({ projectId, name, root } = {}) {
    return this.request("POST", "/v1/projects", { project_id: projectId, name, root });
  }

  listProjects() {
    return this.request("GET", "/v1/projects");
  }

  getProject(idOrName = this.project) {
    return this.request("GET", `/v1/projects/${encodeURIComponent(idOrName)}`);
  }

  renameProject(idOrName, name) {
    return this.request("POST", `/v1/projects/${encodeURIComponent(idOrName)}/rename`, { name });
  }

  deleteProject(idOrName) {
    return this.request("DELETE", `/v1/projects/${encodeURIComponent(idOrName)}`);
  }
}
