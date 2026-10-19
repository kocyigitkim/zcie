//! The query trace (ADR-019, D-066). With `explain: true`, the stages of the answer run again with
//! timers, and each candidate gets the reason why it is in the response or not. The answer itself is
//! not changed. A trace is never written to a log.

use std::collections::{HashMap, HashSet};
use std::time::Instant;

use zcie_types::{QueryClass, QueryRequest, QueryResponse, QueryTrace, TraceCandidate, TraceCount, TraceStages, truncate_chars};

use crate::classify::normalize;
use crate::error::Result;
use crate::planner::{QueryContext, QueryEngine, drop_echoes, rank};
use crate::routes::Candidate;

/// Characters of a title in a trace.
const TITLE_CHARS: usize = 60;
/// Most candidate lines in a trace.
const MAX_TRACE_CANDIDATES: usize = 60;

fn elapsed_ms(started: Instant) -> f64 {
    (started.elapsed().as_secs_f64() * 1000.0 * 100.0).round() / 100.0
}

fn line(candidate: &Candidate, source: &str, score: f32, reason: &str) -> TraceCandidate {
    let item = &candidate.loaded.item;
    TraceCandidate {
        entity_ref: item.entity_ref.clone(),
        kind: item.kind.clone(),
        title: truncate_chars(&item.title, TITLE_CHARS),
        source: source.to_string(),
        score,
        reason: reason.to_string(),
    }
}

impl QueryEngine {
    /// Answers a query with cited evidence under the token budget. With `explain`, the response
    /// also has the trace of the query.
    pub fn run(&self, ctx: &QueryContext, request: &QueryRequest) -> Result<QueryResponse> {
        let started = Instant::now();
        let mut response = self.answer(ctx, request)?;
        if request.cursor.is_none() {
            self.add_evidence(ctx, request, &mut response)?;
        }
        if request.explain && request.cursor.is_none() {
            response.trace = Some(self.trace(ctx, request, &response, elapsed_ms(started))?);
        }
        Ok(response)
    }

    /// Returns the graph candidates of a class. Graph ranking is for free questions and task
    /// chains; the exact classes have their own routes and answers. A task question keeps its one
    /// task (the task answer needs it).
    pub(crate) fn graph_for_class(&self, ctx: &QueryContext, class: QueryClass, question: &str) -> Result<Vec<Candidate>> {
        if !matches!(class, QueryClass::Search | QueryClass::Timeline | QueryClass::Task) {
            return Ok(Vec::new());
        }
        let graph = self.graph_candidates(ctx, question)?;
        Ok(graph.into_iter().filter(|c| class != QueryClass::Task || c.loaded.item.kind != "task").collect())
    }

    fn trace(&self, ctx: &QueryContext, request: &QueryRequest, response: &QueryResponse, answer_ms: f64) -> Result<QueryTrace> {
        let question = normalize(&request.question);
        let mut trace = QueryTrace {
            class: serde_json::to_value(response.class).ok().and_then(|v| v.as_str().map(str::to_string)).unwrap_or_default(),
            mode: request.mode.name().to_string(),
            intent: response.intent.map(|i| i.name().to_string()),
            terms: crate::relevance::stems(&question).into_iter().collect(),
            stages: TraceStages { answer_ms, ..TraceStages::default() },
            ..QueryTrace::default()
        };
        trace.terms.sort();
        // A special answer (map, locate, project card, decision list) has no route stage.
        if is_special(response) {
            trace.candidates = response.results.iter().map(special_line).collect();
            return Ok(trace);
        }
        let started = Instant::now();
        let (route, _) = self.route(ctx, response.class, &question)?;
        trace.stages.route_ms = elapsed_ms(started);
        let started = Instant::now();
        let graph = self.graph_for_class(ctx, response.class, &question)?;
        trace.stages.graph_ms = elapsed_ms(started);
        trace.sources = vec![TraceCount { source: "route".into(), count: route.len() }, TraceCount { source: "graph".into(), count: graph.len() }];
        let started = Instant::now();
        trace.candidates = reasons(&route, &graph, response, &question);
        trace.stages.rank_ms = elapsed_ms(started);
        Ok(trace)
    }
}

/// True for a response of a special answer (map, locate, project card, decision list).
fn is_special(response: &QueryResponse) -> bool {
    let note = response.note.as_deref().unwrap_or_default();
    [crate::repomap::MAP_NOTE, crate::locate::LOCATE_NOTE, crate::overview::OVERVIEW_NOTE].contains(&note) || note.starts_with("all decisions and rules")
}

fn special_line(item: &zcie_types::ResultItem) -> TraceCandidate {
    TraceCandidate {
        entity_ref: item.entity_ref.clone(),
        kind: item.kind.clone(),
        title: truncate_chars(&item.title, TITLE_CHARS),
        source: "answer".into(),
        score: item.score,
        reason: "kept".into(),
    }
}

/// Gives each candidate of the route and the graph its final score and its reason.
fn reasons(route: &[Candidate], graph: &[Candidate], response: &QueryResponse, question: &str) -> Vec<TraceCandidate> {
    let all: Vec<Candidate> = route.iter().chain(graph).cloned().collect();
    let kept_echo: HashSet<String> = drop_echoes(all.clone(), question).iter().map(|c| c.loaded.item.entity_ref.clone()).collect();
    let ranked = rank(all, response.class, question);
    let scores: HashMap<String, f32> = ranked.iter().map(|c| (c.loaded.item.entity_ref.clone(), c.loaded.item.score)).collect();
    let unique: HashSet<String> = crate::dedupe::dedupe_text(ranked).iter().map(|c| c.loaded.item.entity_ref.clone()).collect();
    let shown: HashSet<&str> = response.results.iter().map(|r| r.entity_ref.as_str()).collect();
    let tagged = route.iter().map(|c| (c, "route")).chain(graph.iter().map(|c| (c, "graph")));
    let mut seen = HashSet::new();
    let mut out: Vec<TraceCandidate> = tagged
        .filter(|(c, source)| seen.insert((c.loaded.item.entity_ref.clone(), *source)))
        .map(|(c, source)| {
            let item_ref = c.loaded.item.entity_ref.as_str();
            let reason = if shown.contains(item_ref) {
                "kept"
            } else if !kept_echo.contains(item_ref) {
                "echo"
            } else if !unique.contains(item_ref) {
                "duplicate"
            } else {
                "below_cut"
            };
            line(c, source, scores.get(item_ref).copied().unwrap_or(0.0), reason)
        })
        .collect();
    out.sort_by(|a, b| b.score.total_cmp(&a.score).then_with(|| a.entity_ref.cmp(&b.entity_ref)));
    out.truncate(MAX_TRACE_CANDIDATES);
    out
}
