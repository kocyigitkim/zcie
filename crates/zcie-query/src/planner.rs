//! Query planner: cache, routes, ranking, freshness, local answers and the token firewall.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use zcie_cache::{CacheKeyParts, L0Cache, cache_key};
use zcie_context::clamp_budget;
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
    /// Context-efficiency switches (ADR-017).
    pub context: crate::capsule_rules::ContextOptions,
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
    pub(crate) vectors: Option<Arc<zcie_vector::Vectors>>,
    /// File graphs of the repository map, per workspace, with the search watermark of their build.
    pub(crate) repo_graphs: std::sync::Mutex<HashMap<String, (u64, Arc<crate::repomap::RepoGraph>)>>,
}

impl QueryEngine {
    /// Makes the read path.
    pub fn new(store: Arc<Store>, objects: Arc<ObjectStore>, search: Arc<SearchIndex>, cache: Arc<L0Cache>, ingest: Arc<Ingest>, config: QueryConfig) -> Self {
        Self { store, objects, search, cache, ingest, config, vectors: None, repo_graphs: std::sync::Mutex::new(HashMap::new()) }
    }

    /// Adds local vector search to the search route (ADR-011).
    pub fn with_vectors(mut self, vectors: Option<Arc<zcie_vector::Vectors>>) -> Self {
        self.vectors = vectors;
        self
    }

    /// Answers a query with cited evidence under the token budget (`run` adds the trace).
    pub(crate) fn answer(&self, ctx: &QueryContext, request: &QueryRequest) -> Result<QueryResponse> {
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
        if request.mode == zcie_types::QueryMode::Map {
            return self.map_answer(ctx, &question, watermarks, budget);
        }
        if request.mode == zcie_types::QueryMode::Locate || crate::locate::asks_for_location(&question) {
            return self.locate_answer(ctx, &question, watermarks, budget);
        }
        let is_symbol = |name: &str| self.is_symbol(ctx, name);
        let class = classify(&question, request.mode, &is_symbol);
        if let Some(listed) = self.list_all(ctx, (class, &request.question), watermarks, budget)? {
            return Ok(listed);
        }
        if let Some(card) = self.overview(ctx, (class, &request.question), watermarks, budget)? {
            return Ok(card);
        }
        let (ranked, note, overlay_hits) = self.ranked(ctx, class, &question)?;
        let parts = (class, note, request.question.as_str());
        let response = self.compose(ctx, parts, ranked, watermarks, overlay_hits, budget, max_results);
        self.store_in_cache(key, ctx, &response);
        Ok(response)
    }

    /// Returns the ranked candidates of a question, the note of its route and the overlay hits.
    fn ranked(&self, ctx: &QueryContext, class: QueryClass, question: &str) -> Result<(Vec<Candidate>, Option<String>, usize)> {
        let (mut candidates, note) = self.route(ctx, class, question)?;
        candidates.extend(self.graph_for_class(ctx, class, question)?);
        let (candidates, overlay_hits) = check_overlay(self, candidates);
        Ok((crate::dedupe::dedupe_text(rank(drop_echoes(candidates, question), class, question)), note, overlay_hits))
    }

    pub(crate) fn is_symbol(&self, ctx: &QueryContext, name: &str) -> bool {
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
            code_seq: self.code_seq(&ctx.scope.workspace_id),
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn compose(
        &self,
        ctx: &QueryContext,
        (class, note, question): (QueryClass, Option<String>, &str),
        ranked: Vec<Candidate>,
        watermarks: Watermarks,
        overlay_hits: usize,
        budget: u32,
        max_results: usize,
    ) -> QueryResponse {
        let answer = local_answer(class, &ranked);
        let memory_links = memory_links(&ranked, max_results);
        let compress = self.config.context.compress;
        let mut items: Vec<ResultItem> = ranked.into_iter().take(MAX_RANKED).map(|c| to_item(c, budget, question, compress)).collect();
        crate::followups::add_follow_ups(&self.store, &self.objects, &self.config.tenant, &mut items);
        let base = QueryResponse {
            query_id: new_id(prefix::QUERY),
            class,
            answer_kind: if answer.is_some() { AnswerKind::Local } else if items.is_empty() { AnswerKind::NotFound } else { AnswerKind::Evidence },
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
            // Set before the capsule is built, so that the token budget counts it.
            match_coverage: Some(crate::relevance::match_coverage(question, &items)),
            capsule: None,
            intent: None,
            evidence: Vec::new(),
            trace: None,
        };
        let (base, items) = crate::capsule_rules::apply(base, items, question, self.config.context);
        self.paginate(ctx, base, items, budget, max_results, 0)
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
/// Share of the index score in the lexical part of the rank; the rest is the term coverage.
const LEXICAL_SHARE: f32 = 0.3;
/// Lexical bonus of a commit for "which commit …" questions.
const COMMIT_BONUS: f32 = 0.3;

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
pub(crate) fn drop_echoes(candidates: Vec<Candidate>, question: &str) -> Vec<Candidate> {
    let echo = text_key(question);
    candidates
        .into_iter()
        .filter(|c| !matches!(c.loaded.item.kind.as_str(), "memory" | "event") || text_key(&c.loaded.item.title) != echo)
        .collect()
}

fn is_decision(candidate: &Candidate) -> bool {
    let label = candidate.loaded.item.evidence_status.as_deref().unwrap_or_default();
    ["design_decision", "preference", "correction", "constraint"].iter().any(|k| label.starts_with(k))
}

/// Lexical bonus by query class: decisions first for history questions, commits first for
/// "which commit …" questions.
/// The decision bonus is scaled by the term coverage of the decision: a decision that does not
/// match the question gets no bonus (it must not push the answer out). Requests (what the user
/// said) have no penalty: "what did the user want" questions are answered by them (ADR-013).
fn class_bonus(class: QueryClass, candidate: &Candidate, coverage: f32) -> f32 {
    let memory_first = matches!(class, QueryClass::Timeline | QueryClass::Task);
    match (memory_first, is_decision(candidate)) {
        (true, true) => DECISION_BONUS * coverage,
        _ if class == QueryClass::Implementation && candidate.loaded.item.kind == "commit" => COMMIT_BONUS,
        _ => 0.0,
    }
}

pub(crate) fn rank(candidates: Vec<Candidate>, class: QueryClass, question: &str) -> Vec<Candidate> {
    let memory_first = matches!(class, QueryClass::Timeline | QueryClass::Task);
    let terms = crate::relevance::stems(question);
    let now = time_now();
    let mut best: HashMap<String, Candidate> = HashMap::new();
    for mut candidate in candidates {
        // A corrected, disputed or retracted record keeps its own status, whatever path found it (ADR-020).
        let own = candidate.loaded.item.status.clone().filter(|s| matches!(s.as_str(), "superseded" | "disputed" | "retracted"));
        let status = own.as_deref().or(candidate.path_status.map(|s| s.name())).or(candidate.loaded.item.status.as_deref());
        let age_days = candidate.loaded.time.as_deref().and_then(parse_utc_seconds).map(|t| (now - t) as f32 / 86_400.0).unwrap_or(30.0);
        let coverage = crate::relevance::item_coverage(&terms, question, &candidate.loaded.item);
        let bonus = class_bonus(class, &candidate, coverage);
        let lexical = (LEXICAL_SHARE * candidate.lexical + (1.0 - LEXICAL_SHARE) * coverage + bonus).clamp(0.0, 1.0);
        let value = score(lexical, status_weight(status), recency_weight(age_days), kind_weight(&candidate.loaded.item.kind, memory_first)) * crate::items::source_factor(&candidate.loaded.item);
        candidate.loaded.item.score = (value * 1000.0).round() / 1000.0;
        if let Some(path_status) = candidate.path_status.filter(|_| own.is_none()) {
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

fn to_item(candidate: Candidate, budget: u32, question: &str, compress: bool) -> ResultItem {
    let mut item = candidate.loaded.item;
    let full_text = matches!(item.kind.as_str(), "memory" | "event");
    if budget < L1_BUDGET && !full_text {
        item.snippet = None;
    }
    // A long text keeps the sentences that the question needs (ADR-017); without compression, the
    // window with the most question terms.
    if full_text || item.kind == "file" {
        item.snippet = item.snippet.map(|s| match compress {
            true => crate::compress::compress(&s, question, crate::compress::MAX_ITEM_CHARS),
            false => crate::relevance::best_window(&s, question, crate::relevance::SNIPPET_WINDOW_CHARS),
        });
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
