//! In-memory vector index per workspace. Vectors are stored as int8 (a unit vector has values in
//! [-1, 1]; `round(v * 127)`), so 150,000 symbols with 256 dimensions use about 40 MB.

use std::collections::HashMap;
use std::sync::RwLock;

/// Scale of the int8 quantization.
const SCALE: f32 = 127.0;

/// One search hit.
#[derive(Debug, Clone, PartialEq)]
pub struct VectorHit {
    /// Entity ref.
    pub entity_ref: String,
    /// Cosine similarity (approximate, -1.0 to 1.0).
    pub similarity: f32,
}

/// A vector index (docs/00-agent-rules.md C-05: the one allowed trait with one implementation,
/// so that an approximate index can replace the flat index later).
pub trait VectorIndex: Send + Sync {
    /// Adds or replaces the vector of a ref.
    fn upsert(&self, workspace: &str, entity_ref: &str, vector: &[f32]);
    /// Adds the vector of a ref only if the ref has none (the startup fill must not replace newer vectors).
    fn insert_if_absent(&self, workspace: &str, entity_ref: &str, vector: &[f32]);
    /// Adds the vectors of a batch of (workspace, ref, vector) under one write lock; a ref that has a
    /// vector keeps it (the startup fill: few lock changes, so that queries are not slowed).
    fn insert_many_if_absent(&self, batch: &[(String, String, Vec<f32>)]);
    /// Removes a ref from all workspaces.
    fn remove(&self, entity_ref: &str);
    /// Removes all vectors.
    fn clear(&self);
    /// Returns the `limit` most similar refs of a workspace with a similarity of `min` or more.
    fn search(&self, workspace: &str, vector: &[f32], limit: usize, min: f32) -> Vec<VectorHit>;
    /// Returns the number of vectors.
    fn len(&self) -> usize;
    /// Returns true if the index has no vectors.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[derive(Default)]
struct Workspace {
    position: HashMap<String, usize>,
    refs: Vec<String>,
    vectors: Vec<Vec<i8>>,
}

/// A flat (exact, brute-force) index. A query reads all vectors of one workspace.
#[derive(Default)]
pub struct FlatIndex {
    workspaces: RwLock<HashMap<String, Workspace>>,
}

fn quantize(vector: &[f32]) -> Vec<i8> {
    vector.iter().map(|v| (v * SCALE).round().clamp(-SCALE, SCALE) as i8).collect()
}

fn similarity(a: &[i8], b: &[i8]) -> f32 {
    let dot: i32 = a.iter().zip(b).map(|(x, y)| i32::from(*x) * i32::from(*y)).sum();
    dot as f32 / (SCALE * SCALE)
}

impl FlatIndex {
    fn put(&self, workspace: &str, entity_ref: &str, vector: &[f32], replace: bool) {
        let Ok(mut all) = self.workspaces.write() else { return };
        put_in(&mut all, workspace, entity_ref, vector, replace);
    }
}

fn put_in(all: &mut HashMap<String, Workspace>, workspace: &str, entity_ref: &str, vector: &[f32], replace: bool) {
    let space = all.entry(workspace.to_string()).or_default();
    match space.position.get(entity_ref) {
        Some(&index) if replace => space.vectors[index] = quantize(vector),
        Some(_) => {}
        None => {
            space.position.insert(entity_ref.to_string(), space.refs.len());
            space.refs.push(entity_ref.to_string());
            space.vectors.push(quantize(vector));
        }
    }
}

impl VectorIndex for FlatIndex {
    fn upsert(&self, workspace: &str, entity_ref: &str, vector: &[f32]) {
        self.put(workspace, entity_ref, vector, true);
    }

    fn insert_if_absent(&self, workspace: &str, entity_ref: &str, vector: &[f32]) {
        self.put(workspace, entity_ref, vector, false);
    }

    fn insert_many_if_absent(&self, batch: &[(String, String, Vec<f32>)]) {
        let Ok(mut all) = self.workspaces.write() else { return };
        for (workspace, entity_ref, vector) in batch {
            put_in(&mut all, workspace, entity_ref, vector, false);
        }
    }

    fn remove(&self, entity_ref: &str) {
        let Ok(mut all) = self.workspaces.write() else { return };
        for space in all.values_mut() {
            let Some(index) = space.position.remove(entity_ref) else { continue };
            space.refs.swap_remove(index);
            space.vectors.swap_remove(index);
            if let Some(moved) = space.refs.get(index) {
                space.position.insert(moved.clone(), index);
            }
        }
    }

    fn clear(&self) {
        if let Ok(mut all) = self.workspaces.write() {
            all.clear();
        }
    }

    fn search(&self, workspace: &str, vector: &[f32], limit: usize, min: f32) -> Vec<VectorHit> {
        let Ok(all) = self.workspaces.read() else { return Vec::new() };
        let Some(space) = all.get(workspace) else { return Vec::new() };
        let query = quantize(vector);
        let mut hits: Vec<(f32, usize)> = space.vectors.iter().enumerate().map(|(i, v)| (similarity(&query, v), i)).filter(|(s, _)| *s >= min).collect();
        hits.sort_by(|a, b| b.0.total_cmp(&a.0));
        hits.truncate(limit);
        hits.into_iter().map(|(similarity, i)| VectorHit { entity_ref: space.refs[i].clone(), similarity }).collect()
    }

    fn len(&self) -> usize {
        self.workspaces.read().map(|all| all.values().map(|s| s.refs.len()).sum()).unwrap_or(0)
    }
}
