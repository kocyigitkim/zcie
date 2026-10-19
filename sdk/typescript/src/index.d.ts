// Types of the ZCIE SDK (docs/09-query-and-context.md, docs/10-api-and-gateways.md).

export declare const DEFAULT_ENDPOINT: string;
export declare const EVENT_SCHEMA: string;
export declare const ID_FILE: string;

export declare class ZcieError extends Error {
  /** HTTP status; 0 for a transport error (for example, the daemon does not run). */
  readonly status: number;
  /** Error code of the daemon, for example `unauthorized` or `not_found`. */
  readonly code: string;
}

export declare function findProjectId(start?: string): string | undefined;

export type QueryMode = "auto" | "exact" | "search" | "timeline" | "memory";
export type ExpandLevel = "evidence" | "source" | "audit";
export type Freshness = "current" | "stale" | "partial";

export interface ResultItem {
  ref: string;
  kind: "symbol" | "file" | "memory" | "task" | "commit" | "event" | string;
  title: string;
  path: string | null;
  range: [number, number] | null;
  status: string | null;
  evidence_status: string | null;
  revision: string | null;
  score: number;
  snippet: string | null;
  /** Project name; set when a query reads more than one project. */
  project?: string;
}

export interface Watermarks {
  canonical_seq: number;
  graph_seq: number;
  search_seq: number;
}

export interface QueryResponse {
  query_id: string;
  class: string;
  answer_kind: "local" | "evidence" | "not_found";
  answer: string | null;
  results: ResultItem[];
  memory_links: string[];
  freshness: Freshness;
  watermarks: Watermarks;
  token_budget: number;
  token_estimate: number;
  truncated: boolean;
  cursor: string | null;
  note?: string;
  match_coverage?: number;
}

export interface QueryOptions {
  project?: string;
  mode?: QueryMode;
  tokenBudget?: number;
  maxResults?: number;
  cursor?: string;
  scope?: "project" | "global";
  /** Also read projects that the question names (default true). */
  crossProject?: boolean;
}

export interface ExpandItem {
  ref: string;
  status: "ok" | "changed" | "not_found" | "forbidden";
  revision: string | null;
  path: string | null;
  range: [number, number] | null;
  content: string | null;
  provenance: string[];
  /** The rest of a cut range: "path:start-end". */
  more?: string;
  /** Related lines added by `with`: "caller path:line text", "test path:line text". */
  related?: string[];
}

export interface ExpandResponse {
  items: ExpandItem[];
  freshness: Freshness;
  token_estimate: number;
  truncated: boolean;
  cursor: string | null;
}

export interface CaptureReceipt {
  event_id: string;
  seq: number;
  durable: boolean;
  indexed: boolean;
  duplicate: boolean;
  canonical_seq: number;
}

export interface CaptureInput {
  /** Event type, for example `user.message`, `file.changed`, `test.result`. */
  type: string;
  sessionId: string;
  payload?: string;
  turnId?: string;
  idempotencyKey?: string;
  app?: string;
  repositoryId?: string;
}

export interface ProjectInfo {
  project_id: string;
  name: string;
  roots: string[];
  created_utc: string;
}

export interface HealthReport {
  status: string;
  version: string;
  watermarks?: Watermarks;
  queue?: { interactive: number; background: number; dead_letter: number };
  storage_bytes?: number;
  uptime_s: number;
}

export interface ClientOptions {
  endpoint?: string;
  apiKey?: string;
  /** Project ID or name. */
  project?: string;
  timeoutMs?: number;
}

export declare class Client {
  constructor(options?: ClientOptions);
  readonly endpoint: string;
  readonly project: string;
  static fromEnv(env?: Record<string, string | undefined>, cwd?: string): Client;
  request<T = unknown>(method: string, path: string, body?: unknown): Promise<T>;
  health(): Promise<HealthReport>;
  capture(input: CaptureInput): Promise<CaptureReceipt>;
  registerRepository(path: string, project?: string): Promise<{ repository_id: string; worktree_id: string; scan_job_id: string; git_available: boolean }>;
  query(question: string, options?: QueryOptions): Promise<QueryResponse>;
  /** `refs` can hold line ranges "path:start-end"; `withRelated` adds "callers" and "tests" of symbols. */
  expand(refs: string[], level?: ExpandLevel, tokenBudget?: number, withRelated?: ("callers" | "tests")[]): Promise<ExpandResponse>;
  getEntity(ref: string): Promise<unknown>;
  getTimeline(ref: string): Promise<{ items: unknown[] }>;
  createTask(title: string, sourceEventId: string, externalRef?: string): Promise<unknown>;
  link(input: { from: string; relation: string; to: string; evidence?: "user_confirmation" | "client_trace"; sourceEventId: string; note?: string }): Promise<{ edge_id: string }>;
  correctMemory(memoryId: string, text: string, sourceEventId: string): Promise<unknown>;
  forget(selector: { ref?: string; session_id?: string; before_utc?: string }, reason: string): Promise<unknown>;
  rebuildSearch(): Promise<{ job_id: string }>;
  ensureProject(input?: { projectId?: string; name?: string; root?: string }): Promise<ProjectInfo>;
  listProjects(): Promise<{ projects: ProjectInfo[] }>;
  getProject(idOrName?: string): Promise<ProjectInfo>;
  renameProject(idOrName: string, name: string): Promise<ProjectInfo>;
  deleteProject(idOrName: string): Promise<ProjectInfo>;
}
