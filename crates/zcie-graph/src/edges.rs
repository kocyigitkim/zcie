//! Edge write and read functions (`docs/08-memory-and-graph.md` section 5.1).

use zcie_store::{Batch, Cf, Store, layout};
use zcie_types::{Edge, EdgeId, EdgeStatus, Evidence, Relation, Scope, now_utc};

use crate::error::Result;
use crate::status::{effective_status, merged_status};

/// Highest number of edges that one read returns.
pub const MAX_EDGES_PER_READ: usize = 10_000;

/// Makes a new edge value. `put_edge` applies the status rules.
pub fn new_edge(scope: &Scope, from: &str, relation: Relation, to: &str, status: EdgeStatus, evidence: Evidence) -> Edge {
    Edge {
        v: 1,
        edge_id: EdgeId::new(),
        scope: scope.clone(),
        from: from.to_string(),
        relation,
        to: to.to_string(),
        status,
        confidence: confidence_for(status),
        evidence,
        created_utc: now_utc(),
        generation: 1,
    }
}

fn confidence_for(status: EdgeStatus) -> f32 {
    match status {
        EdgeStatus::Verified => 1.0,
        EdgeStatus::Inferred => 0.6,
        EdgeStatus::Candidate => 0.3,
        EdgeStatus::Rejected => 0.0,
    }
}

/// Adds an edge in both directions to the batch. Returns the edge ID.
///
/// The status is lowered to the evidence maximum. An existing edge keeps its ID and the higher status.
pub fn put_edge(store: &Store, batch: &mut Batch, mut edge: Edge) -> Result<EdgeId> {
    edge.status = effective_status(edge.status, edge.evidence.kind);
    edge.confidence = confidence_for(edge.status);
    let tenant = edge.scope.tenant_id.clone();
    let out_key = layout::edge(&tenant, &edge.from, edge.relation.name(), &edge.to);
    let existing: Option<Edge> = match batch.pending(Cf::EdgesOut, &out_key) {
        Some(edge) => Some(edge),
        None => store.get(Cf::EdgesOut, &out_key)?,
    };
    if let Some(old) = existing {
        edge = merge(old, edge);
    }
    let in_key = layout::edge(&tenant, &edge.to, edge.relation.name(), &edge.from);
    batch.put(Cf::EdgesOut, out_key, &edge)?;
    batch.put(Cf::EdgesIn, in_key, &edge)?;
    Ok(edge.edge_id)
}

fn merge(old: Edge, new: Edge) -> Edge {
    let status = merged_status(old.status, new.status, new.evidence.kind);
    let evidence = if status == new.status && new.status.rank() >= old.status.rank() {
        merge_sources(new.evidence, &old.evidence)
    } else {
        merge_sources(old.evidence.clone(), &new.evidence)
    };
    Edge {
        edge_id: old.edge_id,
        created_utc: old.created_utc,
        generation: old.generation + 1,
        status,
        confidence: confidence_for(status),
        evidence,
        ..new
    }
}

fn merge_sources(mut base: Evidence, other: &Evidence) -> Evidence {
    for id in &other.source_event_ids {
        if !base.source_event_ids.contains(id) {
            base.source_event_ids.push(id.clone());
        }
    }
    base
}

/// Reads the outgoing edges of a ref, optionally of one relation.
pub fn edges_from(store: &Store, tenant: &str, from: &str, relation: Option<Relation>) -> Result<Vec<Edge>> {
    read_edges(store, Cf::EdgesOut, tenant, from, relation, MAX_EDGES_PER_READ)
}

/// Reads the incoming edges of a ref, optionally of one relation.
pub fn edges_to(store: &Store, tenant: &str, to: &str, relation: Option<Relation>) -> Result<Vec<Edge>> {
    read_edges(store, Cf::EdgesIn, tenant, to, relation, MAX_EDGES_PER_READ)
}

/// Reads at most `limit` edges of a ref in one direction.
pub fn read_edges(store: &Store, cf: Cf, tenant: &str, entity_ref: &str, relation: Option<Relation>, limit: usize) -> Result<Vec<Edge>> {
    let prefix = layout::edge_prefix(tenant, entity_ref, relation.map(|r| r.name()));
    let items = store.prefix_scan::<Edge>(cf, &prefix, limit)?;
    Ok(items.into_iter().map(|(_, edge)| edge).collect())
}

/// Adds the deletion of an edge (both directions) to the batch.
pub fn delete_edge(batch: &mut Batch, tenant: &str, from: &str, relation: Relation, to: &str) {
    batch.delete(Cf::EdgesOut, layout::edge(tenant, from, relation.name(), to));
    batch.delete(Cf::EdgesIn, layout::edge(tenant, to, relation.name(), from));
}

/// Deletes all outgoing edges of one relation from a ref. Returns the number of edges.
pub fn delete_edges_from(store: &Store, batch: &mut Batch, tenant: &str, from: &str, relation: Relation) -> Result<usize> {
    let edges = edges_from(store, tenant, from, Some(relation))?;
    for edge in &edges {
        delete_edge(batch, tenant, &edge.from, edge.relation, &edge.to);
    }
    Ok(edges.len())
}

/// Deletes all edges (both directions) that touch a ref. Returns the number of edges.
pub fn delete_all_edges_of(store: &Store, batch: &mut Batch, tenant: &str, entity_ref: &str) -> Result<usize> {
    let mut count = 0;
    for edge in edges_from(store, tenant, entity_ref, None)? {
        delete_edge(batch, tenant, &edge.from, edge.relation, &edge.to);
        count += 1;
    }
    for edge in edges_to(store, tenant, entity_ref, None)? {
        delete_edge(batch, tenant, &edge.from, edge.relation, &edge.to);
        count += 1;
    }
    Ok(count)
}
