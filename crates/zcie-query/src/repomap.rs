//! Repository map (ADR-017, D-058; the method of the Aider repository map). The files of a project
//! are nodes; a file that uses a name that another file defines has an edge to it. Personalized
//! PageRank ranks the files, and the map shows the best files with the signatures of their most
//! used symbols, within a token budget. The model then goes to the right file with no file scan.

use std::collections::HashMap;
use std::sync::Arc;

use zcie_store::values::FileRefsValue;
use zcie_store::{Cf, layout};
use zcie_types::SymbolRecord;

use crate::error::Result;
use crate::planner::QueryEngine;

/// Damping factor of the PageRank (the usual value).
const DAMPING: f64 = 0.85;
/// PageRank iterations.
const ITERATIONS: usize = 20;
/// Most files of one map graph.
const MAX_FILES: usize = 50_000;
/// Symbols shown for one file.
const SYMBOLS_PER_FILE: usize = 3;
/// Search hits that seed the ranking of a question.
const SEED_HITS: usize = 8;
/// Characters of one signature line.
const SIGNATURE_CHARS: usize = 110;

/// Weight of a used name (the multipliers of the Aider repository map).
fn name_weight(name: &str, defining_files: usize) -> f64 {
    let specific = name.chars().count() >= 8 && (name.contains('_') || name.chars().zip(name.chars().skip(1)).any(|(a, b)| a.is_lowercase() && b.is_uppercase()));
    let mut weight = if specific { 10.0 } else { 1.0 };
    if name.starts_with('_') {
        weight *= 0.1;
    }
    if defining_files > 5 {
        weight *= 0.1;
    }
    weight
}

/// One symbol of a file, with its weighted uses by other files.
struct MapSymbol {
    name: String,
    signature: String,
    uses: f64,
}

/// The file graph of one project.
pub(crate) struct RepoGraph {
    paths: Vec<String>,
    file_ids: Vec<String>,
    users: HashMap<String, Vec<usize>>,
    symbols: Vec<Vec<MapSymbol>>,
    edges: Vec<HashMap<usize, f64>>,
    by_name: HashMap<String, Vec<usize>>,
}

impl RepoGraph {
    /// Returns the seed files with their weights: the files of the search hits (`hit_paths`, best
    /// first; weight 1 / rank), the files whose path ends with a word of the question and the files
    /// that define a specific name of it (weight 1).
    fn seeds(&self, question: &str, hit_paths: &[String]) -> Vec<(usize, f64)> {
        let words: Vec<&str> = question.split(|c: char| c.is_whitespace() || ",;:()`'\"?".contains(c)).filter(|w| w.len() >= 3).collect();
        let mut weights: HashMap<usize, f64> = HashMap::new();
        for (i, path) in self.paths.iter().enumerate() {
            let hit = hit_paths.iter().position(|p| p == path).map_or(0.0, |rank| 1.0 / (rank as f64 + 1.0));
            let named = words.iter().any(|w| w.contains('.') && path.to_lowercase().ends_with(&w.to_lowercase()));
            let weight = if named { 1.0 } else { hit };
            if weight > 0.0 {
                weights.insert(i, weight);
            }
        }
        for word in words.iter().filter(|w| name_weight(w, 1) > 1.0) {
            for &file in self.by_name.get(&word.to_lowercase()).into_iter().flatten() {
                weights.insert(file, 1.0);
            }
        }
        let mut out: Vec<(usize, f64)> = weights.into_iter().collect();
        out.sort_unstable_by_key(|(i, _)| *i);
        out
    }

    /// Personalized PageRank. `seeds` empty: a uniform start vector.
    fn rank(&self, seeds: &[(usize, f64)]) -> Vec<f64> {
        let n = self.paths.len();
        let mut start = vec![0.0; n];
        let total: f64 = seeds.iter().map(|(_, w)| w).sum();
        if total > 0.0 {
            seeds.iter().for_each(|&(i, w)| start[i] = w / total);
        } else if n > 0 {
            start.iter_mut().for_each(|p| *p = 1.0 / n as f64);
        }
        let mut rank = start.clone();
        for _ in 0..ITERATIONS {
            let mut next: Vec<f64> = start.iter().map(|p| p * (1.0 - DAMPING)).collect();
            let mut dangling = 0.0;
            for (i, out) in self.edges.iter().enumerate() {
                let total: f64 = out.values().sum();
                if total <= 0.0 {
                    dangling += rank[i];
                    continue;
                }
                for (&j, &w) in out {
                    next[j] += DAMPING * rank[i] * w / total;
                }
            }
            next.iter_mut().zip(&start).for_each(|(v, p)| *v += DAMPING * dangling * p);
            rank = next;
        }
        rank
    }

    /// Returns the (path, file ID) of the files that use a name, at most `limit` (batch expand, D-063).
    pub(crate) fn users_of(&self, name: &str, limit: usize) -> Vec<(&str, &str)> {
        let files = self.users.get(&name.to_lowercase()).map(Vec::as_slice).unwrap_or_default();
        files.iter().take(limit).map(|&i| (self.paths[i].as_str(), self.file_ids[i].as_str())).collect()
    }

    /// Returns the PageRank of each file path, divided by the best rank (1.0 = the best file).
    pub(crate) fn path_ranks(&self, question: &str, hit_paths: &[String]) -> HashMap<String, f64> {
        let rank = self.rank(&self.seeds(question, hit_paths));
        let best = rank.iter().copied().fold(0.0, f64::max).max(f64::EPSILON);
        self.paths.iter().cloned().zip(rank.into_iter().map(|r| r / best)).collect()
    }

    /// Renders the map of the ranked files within `budget` tokens.
    pub(crate) fn render(&self, question: &str, hit_paths: &[String], budget: u32) -> String {
        let rank = self.rank(&self.seeds(question, hit_paths));
        let mut order: Vec<usize> = (0..self.paths.len()).filter(|&i| !self.symbols[i].is_empty()).collect();
        order.sort_by(|&a, &b| rank[b].total_cmp(&rank[a]).then_with(|| self.paths[a].cmp(&self.paths[b])));
        let mut out = String::new();
        for i in order {
            let mut block = format!("{}\n", self.paths[i]);
            for symbol in self.symbols[i].iter().take(SYMBOLS_PER_FILE) {
                block.push_str(&format!("  {}\n", symbol.signature.chars().take(SIGNATURE_CHARS).collect::<String>()));
            }
            if zcie_types::estimate_tokens(&out) + zcie_types::estimate_tokens(&block) > budget {
                break;
            }
            out.push_str(&block);
        }
        out.trim_end().to_string()
    }
}

impl QueryEngine {
    /// Returns the file graph of a workspace, built again only after a file change of the workspace
    /// (a captured message does not change the graph).
    pub(crate) fn repo_graph(&self, workspace: &str) -> Result<Arc<RepoGraph>> {
        let seq = self.code_seq(workspace);
        let mut cache = self.repo_graphs.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((_, graph)) = cache.get(workspace).filter(|(cached, _)| *cached == seq) {
            return Ok(Arc::clone(graph));
        }
        let graph = Arc::new(self.build_repo_graph(workspace)?);
        cache.insert(workspace.to_string(), (seq, Arc::clone(&graph)));
        Ok(graph)
    }

    /// Returns the sequence of the last file change of a workspace (0 if none).
    pub(crate) fn code_seq(&self, workspace: &str) -> u64 {
        let key = layout::code_seq(&self.config.tenant, workspace);
        self.store.get::<zcie_store::values::CodeSeqValue>(Cf::Lookup, &key).ok().flatten().map_or(0, |v| v.seq)
    }

    fn build_repo_graph(&self, workspace: &str) -> Result<RepoGraph> {
        let t = self.config.tenant.clone();
        let mut graph = RepoGraph { paths: Vec::new(), file_ids: Vec::new(), users: HashMap::new(), symbols: Vec::new(), edges: Vec::new(), by_name: HashMap::new() };
        let mut index: HashMap<String, usize> = HashMap::new();
        for worktree in zcie_memory::worktrees_of_workspace(&self.store, &t, workspace)?.into_iter().filter(|w| w.indexed) {
            for (path, file_id) in zcie_store::files::current_files(&self.store, &t, &worktree.worktree_id, MAX_FILES)? {
                index.insert(file_id.clone(), graph.paths.len());
                graph.paths.push(path);
                graph.file_ids.push(file_id);
            }
        }
        graph.symbols = (0..graph.paths.len()).map(|_| Vec::new()).collect();
        graph.edges = (0..graph.paths.len()).map(|_| HashMap::new()).collect();
        self.add_symbols(&mut graph, &index, workspace)?;
        self.add_references(&mut graph, &index)?;
        for list in &mut graph.symbols {
            list.sort_by(|a, b| b.uses.total_cmp(&a.uses).then_with(|| a.name.cmp(&b.name)));
        }
        Ok(graph)
    }

    fn add_symbols(&self, graph: &mut RepoGraph, index: &HashMap<String, usize>, workspace: &str) -> Result<()> {
        let t = &self.config.tenant;
        for key in self.store.prefix_keys(Cf::Lookup, &layout::symbol_names_prefix(t, workspace), usize::MAX)? {
            let id = zcie_store::keys::last_part(&key);
            let Some(symbol) = self.store.get::<SymbolRecord>(Cf::Entities, &layout::entity(t, "sym", &id))? else { continue };
            let Some(&file) = index.get(symbol.file_id.as_str()) else { continue };
            // A module declaration (`pub mod x;`) is not a symbol that a map needs.
            if symbol.kind == zcie_types::SymbolKind::Module {
                continue;
            }
            let lower = symbol.name.to_lowercase();
            let files = graph.by_name.entry(lower).or_default();
            if !files.contains(&file) {
                files.push(file);
            }
            graph.symbols[file].push(MapSymbol { name: symbol.name, signature: symbol.signature, uses: 0.0 });
        }
        Ok(())
    }

    /// Adds the edges "file uses a name that a file defines", with the weights of the Aider map:
    /// the square root of the uses, ×10 for a specific name (snake_case or camelCase, 8 characters
    /// or more), ×0.1 for a private name (`_x`) and for a name that more than 5 files define.
    fn add_references(&self, graph: &mut RepoGraph, index: &HashMap<String, usize>) -> Result<()> {
        let t = &self.config.tenant;
        for (file_id, &from) in index {
            let Some(refs) = self.store.get::<FileRefsValue>(Cf::Lookup, &layout::file_refs(t, file_id))? else { continue };
            for (name, count) in refs.names {
                let lower = name.to_lowercase();
                let Some(targets) = graph.by_name.get(&lower) else { continue };
                graph.users.entry(lower.clone()).or_default().push(from);
                let weight = name_weight(&name, targets.len()) * f64::from(count).sqrt();
                for &to in targets.iter().filter(|&&to| to != from) {
                    *graph.edges[from].entry(to).or_default() += weight;
                    if let Some(symbol) = graph.symbols[to].iter_mut().find(|s| s.name.to_lowercase() == lower) {
                        symbol.uses += weight;
                    }
                }
            }
        }
        Ok(())
    }
}

/// Note of a map answer.
pub const MAP_NOTE: &str = "repository map";

impl QueryEngine {
    /// Answers `mode: map`: the repository map of the project as the answer (at most
    /// `[context] repo_map_budget` tokens). No map (no indexed folder, or the map is off): not found.
    pub(crate) fn map_answer(&self, ctx: &crate::planner::QueryContext, question: &str, watermarks: zcie_types::Watermarks, budget: u32) -> Result<zcie_types::QueryResponse> {
        let map = self.map_text(ctx, question, budget)?;
        let mut response = self.compose(ctx, (zcie_types::QueryClass::Search, Some(MAP_NOTE.to_string()), question), Vec::new(), watermarks, 0, budget, 1);
        if !map.is_empty() {
            response.answer = Some(map);
            response.answer_kind = zcie_types::AnswerKind::Local;
        }
        Ok(response)
    }

    /// Returns the map text of a workspace within `budget` tokens (capped by the setting), or "".
    /// A question seeds the ranking with the files of its search hits (personalized PageRank).
    pub(crate) fn map_text(&self, ctx: &crate::planner::QueryContext, question: &str, budget: u32) -> Result<String> {
        let budget = budget.min(self.config.context.repo_map_budget);
        if budget == 0 {
            return Ok(String::new());
        }
        let hit_paths = if question.trim().is_empty() { Vec::new() } else { self.hit_paths(ctx, question)? };
        Ok(self.repo_graph(&ctx.scope.workspace_id)?.render(question, &hit_paths, budget))
    }

    /// Returns the paths of the best code hits of a question (at most `SEED_HITS`).
    fn hit_paths(&self, ctx: &crate::planner::QueryContext, question: &str) -> Result<Vec<String>> {
        let (candidates, _) = self.route(ctx, zcie_types::QueryClass::Search, question)?;
        let mut ranked = candidates;
        ranked.sort_by(|a, b| b.lexical.total_cmp(&a.lexical));
        Ok(ranked.into_iter().filter(|c| matches!(c.loaded.item.kind.as_str(), "symbol" | "file")).filter_map(|c| c.loaded.item.path).take(SEED_HITS).collect())
    }
}
