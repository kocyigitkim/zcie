//! Passages of long texts (ADR-013).
#![allow(clippy::unwrap_used)]

use zcie_search::{PASSAGE_CHARS, passages};

#[test]
fn short_text_is_one_passage_and_long_text_is_covered_with_overlap() {
    assert_eq!(passages("short text"), ["short text"]);
    let text: String = (0..400).map(|i| format!("Sentence number {i} of the document. ")).collect();
    let parts = passages(&text);
    assert!(parts.len() > 3);
    assert!(parts.iter().all(|p| p.chars().count() <= PASSAGE_CHARS));
    let last = parts.len() - 1;
    assert!(parts.iter().enumerate().all(|(i, p)| i == last || p.ends_with('.')), "passages end at sentence ends");
    for i in [0, 137, 399] {
        let sentence = format!("Sentence number {i} of the document.");
        assert!(parts.iter().any(|p| p.contains(&sentence)), "{sentence} is in a passage");
    }
}
