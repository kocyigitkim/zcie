//! Live-overlay check of results (`docs/09-query-and-context.md` section 7).

use std::path::Path;

use zcie_store::{Cf, layout};
use zcie_types::WorktreeRecord;

use crate::planner::QueryEngine;
use crate::routes::Candidate;

/// Largest file that the freshness check reads.
const MAX_CHECK_BYTES: u64 = 4 * 1024 * 1024;

/// Checks results whose path has a pending file job. Drops results that the current file
/// does not confirm. Returns the kept results and the number of overlay results.
pub fn check_overlay(engine: &QueryEngine, candidates: Vec<Candidate>) -> (Vec<Candidate>, usize) {
    let mut overlay_hits = 0;
    let mut out = Vec::with_capacity(candidates.len());
    for candidate in candidates {
        let Some(path) = candidate.loaded.item.path.clone() else {
            out.push(candidate);
            continue;
        };
        if !engine.ingest.overlay.contains_path(&path) {
            out.push(candidate);
            continue;
        }
        overlay_hits += 1;
        if confirmed(engine, &candidate, &path) {
            out.push(candidate);
        }
    }
    (out, overlay_hits)
}

fn confirmed(engine: &QueryEngine, candidate: &Candidate, path: &str) -> bool {
    let Some(worktree_id) = &candidate.loaded.scope.worktree_id else { return true };
    let key = layout::registry(&engine.config.tenant, "worktree", worktree_id);
    let Ok(Some(worktree)) = engine.store.get::<WorktreeRecord>(Cf::Registry, &key) else { return true };
    let file = Path::new(&worktree.root_path).join(path);
    if candidate.loaded.item.kind != "symbol" {
        return file.exists();
    }
    let name = candidate.loaded.item.title.rsplit(['.', ':']).next().unwrap_or_default().to_string();
    // A file that grew past the read limit is not read whole; its symbol is not confirmed.
    if std::fs::metadata(&file).map(|m| m.len() > MAX_CHECK_BYTES).unwrap_or(true) {
        return false;
    }
    std::fs::read_to_string(file).map(|text| text.contains(&name)).unwrap_or(false)
}
