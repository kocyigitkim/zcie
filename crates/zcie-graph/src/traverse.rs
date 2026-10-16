//! Bounded breadth-first traversal (`docs/08-memory-and-graph.md` section 8).

use std::collections::{HashSet, VecDeque};

use zcie_store::{Cf, Store};
use zcie_types::{Edge, EdgeStatus, Relation};

use crate::edges::read_edges;
use crate::error::Result;

/// Traversal direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// Follow `edges_out`.
    Out,
    /// Follow `edges_in`.
    In,
    /// Follow both.
    Both,
}

/// Traversal limits and filters.
#[derive(Debug, Clone)]
pub struct TraverseOptions {
    /// Only these relations. Empty means all relations.
    pub relations: Vec<Relation>,
    /// Direction to follow.
    pub direction: Direction,
    /// Maximum depth (1 or more).
    pub max_depth: usize,
    /// Maximum edges read from one node.
    pub max_fanout: usize,
    /// Only nodes reached through edges of this workspace.
    pub workspace_id: String,
}

/// One node reached by a traversal.
#[derive(Debug, Clone, PartialEq)]
pub struct PathItem {
    /// The node ref.
    pub entity_ref: String,
    /// The node from which the edge was followed.
    pub parent_ref: String,
    /// Depth from the start (1 for direct neighbors).
    pub depth: usize,
    /// The relation of the last edge.
    pub via_relation: Relation,
    /// True if the last edge was followed in its direction.
    pub forward: bool,
    /// Lowest status on the path.
    pub path_status: EdgeStatus,
    /// Edge IDs on the path, start first.
    pub edge_ids: Vec<String>,
    /// The last edge of the path.
    pub edge: Edge,
}

struct Frontier {
    entity_ref: String,
    depth: usize,
    status: EdgeStatus,
    edge_ids: Vec<String>,
}

/// Returns the nodes reachable from `start` within the limits. Rejected edges are skipped.
pub fn neighbors(store: &Store, tenant: &str, start: &str, options: &TraverseOptions) -> Result<Vec<PathItem>> {
    let mut visited: HashSet<String> = HashSet::from([start.to_string()]);
    let mut queue = VecDeque::from([Frontier {
        entity_ref: start.to_string(),
        depth: 0,
        status: EdgeStatus::Verified,
        edge_ids: Vec::new(),
    }]);
    let mut out = Vec::new();
    while let Some(node) = queue.pop_front() {
        if node.depth >= options.max_depth {
            continue;
        }
        for (edge, forward) in node_edges(store, tenant, &node.entity_ref, options)? {
            let next = if forward { edge.to.clone() } else { edge.from.clone() };
            if !visited.insert(next.clone()) {
                continue;
            }
            let item = path_item(&node, &edge, next, forward);
            queue.push_back(Frontier {
                entity_ref: item.entity_ref.clone(),
                depth: item.depth,
                status: item.path_status,
                edge_ids: item.edge_ids.clone(),
            });
            out.push(item);
        }
    }
    Ok(out)
}

fn path_item(node: &Frontier, edge: &Edge, next: String, forward: bool) -> PathItem {
    let mut edge_ids = node.edge_ids.clone();
    edge_ids.push(edge.edge_id.0.clone());
    PathItem {
        entity_ref: next,
        parent_ref: node.entity_ref.clone(),
        depth: node.depth + 1,
        via_relation: edge.relation,
        forward,
        path_status: node.status.min(edge.status),
        edge_ids,
        edge: edge.clone(),
    }
}

fn node_edges(store: &Store, tenant: &str, entity_ref: &str, options: &TraverseOptions) -> Result<Vec<(Edge, bool)>> {
    let mut out = Vec::new();
    if options.direction != Direction::In {
        let edges = read_edges(store, Cf::EdgesOut, tenant, entity_ref, None, options.max_fanout)?;
        out.extend(edges.into_iter().map(|e| (e, true)));
    }
    if options.direction != Direction::Out {
        let edges = read_edges(store, Cf::EdgesIn, tenant, entity_ref, None, options.max_fanout)?;
        out.extend(edges.into_iter().map(|e| (e, false)));
    }
    out.retain(|(edge, _)| {
        edge.status != EdgeStatus::Rejected
            && edge.scope.workspace_id == options.workspace_id
            && (options.relations.is_empty() || options.relations.contains(&edge.relation))
    });
    Ok(out)
}
