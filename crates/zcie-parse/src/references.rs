//! Names that a file uses (ADR-017, D-057): the identifiers of calls, types and fields that the
//! file does not define. The repository map joins them with the symbol definitions of the project.

use std::collections::{HashMap, HashSet};

use tree_sitter::Node;

/// Node kinds that hold a used name (Rust, TypeScript, Python).
const NAME_KINDS: [&str; 5] = ["identifier", "type_identifier", "field_identifier", "property_identifier", "scoped_identifier"];
/// Shortest name that counts.
const MIN_NAME_CHARS: usize = 3;
/// Most names that one file keeps (the most used).
pub const MAX_NAMES: usize = 500;

/// Returns the used names that `defined` does not have, with their counts, the most used first.
pub fn used_names(root: Node, src: &[u8], defined: &HashSet<&str>) -> Vec<(String, u32)> {
    let mut counts: HashMap<String, u32> = HashMap::new();
    let mut cursor = root.walk();
    let mut visited_children = false;
    loop {
        let node = cursor.node();
        if !visited_children && NAME_KINDS.contains(&node.kind()) && node.child_count() == 0 {
            let text = node.utf8_text(src).unwrap_or_default();
            if text.chars().count() >= MIN_NAME_CHARS && !defined.contains(text) {
                *counts.entry(text.to_string()).or_default() += 1;
            }
        }
        if !visited_children && cursor.goto_first_child() {
            continue;
        }
        if cursor.goto_next_sibling() {
            visited_children = false;
            continue;
        }
        if !cursor.goto_parent() {
            break;
        }
        visited_children = true;
    }
    let mut names: Vec<(String, u32)> = counts.into_iter().collect();
    names.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    names.truncate(MAX_NAMES);
    names
}
