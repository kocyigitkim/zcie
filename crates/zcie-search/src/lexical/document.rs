//! Search documents and write operations.

use std::sync::mpsc::SyncSender;

use crate::lexical::schema::IndexName;

/// One document for the search index.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SearchDoc {
    /// Entity ref. Unique per document.
    pub entity_ref: String,
    /// Entity kind, for example `symbol`.
    pub kind: String,
    /// Tenant ID.
    pub tenant_id: String,
    /// Workspace ID.
    pub workspace_id: String,
    /// Repository ID, or empty.
    pub repository_id: String,
    /// Worktree ID, or empty.
    pub worktree_id: String,
    /// File path, or empty.
    pub path: String,
    /// Name or title.
    pub name: String,
    /// Full text.
    pub body: String,
    /// Sequence of the source state.
    pub seq: u64,
}

/// Characters of one passage of a long text.
pub const PASSAGE_CHARS: usize = 1_500;
/// Characters that two neighbor passages share, so that a sentence on a border is in one passage.
pub const PASSAGE_OVERLAP_CHARS: usize = 200;

/// Returns the Markdown heading lines (`# …`) of a text with the char offsets of their line starts.
/// One pass over the text (a long document has many passages).
fn headings(chars: &[char]) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    let mut line_start = 0;
    for i in 0..=chars.len() {
        if i == chars.len() || chars[i] == '\n' {
            let line: String = chars[line_start..i].iter().collect();
            let line = line.trim();
            if line.starts_with('#') && line.trim_start_matches('#').starts_with(' ') {
                out.push((line_start, line.to_string()));
            }
            line_start = i + 1;
        }
    }
    out
}

/// Returns the last heading that starts before `position`, if any.
fn heading_before(headings: &[(usize, String)], position: usize) -> Option<String> {
    let count = headings.partition_point(|(start, _)| *start < position);
    count.checked_sub(1).map(|i| headings[i].1.clone())
}

/// Splits a long text into passages of about `PASSAGE_CHARS` characters that end at a sentence or
/// line end when possible. A short text is one passage.
pub fn passages(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= PASSAGE_CHARS {
        return vec![text.to_string()];
    }
    let marks = headings(&chars);
    let mut out = Vec::new();
    let mut start = 0;
    while start < chars.len() {
        let hard_end = (start + PASSAGE_CHARS).min(chars.len());
        let soft_end = (start + PASSAGE_CHARS / 2..hard_end).rev().find(|i| matches!(chars[*i], '.' | '\n' | '!' | '?')).map(|i| i + 1);
        let end = if hard_end == chars.len() { hard_end } else { soft_end.unwrap_or(hard_end) };
        // A passage inside a section carries its heading, so that it is findable alone (ADR-017).
        let passage: String = chars[start..end].iter().collect();
        out.push(match heading_before(&marks, start).filter(|h| !passage.starts_with(h.as_str())) {
            Some(heading) => format!("{heading}\n{passage}"),
            None => passage,
        });
        if end == chars.len() {
            break;
        }
        start = end.saturating_sub(PASSAGE_OVERLAP_CHARS).max(start + 1);
    }
    out
}

/// One operation for the search writer.
#[derive(Debug)]
pub enum SearchOp {
    /// Replaces the document with the same ref in the index.
    Upsert(IndexName, Box<SearchDoc>),
    /// Replaces the documents of a ref with one document per passage of a long text (ADR-013).
    /// The body of the given document is ignored; each passage becomes a body.
    UpsertPassages(IndexName, Box<SearchDoc>, Vec<String>),
    /// Deletes the documents with this ref from all indexes.
    Delete(String),
    /// Deletes all documents of all indexes.
    DeleteAll,
    /// Commits now. Sends `true` on the channel after the commit.
    Flush(SyncSender<bool>),
    /// Commits and stops the writer thread.
    Stop,
}

#[cfg(test)]
mod passage_tests {
    use super::passages;

    #[test]
    fn each_passage_carries_its_heading_and_a_long_text_is_fast() {
        let section = |name: &str| format!("# {name}\n{}", "Bu satır bölümün içeriğidir ve uzun bir metin oluşturur.\n".repeat(400));
        let text = format!("{}{}", section("Kurulum"), section("Ayarlar"));
        let started = std::time::Instant::now();
        let parts = passages(&text);
        assert!(started.elapsed().as_millis() < 500, "{:?}", started.elapsed());
        assert!(parts[1].starts_with("# Kurulum\n"), "{}", &parts[1][..40]);
        assert!(parts.last().is_some_and(|p| p.starts_with("# Ayarlar\n")));
    }
}
