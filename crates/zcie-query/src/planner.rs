//! Query planner: cache, routes, ranking, freshness, local answers and the token firewall.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use zcie_cache::{CacheKeyParts, L0Cache, cache_key};
use zcie_context::{build_capsule, clamp_budget, parse_cursor};
use zcie_ingest::Ingest;
use zcie_search::SearchIndex;
use zcie_search::freshness::freshness;
use zcie_search::ranking::{kind_weight, recency_weight, score, status_weight};
use zcie_security::AuthContext;
use zcie_store::watermarks::read_watermarks;
use zcie_store::{ObjectStore, Store};
use zcie_types::ids::{new_id, prefix};
use zcie_types::scope::parse_utc_seconds;
use zcie_types::{AnswerKind, Freshness, QueryClass, QueryRequest, QueryResponse, ResultItem, Scope, Watermarks};

use crate::classify::{classify, normalize};
use crate::error::{Error, Result};
use crate::freshness::check_overlay;
use crate::routes::Candidate;

/// Longest permitted question.
pub const MAX_QUESTION_CHARS: usize = 2000;
/// Budget at which results include snippets (level L1).
pub const L1_BUDGET: u32 = 400;
/// Maximum ranked results kept for cursor pages.
const MAX_RANKED: usize = 100;

/// Settings of the read path.
#[derive(Debug, Clone)]
pub struct QueryConfig {
    /// Tenant ID.
    pub tenant: String,
    /// Default token budget.
    pub default_budget: u32,
    /// Maximum token budget.
    pub max_budget: u32,
    /// Default maximum results.
    pub max_results: usize,
    /// Maximum traversal depth.
    pub graph_max_depth: usize,
    /// Maximum edges per node.
    pub graph_max_fanout: usize,
    /// Lifetime of a cached "not found" response.
    pub negative_ttl: Duration,
}

/// The authenticated request scope.
#[derive(Debug, Clone)]
pub struct QueryContext {
    /// The auth context of the client.
    pub auth: AuthContext,
    /// The resolved request scope (workspace ID, optional repository).
    pub scope: Scope,
}

/// The read path.
pub struct QueryEngine {
    pub(crate) store: Arc<Store>,
    pub(crate) objects: Arc<ObjectStore>,
    pub(crate) search: Arc<SearchIndex>,
    pub(crate) cache: Arc<L0Cache>,
    pub(crate) ingest: Arc<Ingest>,
    pub(crate) config: QueryConfig,
}

impl QueryEngine {
    /// Makes the read path.
    pub fn new(store: Arc<Store>, objects: Arc<ObjectStore>, search: Arc<SearchIndex>, cache: Arc<L0Cache>, ingest: Arc<Ingest>, config: QueryConfig) -> Self {
        Self { store, objects, search, cache, ingest, config }
    }

    /// Answers a query with cited evidence under the token budget.
    pub fn run(&self, ctx: &QueryContext, request: &QueryRequest) -> Result<QueryResponse> {
        if request.question.chars().count() > MAX_QUESTION_CHARS || request.question.trim().is_empty() {
            return Err(Error::Invalid("question must have 1 to 2000 characters".into()));
        }
        let budget = clamp_budget(request.token_budget, self.config.default_budget, self.config.max_budget);
        if let Some(cursor) = &request.cursor {
            return self.next_page(ctx, cursor, budget);
        }
        let max_results = request.max_results.map(|m| m as usize).unwrap_or(self.config.max_results).clamp(1, 50);
        let watermarks = read_watermarks(&self.store, &self.config.tenant)?;
        let question = normalize(&request.question);
        let key = self.key(ctx, request, &question, budget, max_results, &watermarks);
        if let Some(hit) = self.cache.get(&key) {
            self.ingest.counters.cache_hits.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            return Ok(hit);
        }
        let is_symbol = |name: &str| self.is_symbol(ctx, name);
        let class = classify(&question, request.mode, &is_symbol);
        let (candidates, note) = self.route(ctx, class, &question)?;
        let (candidates, overlay_hits) = check_overlay(self, candidates);
        let ranked = dedupe_text(rank(drop_echoes(candidates, &question), class));
        let response = self.compose(ctx, class, ranked, watermarks, overlay_hits, budget, max_results, note);
        self.store_in_cache(key, ctx, &response);
        Ok(response)
    }

    fn is_symbol(&self, ctx: &QueryContext, name: &str) -> bool {
        zcie_symbols::find_by_name(&self.store, &self.config.tenant, &ctx.scope.workspace_id, name, 1)
            .map(|found| !found.is_empty())
            .unwrap_or(false)
    }

    fn key(&self, ctx: &QueryContext, request: &QueryRequest, question: &str, budget: u32, max_results: usize, wm: &Watermarks) -> String {
        cache_key(&CacheKeyParts {
            tenant_id: &self.config.tenant,
            principal_id: &ctx.auth.principal_id,
            workspace_id: &ctx.scope.workspace_id,
            repository_id: ctx.scope.repository_id.as_deref().unwrap_or_default(),
            policy_version: ctx.auth.policy_version,
            question,
            mode: request.mode.name(),
            token_budget: budget,
            max_results: max_results as u32,
            canonical_seq: wm.canonical_seq,
            graph_seq: wm.graph_seq,
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn compose(
        &self,
        ctx: &QueryContext,
        class: QueryClass,
        ranked: Vec<Candidate>,
        watermarks: Watermarks,
        overlay_hits: usize,
        budget: u32,
        max_results: usize,
        note: Option<String>,
    ) -> QueryResponse {
        let answer = local_answer(class, &ranked);
        let memory_links = memory_links(&ranked, max_results);
        let items: Vec<ResultItem> = ranked.into_iter().take(MAX_RANKED).map(|c| to_item(c, budget)).collect();
        let answer_kind = match (&answer, items.is_empty()) {
            (Some(_), _) => AnswerKind::Local,
            (None, true) => AnswerKind::NotFound,
            (None, false) => AnswerKind::Evidence,
        };
        let base = QueryResponse {
            query_id: new_id(prefix::QUERY),
            class,
            answer_kind,
            answer,
            results: Vec::new(),
            memory_links,
            freshness: self.freshness_label(&watermarks, overlay_hits),
            watermarks,
            token_budget: budget,
            token_estimate: 0,
            truncated: false,
            cursor: None,
            note,
        };
        self.paginate(ctx, base, items, budget, max_results, 0)
    }

    fn paginate(&self, ctx: &QueryContext, base: QueryResponse, mut items: Vec<ResultItem>, budget: u32, max_results: usize, offset: usize) -> QueryResponse {
        let rest = if items.len() > max_results { items.split_off(max_results) } else { Vec::new() };
        let capsule = build_capsule(base, items, budget, offset);
        let mut response = capsule.response;
        let mut remaining = capsule.remaining;
        remaining.extend(rest);
        if !remaining.is_empty() {
            let next_offset = offset + response.results.len();
            response.truncated = true;
            response.cursor = Some(zcie_context::make_cursor(&response.query_id, next_offset));
            let mut page_base = response.clone();
            page_base.results.clear();
            self.cache.put_page(&response.query_id, page_base, remaining, &ctx.scope.workspace_id, &ctx.auth.principal_id);
        }
        response
    }

    fn next_page(&self, ctx: &QueryContext, cursor: &str, budget: u32) -> Result<QueryResponse> {
        let (query_id, offset) = parse_cursor(cursor).ok_or_else(|| Error::Invalid("bad cursor".into()))?;
        let page = self.cache.get_page(&query_id).ok_or_else(|| Error::NotFound("cursor expired".into()))?;
        if page.principal_id != ctx.auth.principal_id || page.workspace_id != ctx.scope.workspace_id {
            return Err(Error::Forbidden);
        }
        let max_results = self.config.max_results;
        Ok(self.paginate(ctx, page.base, page.remaining, budget, max_results, offset))
    }

    fn store_in_cache(&self, key: String, ctx: &QueryContext, response: &QueryResponse) {
        match (response.answer_kind, response.freshness) {
            (AnswerKind::NotFound, _) => self.cache.put(key, &ctx.scope.workspace_id, response.clone(), Some(self.config.negative_ttl)),
            (_, Freshness::Current) if !response.truncated => self.cache.put(key, &ctx.scope.workspace_id, response.clone(), None),
            _ => {}
        }
    }
}

fn memory_links(ranked: &[Candidate], max_results: usize) -> Vec<String> {
    ranked
        .iter()
        .filter(|c| matches!(c.loaded.item.kind.as_str(), "task" | "memory"))
        .map(|c| c.loaded.item.entity_ref.clone())
        .take(max_results)
        .collect()
}

/// Summarizes one task with its verified implementation links only.
fn task_answer(ranked: &[Candidate]) -> Option<String> {
    let tasks: Vec<&Candidate> = ranked.iter().filter(|c| c.loaded.item.kind == "task").collect();
    let [task] = tasks.as_slice() else { return None };
    let verified = |kind: &str| -> Vec<&Candidate> {
        ranked.iter().filter(|c| c.loaded.item.kind == kind && c.path_status == Some(zcie_types::EdgeStatus::Verified)).collect()
    };
    let item = &task.loaded.item;
    let mut answer = format!("Task {} \"{}\" is {}.", item.entity_ref, item.title, item.status.clone().unwrap_or_default());
    let commits: Vec<String> = verified("commit").iter().map(|c| format!("{} ({})", c.loaded.item.revision.clone().unwrap_or_default(), c.loaded.item.title)).collect();
    if !commits.is_empty() {
        answer.push_str(&format!(" Implemented by commit {} (verified).", commits.join(", ")));
    }
    let files: Vec<String> = verified("file").iter().filter_map(|c| c.loaded.item.path.clone()).collect();
    if !files.is_empty() {
        answer.push_str(&format!(" Files: {}.", files.join(", ")));
    }
    Some(answer)
}

/// Merges duplicate refs, computes scores and sorts.
/// Characters compared to find an echo of the question or a duplicate text.
const ECHO_CHARS: usize = 60;
/// Lexical bonus of a decision, preference or correction record in memory-first classes.
const DECISION_BONUS: f32 = 0.6;
/// Lexical penalty of a plain user-request record in memory-first classes.
const REQUEST_PENALTY: f32 = 0.4;

/// Returns the comparable start of a message text. An event prefix (`user.message: ` or
/// `user.message `) is removed.
pub(crate) fn text_key(text: &str) -> String {
    let text = text.trim();
    let text = match text.split_once([':', ' ']) {
        Some((head, rest)) if head.contains('.') && !head.contains('/') => rest.trim_start_matches([':', ' ']),
        _ => text,
    };
    zcie_types::truncate_chars(&text.to_lowercase(), ECHO_CHARS)
}

/// Drops memory and event results that only repeat the question.
fn drop_echoes(candidates: Vec<Candidate>, question: &str) -> Vec<Candidate> {
    let echo = text_key(question);
    candidates
        .into_iter()
        .filter(|c| !matches!(c.loaded.item.kind.as_str(), "memory" | "event") || text_key(&c.loaded.item.title) != echo)
        .collect()
}

/// Keeps one result per text (repeated messages and their memory records). A memory record is
/// kept before the event it came from, because it carries status and provenance.
fn dedupe_text(ranked: Vec<Candidate>) -> Vec<Candidate> {
    let memory_texts: std::collections::HashSet<String> = ranked
        .iter()
        .filter(|c| c.loaded.item.kind == "memory")
        .map(|c| text_key(&c.loaded.item.title))
        .collect();
    let mut seen = std::collections::HashSet::new();
    ranked
        .into_iter()
        .filter(|c| match c.loaded.item.kind.as_str() {
            "event" => !memory_texts.contains(&text_key(&c.loaded.item.title)),
            "memory" => seen.insert(text_key(&c.loaded.item.title)),
            _ => true,
        })
        .collect()
}

fn is_decision(candidate: &Candidate) -> bool {
    let label = candidate.loaded.item.evidence_status.as_deref().unwrap_or_default();
    ["design_decision", "preference", "correction", "constraint"].iter().any(|k| label.starts_with(k))
}

fn rank(candidates: Vec<Candidate>, class: QueryClass) -> Vec<Candidate> {
    let memory_first = matches!(class, QueryClass::Timeline | QueryClass::Task);
    let now = time_now();
    let mut best: HashMap<String, Candidate> = HashMap::new();
    for mut candidate in candidates {
        let status = candidate.path_status.map(|s| s.name()).or(candidate.loaded.item.status.as_deref());
        let age_days = candidate.loaded.time.as_deref().and_then(parse_utc_seconds).map(|t| (now - t) as f32 / 86_400.0).unwrap_or(30.0);
        let label = candidate.loaded.item.evidence_status.clone().unwrap_or_default();
        let bonus = match (memory_first, is_decision(&candidate), label.starts_with("user_requirement")) {
            (true, true, _) => DECISION_BONUS,
            (true, false, true) => -REQUEST_PENALTY,
            _ => 0.0,
        };
        let lexical = (candidate.lexical + bonus).clamp(0.0, 1.0);
        let value = score(lexical, status_weight(status), recency_weight(age_days), kind_weight(&candidate.loaded.item.kind, memory_first));
        candidate.loaded.item.score = (value * 1000.0).round() / 1000.0;
        if let Some(path_status) = candidate.path_status {
            candidate.loaded.item.status = Some(path_status.name().to_string());
        }
        let key = candidate.loaded.item.entity_ref.clone();
        let keep = best.get(&key).is_none_or(|old| old.loaded.item.score < candidate.loaded.item.score);
        if keep {
            best.insert(key, candidate);
        }
    }
    let mut out: Vec<Candidate> = best.into_values().collect();
    out.sort_by(|a, b| b.loaded.item.score.total_cmp(&a.loaded.item.score).then_with(|| a.loaded.item.entity_ref.cmp(&b.loaded.item.entity_ref)));
    out
}

fn time_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn to_item(candidate: Candidate, budget: u32) -> ResultItem {
    let mut item = candidate.loaded.item;
    let full_text = matches!(item.kind.as_str(), "memory" | "event");
    if budget < L1_BUDGET && !full_text {
        item.snippet = None;
    }
    item
}

/// Makes a short answer only from exact or verified data (`docs/09-query-and-context.md` section 10).
fn local_answer(class: QueryClass, ranked: &[Candidate]) -> Option<String> {
    match class {
        QueryClass::SymbolDefinition => {
            let symbols: Vec<&Candidate> = ranked.iter().filter(|c| c.loaded.item.kind == "symbol").collect();
            let [symbol] = symbols.as_slice() else { return None };
            let item = &symbol.loaded.item;
            let range = item.range?;
            Some(format!("{} is defined in {}:{}-{} (revision {}).", item.title, item.path.clone()?, range[0], range[1], item.revision.clone().unwrap_or_default()))
        }
        QueryClass::Implementation => {
            let tasks: Vec<&Candidate> = ranked.iter().filter(|c| c.loaded.item.kind == "task").collect();
            let commits: Vec<&Candidate> = ranked.iter().filter(|c| c.loaded.item.kind == "commit" && c.loaded.item.status.as_deref() == Some("verified")).collect();
            let ([task], [commit]) = (tasks.as_slice(), commits.as_slice()) else { return None };
            let files: Vec<String> = ranked.iter().filter(|c| c.loaded.item.kind == "file" && c.loaded.item.status.as_deref() == Some("verified")).filter_map(|c| c.loaded.item.path.clone()).collect();
            Some(format!("Task {} is implemented by commit {}, files: {}.", task.loaded.item.entity_ref, commit.loaded.item.revision.clone().unwrap_or_default(), files.join(", ")))
        }
        QueryClass::Task => task_answer(ranked),
        QueryClass::ExactRef => ranked.first().map(|c| {
            let item = &c.loaded.item;
            format!("{} {}, status {}.", item.kind, item.title, item.status.clone().unwrap_or_else(|| "unknown".into()))
        }),
        _ => None,
    }
}

impl QueryEngine {
    /// Returns the freshness label. Uncommitted search operations make the answer partial.
    fn freshness_label(&self, watermarks: &Watermarks, overlay_hits: usize) -> Freshness {
        match freshness(watermarks, overlay_hits, 0) {
            Freshness::Current if self.ingest.search.pending() > 0 => Freshness::Partial,
            label => label,
        }
    }

    /// Returns the token budget that a request gets.
    pub fn budget(&self, requested: Option<u32>) -> u32 {
        clamp_budget(requested, self.config.default_budget, self.config.max_budget)
    }

    /// Returns the result limit that a request gets.
    pub fn max_results(&self, requested: Option<u32>) -> usize {
        requested.map(|m| m as usize).unwrap_or(self.config.max_results).clamp(1, 50)
    }

    /// Returns the maximum edges read from one node.
    pub fn fanout(&self) -> usize {
        self.config.graph_max_fanout
    }
}
