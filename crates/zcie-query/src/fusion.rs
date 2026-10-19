//! Reciprocal rank fusion of the lexical hits and the vector hits (ADR-011).

use std::collections::HashMap;

use zcie_vector::VectorHit;

/// Rank constant of the fusion (the usual value of the RRF paper).
const RRF_K: f32 = 60.0;

/// Fuses two ranked lists of refs. Returns (ref, score) with the best score 1.0, best first.
/// A ref in both lists gets the sum of its two reciprocal ranks.
pub fn fuse(lexical: &[String], vector: &[VectorHit]) -> Vec<(String, f32)> {
    let mut scores: HashMap<&str, f32> = HashMap::new();
    let mut order: Vec<&str> = Vec::new();
    let ranked = lexical.iter().map(String::as_str).chain(vector.iter().map(|h| h.entity_ref.as_str()));
    let ranks = (0..lexical.len()).chain(0..vector.len());
    for (entity_ref, rank) in ranked.zip(ranks) {
        let entry = scores.entry(entity_ref).or_insert_with(|| {
            order.push(entity_ref);
            0.0
        });
        *entry += 1.0 / (RRF_K + rank as f32 + 1.0);
    }
    let best = scores.values().copied().fold(f32::MIN, f32::max).max(f32::EPSILON);
    let mut out: Vec<(String, f32)> = order.into_iter().map(|r| (r.to_string(), scores[r] / best)).collect();
    out.sort_by(|a, b| b.1.total_cmp(&a.1));
    out
}
