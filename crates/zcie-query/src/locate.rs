//! Localization (ADR-017, D-062; coarse to fine as in Agentless and AutoCodeRover). For a change
//! request, the answer names the files to change, best first, and the results are their symbols
//! with line ranges and signatures. The model then reads only those ranges.

use std::collections::HashMap;

use zcie_types::{AnswerKind, QueryClass, QueryResponse, SymbolRecord, Watermarks};

use crate::error::Result;
use crate::planner::{QueryContext, QueryEngine};
use crate::relevance::stems;
use crate::routes::Candidate;

/// Phrases of a change request (lowercase).
const CHANGE_PHRASES: [&str; 12] = [
    "which file must i change", "which file should i change", "which files must i change", "which file do i change",
    "where must i change", "where should i change", "where do i change", "what file do i need to change",
    // Turkish: the forms of "must I change" only; "hangi dosyayı değiştirdi" asks about the history.
    "hangi dosyayı değiştirme", "hangi dosyaları değiştirme", "nereyi değiştirmeli", "nerede değiştirmeli",
];
/// Files in a localization answer.
const TOP_FILES: usize = 5;
/// Symbols shown for one file.
const SYMBOLS_PER_FILE: usize = 3;
/// Hits of one file that its score adds.
const HITS_PER_FILE: usize = 3;
/// Factor of a whole-file hit against a symbol hit.
const FILE_HIT_FACTOR: f64 = 0.5;
/// Extra factor of a document or configuration file (a change request is usually about code).
const DOCUMENT_FACTOR: f64 = 0.3;

/// True for a document or configuration file.
fn is_document(path: &str) -> bool {
    let lower = path.to_lowercase();
    [".md", ".txt", ".rst", ".json", ".yml", ".yaml", ".toml", ".lock", ".html"].iter().any(|e| lower.ends_with(e))
}

/// Factor of a test, example or benchmark file: a change request is usually about the source.
const SUPPORT_FILE_FACTOR: f64 = 0.5;
/// Share of the repository-map rank in the file score.
const MAP_SHARE: f64 = 0.3;
/// Token budget of a localization answer (JSON tokens; the printed capsule is about 300 tokens).
const LOCATE_BUDGET: u32 = 500;
/// Note of a localization answer.
pub const LOCATE_NOTE: &str = "files and symbols to change";

/// Starts of instruction sentences that are not part of the request (lowercase).
const INSTRUCTION_STARTS: [&str; 10] = ["do not", "don't", "answer", "give", "only", "just", "yalnızca", "sadece", "cevap", "değişiklik yapma"];

/// Returns the request part of a change request: the change phrase, "so that" and instruction
/// sentences ("Do not edit anything. Answer with the file path only.") are removed, so that their
/// words ("file", "edit", "path") do not steer the search.
pub fn request_text(question: &str) -> String {
    let mut kept = Vec::new();
    for sentence in question.split_inclusive(['.', '?', '!']) {
        let lower = sentence.trim().to_lowercase();
        if INSTRUCTION_STARTS.iter().any(|s| lower.starts_with(s)) {
            continue;
        }
        let mut text = lower.clone();
        for phrase in CHANGE_PHRASES {
            text = text.replace(phrase, " ");
        }
        for filler in ["so that", "in order to", "to make", "ki "] {
            text = text.replace(filler, " ");
        }
        kept.push(text.trim_matches(|c: char| c.is_whitespace() || c == '?' || c == '.').to_string());
    }
    let request = kept.join(" ").trim().to_string();
    if request.is_empty() { question.to_string() } else { request }
}

/// True for a change request ("which file must I change so that …").
pub fn asks_for_location(question: &str) -> bool {
    let lower = question.to_lowercase();
    CHANGE_PHRASES.iter().any(|p| lower.contains(p))
}

/// Score of a symbol: the share of the question terms in its name and signature.
fn symbol_score(symbol: &SymbolRecord, terms: &std::collections::HashSet<String>) -> f64 {
    if terms.is_empty() {
        return 0.0;
    }
    let own = stems(&format!("{} {}", symbol.qualified_name.replace(['_', ':'], " "), symbol.signature));
    let matched = terms.iter().filter(|t| own.iter().any(|s| crate::synonyms::same_stem(s, t))).count();
    matched as f64 / terms.len() as f64
}

impl QueryEngine {
    /// Ranks the files of the search hits and the repository map. Returns (path, score), best first.
    fn ranked_files(&self, ctx: &QueryContext, question: &str, hits: &[Candidate]) -> Result<Vec<(String, f64)>> {
        // The score of a file is the sum of its best HITS_PER_FILE hits: a file with several matching
        // symbols is a better place to change than a file with one similar name.
        let mut per_file: HashMap<String, Vec<f64>> = HashMap::new();
        for hit in hits.iter().filter(|c| matches!(c.loaded.item.kind.as_str(), "symbol" | "file")) {
            if let Some(path) = &hit.loaded.item.path {
                // A symbol hit names code; a whole-file hit (a changelog mentions everything) counts less.
                let weight = if hit.loaded.item.kind == "symbol" { 1.0 } else if is_document(path) { FILE_HIT_FACTOR * DOCUMENT_FACTOR } else { FILE_HIT_FACTOR };
                per_file.entry(path.clone()).or_default().push(f64::from(hit.lexical) * weight);
            }
        }
        let mut scores: HashMap<String, f64> = per_file
            .into_iter()
            .map(|(path, mut values)| {
                values.sort_by(|a, b| b.total_cmp(a));
                (path, values.into_iter().take(HITS_PER_FILE).sum())
            })
            .collect();
        let hit_paths: Vec<String> = scores.keys().cloned().collect();
        for (path, rank) in self.repo_graph(&ctx.scope.workspace_id)?.path_ranks(question, &hit_paths) {
            if let Some(score) = scores.get_mut(&path) {
                *score += MAP_SHARE * rank;
            }
        }
        let support = |path: &str| ["test", "/examples/", "examples/", "/benches/", "benches/", "/fixtures/"].iter().any(|p| path.contains(p));
        let mut files: Vec<(String, f64)> = scores.into_iter().map(|(p, s)| { let s = if support(&p) { s * SUPPORT_FILE_FACTOR } else { s }; (p, s) }).collect();
        files.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        files.truncate(TOP_FILES);
        Ok(files)
    }

    /// Returns the best symbols of one file for the question, as candidates.
    fn file_symbol_candidates(&self, ctx: &QueryContext, path: &str, file_score: f64, hits: &[Candidate], question: &str) -> Result<Vec<Candidate>> {
        let t = &self.config.tenant;
        let terms = stems(question);
        let hit_scores: HashMap<&str, f32> = hits.iter().map(|c| (c.loaded.item.entity_ref.as_str(), c.lexical)).collect();
        let mut symbols = Vec::new();
        for worktree in zcie_memory::worktrees_of_workspace(&self.store, t, &ctx.scope.workspace_id)? {
            let key = zcie_store::layout::path(t, &worktree.worktree_id, path);
            if let Some(file) = self.store.get::<zcie_store::values::FileIdValue>(zcie_store::Cf::Lookup, &key)? {
                symbols.extend(zcie_symbols::file_symbols(&self.store, t, &file.file_id)?);
            }
        }
        let score = |s: &SymbolRecord| hit_scores.get(format!("sym:{}", s.symbol_id).as_str()).map_or(0.0, |v| f64::from(*v)) + symbol_score(s, &terms);
        symbols.sort_by(|a, b| score(b).total_cmp(&score(a)).then_with(|| a.span[0].cmp(&b.span[0])));
        let mut out = Vec::new();
        for symbol in symbols.iter().take(SYMBOLS_PER_FILE) {
            out.extend(self.candidate(ctx, &format!("sym:{}", symbol.symbol_id), (file_score + score(symbol) * 0.1) as f32, None)?);
        }
        Ok(out)
    }

    /// Answers a change request: the files to change (answer) and their best symbols (results).
    pub(crate) fn locate_answer(&self, ctx: &QueryContext, full_question: &str, watermarks: Watermarks, budget: u32) -> Result<QueryResponse> {
        let request = request_text(full_question);
        let question = request.as_str();
        let (mut hits, _) = self.route(ctx, QueryClass::Search, question)?;
        hits.extend(self.graph_candidates(ctx, question)?);
        let files = self.ranked_files(ctx, question, &hits)?;
        let mut candidates = Vec::new();
        for (path, score) in &files {
            candidates.extend(self.file_symbol_candidates(ctx, path, *score, &hits, question)?);
        }
        let budget = budget.min(LOCATE_BUDGET);
        let mut response = self.compose(ctx, (QueryClass::Search, Some(LOCATE_NOTE.to_string()), question), candidates, watermarks, 0, budget, TOP_FILES * SYMBOLS_PER_FILE);
        if !files.is_empty() {
            let list: Vec<&str> = files.iter().map(|(p, _)| p.as_str()).collect();
            response.answer = Some(format!("Files to change, best first: {}. The symbols below have their line ranges; read only those ranges.", list.join(", ")));
            response.answer_kind = AnswerKind::Local;
        }
        Ok(response)
    }
}
