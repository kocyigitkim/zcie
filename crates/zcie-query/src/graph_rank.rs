//! Graph ranking for multi-hop questions (ADR-017, D-060; the method of HippoRAG). The entities
//! that a question names (task refs, symbols, files, commits) are seeds. Personalized PageRank
//! over the graph around them ranks the connected entities, so that a question that connects a
//! task, a commit, a file and a test gets the whole chain in one answer.

use std::collections::{HashMap, VecDeque};

use zcie_store::Batch;
use zcie_store::Cf;
use zcie_types::EdgeStatus;

use crate::error::Result;
use crate::planner::{QueryContext, QueryEngine};
use crate::routes::Candidate;

/// Depth of the subgraph around the seeds.
const MAX_DEPTH: usize = 3;
/// Most nodes of the subgraph.
const MAX_NODES: usize = 2_000;
/// Most edges read for one node in one direction.
const EDGES_PER_NODE: usize = 50;
/// Damping factor (low: the rank stays near the seeds).
const DAMPING: f64 = 0.5;
/// PageRank iterations.
const ITERATIONS: usize = 20;
/// Ranked nodes that become candidates.
const TOP_NODES: usize = 10;
/// Most seeds of one question.
const MAX_SEEDS: usize = 8;
/// Score of the best graph node that is not a seed (and of each seed).
const GRAPH_FACTOR: f32 = 0.6;
/// Bonus of a node that two or more seeds reach (on a path between seeds).
const BRIDGE_BONUS: f32 = 0.2;

fn status_weight(status: EdgeStatus) -> f64 {
    match status {
        EdgeStatus::Verified => 1.0,
        EdgeStatus::Inferred => 0.5,
        EdgeStatus::Candidate => 0.2,
        EdgeStatus::Rejected => 0.0,
    }
}

/// The subgraph around the seeds: nodes, weighted undirected edges, and the seeds that reach each node.
struct Subgraph {
    nodes: Vec<String>,
    edges: Vec<HashMap<usize, f64>>,
    reached_by: Vec<u32>,
}

impl Subgraph {
    fn node(&mut self, index: &mut HashMap<String, usize>, entity_ref: &str) -> Option<usize> {
        if let Some(&i) = index.get(entity_ref) {
            return Some(i);
        }
        if self.nodes.len() >= MAX_NODES {
            return None;
        }
        index.insert(entity_ref.to_string(), self.nodes.len());
        self.nodes.push(entity_ref.to_string());
        self.edges.push(HashMap::new());
        self.reached_by.push(0);
        Some(self.nodes.len() - 1)
    }

    fn rank(&self, seeds: &[usize]) -> Vec<f64> {
        let mut start = vec![0.0; self.nodes.len()];
        seeds.iter().for_each(|&s| start[s] = 1.0 / seeds.len() as f64);
        let mut rank = start.clone();
        for _ in 0..ITERATIONS {
            let mut next: Vec<f64> = start.iter().map(|p| p * (1.0 - DAMPING)).collect();
            for (i, out) in self.edges.iter().enumerate() {
                let total: f64 = out.values().sum();
                if total > 0.0 {
                    out.iter().for_each(|(&j, &w)| next[j] += DAMPING * rank[i] * w / total);
                }
            }
            rank = next;
        }
        rank
    }
}

impl QueryEngine {
    /// Returns the seed refs of a question: tasks, symbols, files and commits that it names.
    fn graph_seeds(&self, ctx: &QueryContext, question: &str) -> Result<Vec<String>> {
        let t = &self.config.tenant;
        let ws = &ctx.scope.workspace_id;
        let mut seeds = Vec::new();
        let words: Vec<&str> = question.split(|c: char| c.is_whitespace() || ",;?!()`'\"".contains(c)).filter(|w| w.len() >= 2).collect();
        for word in &words {
            let external = word.trim_end_matches(['.', ':']);
            if (external.starts_with('#') || external.contains('-')) && external.chars().any(|c| c.is_ascii_digit())
                && let Some(task) = zcie_memory::find_by_external_ref(&self.store, &Batch::new(), t, ws, external)?
            {
                seeds.push(format!("task:{}", task.task_id));
            }
            let name = external.trim_end_matches("()");
            if name.len() >= 4 && (name.contains('_') || name.chars().zip(name.chars().skip(1)).any(|(a, b)| a.is_lowercase() && b.is_uppercase())) {
                seeds.extend(zcie_symbols::find_by_name(&self.store, t, ws, name, 2)?.into_iter().map(|s| format!("sym:{}", s.symbol_id)));
            }
        }
        seeds.extend(self.path_and_commit_seeds(ws, &words)?);
        seeds.dedup();
        seeds.truncate(MAX_SEEDS);
        Ok(seeds)
    }

    fn path_and_commit_seeds(&self, workspace: &str, words: &[&str]) -> Result<Vec<String>> {
        let t = &self.config.tenant;
        let mut seeds = Vec::new();
        for worktree in zcie_memory::worktrees_of_workspace(&self.store, t, workspace)? {
            for word in words.iter().map(|w| w.trim_end_matches(['.', ':'])) {
                if word.contains('/')
                    && let Some(file) = self.store.get::<zcie_store::values::FileIdValue>(Cf::Lookup, &zcie_store::layout::path(t, &worktree.worktree_id, word))?
                {
                    seeds.push(format!("file:{}", file.file_id));
                }
                if (7..=40).contains(&word.len()) && word.chars().all(|c| c.is_ascii_hexdigit()) {
                    let prefix = [zcie_store::layout::commits_prefix(t, &worktree.repository_id), word.to_lowercase().into_bytes()].concat();
                    for (_, commit) in self.store.prefix_scan::<zcie_store::values::CommitIdValue>(Cf::Lookup, &prefix, 1)? {
                        seeds.push(format!("commit:{}", commit.commit_id));
                    }
                }
            }
        }
        Ok(seeds)
    }

    /// Collects the subgraph around the seeds (breadth first, both directions).
    fn subgraph(&self, seeds: &[String]) -> Result<(Subgraph, Vec<usize>)> {
        let t = &self.config.tenant;
        let mut graph = Subgraph { nodes: Vec::new(), edges: Vec::new(), reached_by: Vec::new() };
        let mut index = HashMap::new();
        let mut queue = VecDeque::new();
        let mut seed_nodes = Vec::new();
        for (bit, seed) in seeds.iter().enumerate() {
            let Some(i) = graph.node(&mut index, seed) else { break };
            graph.reached_by[i] |= 1 << bit;
            seed_nodes.push(i);
            queue.push_back((i, 0usize, 1u32 << bit));
        }
        while let Some((node, depth, seed_bit)) = queue.pop_front() {
            if depth >= MAX_DEPTH {
                continue;
            }
            let entity = graph.nodes[node].clone();
            let out = zcie_graph::edges::read_edges(&self.store, Cf::EdgesOut, t, &entity, None, EDGES_PER_NODE)?;
            let incoming = zcie_graph::edges::read_edges(&self.store, Cf::EdgesIn, t, &entity, None, EDGES_PER_NODE)?;
            for edge in out.into_iter().chain(incoming) {
                let other = if edge.from == entity { edge.to.clone() } else { edge.from.clone() };
                let Some(j) = graph.node(&mut index, &other) else { continue };
                let weight = status_weight(edge.status);
                *graph.edges[node].entry(j).or_default() += weight;
                *graph.edges[j].entry(node).or_default() += weight;
                if graph.reached_by[j] & seed_bit == 0 {
                    graph.reached_by[j] |= seed_bit;
                    queue.push_back((j, depth + 1, seed_bit));
                }
            }
        }
        Ok((graph, seed_nodes))
    }

    /// Returns the best entities of the graph around the entities that the question names.
    pub(crate) fn graph_candidates(&self, ctx: &QueryContext, question: &str) -> Result<Vec<Candidate>> {
        if !self.config.context.graph_rank {
            return Ok(Vec::new());
        }
        let seeds = self.graph_seeds(ctx, question)?;
        if seeds.is_empty() {
            return Ok(Vec::new());
        }
        let (graph, seed_nodes) = self.subgraph(&seeds)?;
        let rank = graph.rank(&seed_nodes);
        let mut order: Vec<usize> = (0..graph.nodes.len()).collect();
        order.sort_by(|&a, &b| rank[b].total_cmp(&rank[a]));
        // The seeds hold most of the rank; the other nodes are scored against the best of them, and
        // the square root keeps a node two steps away (task → commit → file) in the results.
        let best = order.iter().filter(|i| !seed_nodes.contains(i)).map(|&i| rank[i]).fold(0.0, f64::max).max(f64::EPSILON);
        let mut out = Vec::new();
        for i in order.into_iter().take(TOP_NODES) {
            let bridge = if graph.reached_by[i].count_ones() >= 2 && !seed_nodes.contains(&i) { BRIDGE_BONUS } else { 0.0 };
            let score = GRAPH_FACTOR * (rank[i] / best).min(1.0).sqrt() as f32 + bridge;
            out.extend(self.candidate(ctx, &graph.nodes[i], score, None)?);
        }
        Ok(out)
    }
}
