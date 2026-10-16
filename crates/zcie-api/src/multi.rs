//! Queries over more than one project: global scope and projects named in the question (ADR-009).

use zcie_security::AuthContext;
use zcie_types::{AnswerKind, Freshness, QueryRequest, QueryResponse, QueryScope, ResultItem, WorkspaceRecord};

use crate::engine::Engine;
use crate::error::{ApiError, ApiResult};

/// Shortest project name that a question can name.
const MIN_NAME_CHARS: usize = 3;

impl Engine {
    /// Returns the projects that a query reads.
    pub(crate) fn query_targets(&self, auth: &AuthContext, request: &QueryRequest) -> ApiResult<Vec<WorkspaceRecord>> {
        if request.scope == QueryScope::Global {
            let all = self.permitted_projects(auth)?;
            if all.is_empty() {
                return Err(ApiError::NotFound("no project".into()));
            }
            return Ok(all);
        }
        let current = self.resolve_workspace(auth, &request.workspace, false)?;
        let mut targets = Vec::new();
        if request.cross_project {
            for project in self.permitted_projects(auth)? {
                if project.workspace_id != current.workspace_id && names_project(&request.question, &project.name) {
                    targets.push(project);
                }
            }
        }
        // Projects that the question names come first: the question asks about them.
        targets.push(current);
        Ok(targets)
    }
}

/// Returns true if the question names the project as a whole word (case-insensitive).
pub fn names_project(question: &str, name: &str) -> bool {
    let name = name.to_lowercase();
    if name.chars().count() < MIN_NAME_CHARS {
        return false;
    }
    question
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric() && c != '-' && c != '_')
        .any(|word| word == name || word.strip_suffix("'nin").or(word.strip_suffix("'da")).is_some_and(|w| w == name))
}

/// Runs the query in each project, labels the results with the project name and merges them.
pub(crate) fn query_many(engine: &Engine, auth: &AuthContext, request: &QueryRequest, targets: &[WorkspaceRecord]) -> ApiResult<QueryResponse> {
    let mut responses = Vec::new();
    for project in targets {
        let single = QueryRequest {
            workspace: project.workspace_id.clone(),
            scope: QueryScope::Project,
            cross_project: false,
            cursor: None,
            ..request.clone()
        };
        let ctx = engine.query_context(auth, project, None);
        let mut response = engine.query.run(&ctx, &single)?;
        for item in &mut response.results {
            item.project = Some(project.name.clone());
        }
        responses.push((project.name.clone(), response));
    }
    if request.scope == QueryScope::Global {
        let top = |r: &QueryResponse| r.results.first().map_or(f32::MIN, |i| i.score);
        responses.sort_by(|a, b| top(&b.1).total_cmp(&top(&a.1)));
    }
    Ok(merge(engine, request, responses))
}

fn merge(engine: &Engine, request: &QueryRequest, responses: Vec<(String, QueryResponse)>) -> QueryResponse {
    let budget = engine.query.budget(request.token_budget);
    let max_results = engine.query.max_results(request.max_results);
    let answers: Vec<String> = responses.iter().filter_map(|(name, r)| r.answer.as_ref().map(|a| format!("[{name}] {a}"))).collect();
    let mut items = interleave(&responses);
    items.truncate(max_results);
    let freshness = if responses.iter().any(|(_, r)| r.freshness == Freshness::Stale) {
        Freshness::Stale
    } else if responses.iter().any(|(_, r)| r.freshness == Freshness::Partial) {
        Freshness::Partial
    } else {
        Freshness::Current
    };
    let mut base = responses[0].1.clone();
    base.answer = (!answers.is_empty()).then(|| answers.join(" "));
    base.answer_kind = match (&base.answer, items.is_empty()) {
        (Some(_), _) => AnswerKind::Local,
        (None, true) => AnswerKind::NotFound,
        (None, false) => AnswerKind::Evidence,
    };
    base.memory_links = responses.iter().flat_map(|(_, r)| r.memory_links.clone()).take(max_results).collect();
    base.freshness = freshness;
    base.note = Some(format!("{} projects: {}", responses.len(), responses.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>().join(", ")));
    let mut response = zcie_context::build_capsule(base, items, budget, 0).response;
    // A merged response has no stored continuation. A caller asks again with a larger budget.
    response.cursor = None;
    response
}

/// Takes the results rank by rank: rank 1 of each project, then rank 2 of each project, and so on.
/// Scores of different projects are not comparable, so a score sort could fill the budget with
/// one project only.
fn interleave(responses: &[(String, QueryResponse)]) -> Vec<ResultItem> {
    let longest = responses.iter().map(|(_, r)| r.results.len()).max().unwrap_or(0);
    let mut items = Vec::new();
    for rank in 0..longest {
        items.extend(responses.iter().filter_map(|(_, r)| r.results.get(rank).cloned()));
    }
    items
}
