//! Retrieval routes (`docs/09-query-and-context.md` section 4).

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use zcie_graph::{Direction, TraverseOptions};
use zcie_memory::extraction::tier_a::find_matches;
use zcie_search::{IndexName, SearchHit, SearchQuery};
use zcie_store::Batch;
use zcie_types::{EdgeStatus, FileRecord, QueryClass, Relation};

use crate::error::Result;
use crate::items::{Loaded, load_ref};
use crate::planner::{QueryContext, QueryEngine};

/// One ranked candidate.
#[derive(Debug, Clone)]
pub struct Candidate {
    /// The loaded entity.
    pub loaded: Loaded,
    /// Normalized lexical score (0.0 to 1.0).
    pub lexical: f32,
    /// Status of the path that found the candidate, if any.
    pub path_status: Option<EdgeStatus>,
}

/// Lexical factor of code and document hits in a history question (ADR-008 I-31).
const CODE_IN_TIMELINE: f32 = 0.5;
/// Lowest lexical score of a decision with one matching word.
const DECISION_BASE: f32 = 0.5;
/// Question words that do not identify a topic.
const QUESTION_WORDS: &[&str] = &[
    "what", "did", "decide", "decided", "decision", "earlier", "about", "and", "why", "answer", "sentence", "sentences",
    "two", "one", "the", "our", "project", "rule", "agreed", "there", "any", "was", "were", "have", "has", "for", "with",
    "daha", "önce", "hakkında", "karar", "vermiştik", "neden", "cevapla", "iki", "cümleyle", "kural", "nedir", "ne",
];

/// Lowercase words with 3 or more characters that are not question words.
fn content_words(question: &str) -> Vec<String> {
    let mut words: Vec<String> = question
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .map(str::to_lowercase)
        .filter(|w| w.chars().count() >= 3 && !QUESTION_WORDS.contains(&w.as_str()))
        .collect();
    words.dedup();
    words
}

/// Search hits per wanted result. Repeated questions in memory can fill the first hits.
const SEARCH_FACTOR: usize = 5;
/// Maximum hits that start a graph expansion.
const EXPANSION_PARENTS: usize = 5;
/// Lexical score of an expanded neighbor, relative to its parent.
const NEIGHBOR_FACTOR: f32 = 0.7;
/// Symbols per changed file in an implementation path.
const SYMBOLS_PER_FILE: usize = 3;
/// Note of a "who calls" answer that the language server verified (ADR-033).
pub(crate) const LSP_VERIFIED_NOTE: &str = "callers verified by the language server";
/// Score of a caller that the language server reports.
const LSP_SCORE: f32 = 0.95;

impl QueryEngine {
    /// Loads a ref if it exists and the scope check permits it.
    pub fn load_allowed(&self, ctx: &QueryContext, value: &str) -> Result<Option<Loaded>> {
        let Some(loaded) = load_ref(&self.store, &self.objects, &self.config.tenant, value)? else { return Ok(None) };
        let allowed = zcie_security::can_read(&ctx.auth, &ctx.scope, &loaded.scope, loaded.classification)
            && ctx.scope.repository_id.as_ref().is_none_or(|r| loaded.scope.repository_id.as_ref().is_none_or(|l| l == r));
        Ok(allowed.then_some(loaded))
    }

    pub(crate) fn candidate(&self, ctx: &QueryContext, value: &str, lexical: f32, status: Option<EdgeStatus>) -> Result<Option<Candidate>> {
        Ok(self.load_allowed(ctx, value)?.map(|loaded| Candidate { loaded, lexical, path_status: status }))
    }

    /// Runs the route of a class. Returns candidates and an optional note.
    pub(crate) fn route(&self, ctx: &QueryContext, class: QueryClass, question: &str) -> Result<(Vec<Candidate>, Option<String>)> {
        match class {
            QueryClass::ExactRef => Ok((self.candidate(ctx, question.trim(), 1.0, None)?.into_iter().collect(), None)),
            QueryClass::SymbolDefinition => self.route_symbol(ctx, question),
            QueryClass::Callers => self.route_callers(ctx, question),
            QueryClass::Task => {
                let tasks = self.tasks_in(ctx, question)?;
                let mut expanded = self.expand_neighbors(ctx, &tasks)?;
                // The verified chain of the task (commit, changed files, symbols): "which file did #89 change".
                for task in &tasks {
                    expanded.extend(self.implementation_path(ctx, &task.loaded.item.entity_ref)?);
                }
                Ok((tasks.into_iter().chain(expanded).collect(), None))
            }
            QueryClass::Implementation => self.route_implementation(ctx, question),
            QueryClass::Timeline => self.route_timeline(ctx, question),
            QueryClass::Search => self.route_search(ctx, question, &[IndexName::Code, IndexName::Documents, IndexName::Conversations]),
        }
    }

    fn route_symbol(&self, ctx: &QueryContext, question: &str) -> Result<(Vec<Candidate>, Option<String>)> {
        let name = crate::classify::symbol_name_in(question);
        let symbols = zcie_symbols::find_by_name(&self.store, &self.config.tenant, &ctx.scope.workspace_id, &name, self.config.max_results * 2)?;
        let mut out = Vec::new();
        for symbol in symbols {
            out.extend(self.candidate(ctx, &format!("sym:{}", symbol.symbol_id), 1.0, Some(EdgeStatus::Verified))?);
        }
        if out.is_empty() {
            return self.route_search(ctx, &name, &[IndexName::Code]);
        }
        Ok((out, None))
    }

    /// "Who calls X": the language server's references when it answers (ADR-033), then the symbols
    /// that contain a resolved call of X (D-071), verified ones first.
    fn route_callers(&self, ctx: &QueryContext, question: &str) -> Result<(Vec<Candidate>, Option<String>)> {
        let (symbols, _) = self.route_symbol(ctx, question)?;
        let t = &self.config.tenant;
        let mut targets = Vec::new();
        for found in &symbols {
            let Some(id) = found.loaded.item.entity_ref.strip_prefix("sym:") else { continue };
            targets.extend(self.store.get::<zcie_types::SymbolRecord>(zcie_store::Cf::Entities, &zcie_store::layout::entity(t, "sym", id))?);
        }
        let mut out = Vec::new();
        for target in &targets {
            out.extend(self.lsp_callers(ctx, target)?);
        }
        let by_server = !out.is_empty();
        let seen: HashSet<String> = out.iter().map(|c| c.loaded.item.entity_ref.clone()).collect();
        for target in &targets {
            for caller in self.callers_of(ctx, target, self.config.max_results * 4)? {
                let entity_ref = self.enclosing_ref(target, &caller.file_id, caller.line)?;
                if seen.contains(&entity_ref) {
                    continue;
                }
                let score = if caller.status == EdgeStatus::Verified { 0.9 } else { 0.6 };
                out.extend(self.candidate(ctx, &entity_ref, score, Some(caller.status))?);
            }
        }
        let note = if by_server {
            Some(LSP_VERIFIED_NOTE.to_string())
        } else {
            out.is_empty().then(|| "no resolved call of this symbol".to_string())
        };
        Ok((out, note))
    }

    /// The references of a Rust or TypeScript symbol from its language server, as verified
    /// candidates. Empty when the server is off, not ready, or the symbol cannot be placed.
    fn lsp_callers(&self, ctx: &QueryContext, target: &zcie_types::SymbolRecord) -> Result<Vec<Candidate>> {
        let (Some(pool), Some(worktree)) = (&self.lsp, target.scope.worktree_id.as_ref()) else { return Ok(Vec::new()) };
        let t = &self.config.tenant;
        let Some(root) = self.worktree_root(Some(worktree)) else { return Ok(Vec::new()) };
        let root = std::fs::canonicalize(&root).unwrap_or_else(|_| PathBuf::from(root));
        let Some(file) = self.store.get::<FileRecord>(zcie_store::Cf::Entities, &zcie_store::layout::entity(t, "file", target.file_id.as_str()))? else {
            return Ok(Vec::new());
        };
        let Some(text) = self.file_text(&root, &file) else { return Ok(Vec::new()) };
        let Some((line, column)) = name_position(&text, target) else { return Ok(Vec::new()) };
        let Some(locations) = pool.references(&root, &root.join(&target.path), &text, line, column) else { return Ok(Vec::new()) };
        let mut out = Vec::new();
        for location in locations {
            let Some(relative) = location.path.strip_prefix(&root).ok().and_then(|p| p.to_str()) else { continue };
            let key = zcie_store::layout::path(t, worktree, &relative.replace('\\', "/"));
            let Some(id) = self.store.get::<zcie_store::values::FileIdValue>(zcie_store::Cf::Lookup, &key)? else { continue };
            let entity_ref = self.enclosing_ref(target, &id.file_id, location.line)?;
            out.extend(self.candidate(ctx, &entity_ref, LSP_SCORE, Some(EdgeStatus::Verified))?);
        }
        Ok(out)
    }

    /// The stored text of a file. A file without a stored copy is read from the project folder:
    /// the one read of a project file in a query, because the language server needs the text the
    /// positions were taken from. The disk copy must still have the indexed hash.
    fn file_text(&self, root: &Path, file: &FileRecord) -> Option<String> {
        if let Ok(text) = self.objects.get_text(&file.content_hash) {
            return Some(text);
        }
        let bytes = std::fs::read(root.join(&file.path)).ok()?;
        (zcie_store::ObjectStore::hash(&bytes) == file.content_hash).then(|| String::from_utf8(bytes).ok()).flatten()
    }

    /// The symbol that contains `line` of a file (the smallest one), or the file itself.
    fn enclosing_ref(&self, target: &zcie_types::SymbolRecord, file_id: &str, line: u32) -> Result<String> {
        let enclosing = zcie_symbols::file_symbols(&self.store, &self.config.tenant, file_id)?
            .into_iter()
            .filter(|s| s.symbol_id != target.symbol_id && (s.span[0]..=s.span[1]).contains(&line))
            .min_by_key(|s| s.span[1] - s.span[0]);
        Ok(enclosing.map_or_else(|| format!("file:{file_id}"), |s| format!("sym:{}", s.symbol_id)))
    }

    /// Returns the tasks that the question names by ref.
    pub(crate) fn tasks_in(&self, ctx: &QueryContext, question: &str) -> Result<Vec<Candidate>> {
        let matches = find_matches(question);
        let batch = Batch::new();
        let mut out = Vec::new();
        for id in &matches.internal_task_ids {
            out.extend(self.candidate(ctx, &format!("task:{id}"), 1.0, None)?);
        }
        for external in &matches.external_task_refs {
            if let Some(task) = zcie_memory::find_by_external_ref(&self.store, &batch, &self.config.tenant, &ctx.scope.workspace_id, external)? {
                out.extend(self.candidate(ctx, &format!("task:{}", task.task_id), 1.0, None)?);
            }
        }
        Ok(out)
    }

    fn route_implementation(&self, ctx: &QueryContext, question: &str) -> Result<(Vec<Candidate>, Option<String>)> {
        let mut tasks = self.tasks_in(ctx, question)?;
        if tasks.is_empty() {
            let (found, _) = self.route_search(ctx, question, &[IndexName::Conversations])?;
            tasks = found.into_iter().filter(|c| c.loaded.item.kind == "task").collect();
        }
        let mut out = Vec::new();
        for task in &tasks {
            out.extend(self.implementation_path(ctx, &task.loaded.item.entity_ref)?);
        }
        out.extend(tasks);
        if out.is_empty() {
            // No task matches ("which commit changed the fee rate"): commit messages and code can still answer.
            let (found, _) = self.route_search(ctx, question, &[IndexName::Code, IndexName::Documents, IndexName::Conversations])?;
            return Ok((found, Some("no task found; lexical search".into())));
        }
        let note = out.iter().all(|c| c.loaded.item.kind == "task").then(|| "no verified implementation link found".to_string());
        Ok((out, note))
    }

    /// Follows `task -Implements-> commit <-ChangedInCommit- file -DefinesSymbol-> sym`.
    pub(crate) fn implementation_path(&self, ctx: &QueryContext, task_ref: &str) -> Result<Vec<Candidate>> {
        let t = &self.config.tenant;
        let mut out = Vec::new();
        for implements in zcie_graph::edges_from(&self.store, t, task_ref, Some(Relation::Implements))? {
            if implements.status == EdgeStatus::Rejected {
                continue;
            }
            out.extend(self.candidate(ctx, &implements.to, 0.9, Some(implements.status))?);
            for changed in zcie_graph::edges_to(&self.store, t, &implements.to, Some(Relation::ChangedInCommit))? {
                let status = implements.status.min(changed.status);
                out.extend(self.candidate(ctx, &changed.from, 0.8, Some(status))?);
                let symbols = zcie_graph::edges_from(&self.store, t, &changed.from, Some(Relation::DefinesSymbol))?;
                for defines in symbols.into_iter().take(SYMBOLS_PER_FILE) {
                    out.extend(self.candidate(ctx, &defines.to, 0.6, Some(status.min(defines.status)))?);
                }
            }
        }
        Ok(out)
    }

    /// History questions: conversations first, then code and documents with a lower lexical weight.
    fn route_timeline(&self, ctx: &QueryContext, question: &str) -> Result<(Vec<Candidate>, Option<String>)> {
        let (mut history, _) = self.route_search(ctx, question, &[IndexName::Conversations])?;
        let (code, _) = self.route_search(ctx, question, &[IndexName::Code, IndexName::Documents])?;
        history.extend(code.into_iter().map(|mut c| {
            c.lexical *= CODE_IN_TIMELINE;
            c
        }));
        history.extend(self.decision_candidates(ctx, question)?);
        Ok((history, None))
    }

    /// Reads the decision index directly and scores each record by word overlap with the
    /// question, so a few decisions are not hidden by many stored messages.
    pub(crate) fn decision_candidates(&self, ctx: &QueryContext, question: &str) -> Result<Vec<Candidate>> {
        let words = content_words(question);
        if words.is_empty() {
            return Ok(Vec::new());
        }
        let mut out = Vec::new();
        for record in zcie_memory::decisions_of_workspace(&self.store, &self.config.tenant, &ctx.scope.workspace_id)? {
            let text = record.text.to_lowercase();
            let hits = words.iter().filter(|w| text.contains(w.as_str())).count();
            if hits == 0 {
                continue;
            }
            let lexical = DECISION_BASE + (1.0 - DECISION_BASE) * hits as f32 / words.len() as f32;
            out.extend(self.candidate(ctx, &format!("mem:{}", record.memory_id), lexical, None)?);
        }
        Ok(out)
    }

    /// Lexical search plus a depth-1 graph expansion of the top hits.
    pub(crate) fn route_search(&self, ctx: &QueryContext, question: &str, indexes: &[IndexName]) -> Result<(Vec<Candidate>, Option<String>)> {
        let query = SearchQuery {
            tenant_id: self.config.tenant.clone(),
            workspace_id: ctx.scope.workspace_id.clone(),
            repository_id: ctx.scope.repository_id.clone(),
            text: question.to_string(),
            indexes: indexes.to_vec(),
            limit: self.config.max_results * SEARCH_FACTOR,
        };
        let echo = crate::planner::text_key(question);
        let hits: Vec<_> = self
            .search
            .search(&query)?
            .into_iter()
            .filter(|h| !matches!(h.kind.as_str(), "event" | "memory") || crate::planner::text_key(&h.name) != echo)
            .collect();
        let mut out = Vec::new();
        for (entity_ref, score) in self.fused_hits(ctx, question, &hits) {
            out.extend(self.candidate(ctx, &entity_ref, score, None)?);
        }
        let expanded = self.expand_neighbors(ctx, &out)?;
        out.extend(expanded);
        Ok((out, None))
    }

    /// Returns the lexical hits with their normalized scores; with vector search on, the lexical
    /// and the vector hits fused by rank (ADR-011).
    fn fused_hits(&self, ctx: &QueryContext, question: &str, hits: &[SearchHit]) -> Vec<(String, f32)> {
        let Some(vectors) = &self.vectors else {
            let top = hits.first().map(|h| h.score).unwrap_or(1.0).max(f32::EPSILON);
            return hits.iter().map(|h| (h.entity_ref.clone(), h.score / top)).collect();
        };
        let limit = self.config.max_results * SEARCH_FACTOR;
        let similar = vectors.search(&ctx.scope.workspace_id, question, limit);
        let lexical: Vec<String> = hits.iter().map(|h| h.entity_ref.clone()).collect();
        crate::fusion::fuse(&lexical, &similar)
    }

    /// Adds depth-1 neighbors of the top candidates.
    pub(crate) fn expand_neighbors(&self, ctx: &QueryContext, parents: &[Candidate]) -> Result<Vec<Candidate>> {
        let options = TraverseOptions {
            relations: Vec::new(),
            direction: Direction::Both,
            max_depth: 1,
            max_fanout: self.config.graph_max_fanout,
            workspace_id: ctx.scope.workspace_id.clone(),
        };
        // Parents can also be neighbors of other parents; `rank` keeps the best score of a ref.
        let mut seen: HashSet<String> = HashSet::new();
        let mut out = Vec::new();
        for parent in parents.iter().take(EXPANSION_PARENTS) {
            let start = &parent.loaded.item.entity_ref;
            for item in zcie_graph::neighbors(&self.store, &self.config.tenant, start, &options)? {
                if !useful_neighbor(&item) || item.entity_ref == *start || !seen.insert(item.entity_ref.clone()) {
                    continue;
                }
                let lexical = parent.lexical * NEIGHBOR_FACTOR;
                out.extend(self.candidate(ctx, &item.entity_ref, lexical, Some(item.path_status))?);
            }
        }
        Ok(out)
    }
}

/// 0-based line and character column of the name of `symbol` on its declaration line. The name
/// must stand alone (not inside a longer word).
fn name_position(text: &str, symbol: &zcie_types::SymbolRecord) -> Option<(u32, u32)> {
    let line = symbol.span[0].checked_sub(1)?;
    let text_line = text.lines().nth(line as usize)?;
    let word = |c: char| c.is_alphanumeric() || c == '_';
    let at = text_line.match_indices(symbol.name.as_str()).map(|(i, _)| i).find(|&i| {
        let before = text_line[..i].chars().next_back().is_some_and(word);
        let after = text_line[i + symbol.name.len()..].chars().next().is_some_and(word);
        !before && !after
    })?;
    Some((line, text_line[..at].chars().count() as u32))
}

fn useful_neighbor(item: &zcie_graph::PathItem) -> bool {
    let kind = item.entity_ref.split(':').next().unwrap_or_default();
    match kind {
        "ev" => matches!(item.via_relation, Relation::TestedBy | Relation::FailedIn),
        "task" | "mem" | "commit" | "file" | "sym" => item.via_relation != Relation::DerivedFrom,
        _ => false,
    }
}
