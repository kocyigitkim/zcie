//! Removes repeated results before the ranking is used.

use crate::routes::Candidate;

/// Keeps one result per text (repeated messages and their memory records). A memory record is
/// kept before the event it came from, because it carries status and provenance. A symbol or file
/// with the same path, range and revision in two worktrees of one project (a clone or a Git
/// worktree) is one result.
pub(crate) fn dedupe_text(ranked: Vec<Candidate>) -> Vec<Candidate> {
    let memory_texts: std::collections::HashSet<String> = ranked
        .iter()
        .filter(|c| c.loaded.item.kind == "memory")
        .map(|c| crate::planner::text_key(&c.loaded.item.title))
        .collect();
    let mut seen = std::collections::HashSet::new();
    ranked
        .into_iter()
        .filter(|c| match c.loaded.item.kind.as_str() {
            "event" => !memory_texts.contains(&crate::planner::text_key(&c.loaded.item.title)),
            "memory" => seen.insert(crate::planner::text_key(&c.loaded.item.title)),
            "symbol" | "file" => {
                let item = &c.loaded.item;
                seen.insert(format!("{}|{}|{:?}|{:?}|{:?}", item.kind, item.title, item.path, item.range, item.revision))
            }
            _ => true,
        })
        .collect()
}
