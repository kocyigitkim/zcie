//! Extractive compression of snippets (ADR-017, D-061; sentence filtering as in LLMLingua, with no
//! model). A long memory, event or file text keeps the sentences that the question needs, in their
//! order, and "…" marks each gap.

use crate::relevance::{best_window, names_a_fact, stems};

/// A snippet of this length or shorter is not compressed.
const MIN_COMPRESS_CHARS: usize = 300;
/// Most characters of a compressed item.
pub const MAX_ITEM_CHARS: usize = 450;

/// Splits a text into sentences and lines; each unit keeps its end mark.
fn units(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    let bytes = text.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        let end_of_sentence = matches!(b, b'.' | b'!' | b'?') && bytes.get(i + 1).is_none_or(|n| n.is_ascii_whitespace());
        if b == b'\n' || end_of_sentence {
            let unit = text[start..=i].trim();
            if !unit.is_empty() {
                out.push(unit);
            }
            start = i + 1;
        }
    }
    let rest = text[start..].trim();
    if !rest.is_empty() {
        out.push(rest);
    }
    out
}

/// Importance of a unit for the question: the number of question terms in it, plus a half for a
/// number with a name or a path. 0: the question does not need the unit.
fn importance(unit: &str, terms: &std::collections::HashSet<String>) -> f32 {
    let unit_stems = stems(unit);
    let matched = terms.iter().filter(|t| unit_stems.iter().any(|s| crate::synonyms::same_stem(s, t))).count() as f32;
    let fact = if unit.chars().any(|c| c.is_ascii_digit()) && names_a_fact(unit) { 0.5 } else { 0.0 };
    matched + fact
}

/// Returns the indexes of the units to keep: the most important first, until `max_chars` is used.
/// The unit after the most important one is also kept (it often continues it).
fn select(all: &[&str], terms: &std::collections::HashSet<String>, max_chars: usize) -> Vec<usize> {
    let mut ranked: Vec<(usize, f32)> = all.iter().enumerate().map(|(i, u)| (i, importance(u, terms))).filter(|(_, s)| *s > 0.0).collect();
    ranked.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
    if let Some(&(top, _)) = ranked.first()
        && top + 1 < all.len()
    {
        ranked.insert(1, (top + 1, 0.0));
    }
    let mut used = 0;
    let mut keep = Vec::new();
    for (i, _) in ranked {
        let cost = all[i].chars().count() + 3;
        if keep.contains(&i) || used + cost > max_chars {
            continue;
        }
        used += cost;
        keep.push(i);
    }
    keep.sort_unstable();
    keep
}

/// Returns the compressed text of `text` for `question`, at most `max_chars` characters. The kept
/// units are the most important ones; they keep their order, and "…" marks each gap.
pub fn compress(text: &str, question: &str, max_chars: usize) -> String {
    if text.chars().count() <= MIN_COMPRESS_CHARS {
        return text.to_string();
    }
    let all = units(text);
    let keep = select(&all, &stems(question), max_chars);
    if keep.is_empty() {
        return best_window(text, question, max_chars);
    }
    let mut out = String::new();
    let mut previous: Option<usize> = None;
    for i in keep {
        let gap = previous.map_or(i > 0, |p| i > p + 1);
        if !out.is_empty() {
            out.push(' ');
        }
        if gap {
            out.push_str("… ");
        }
        out.push_str(all[i]);
        previous = Some(i);
    }
    if previous.is_some_and(|p| p + 1 < all.len()) {
        out.push_str(" …");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_most_important_unit_is_kept_even_when_it_is_late() {
        let filler: String = (0..20).map(|i| format!("Hedef {i} için genel bir açıklama burada yazılı.\n")).collect();
        let text = format!("{filler}Warm exact entity lookup p95 gecikme hedefi: 10 ms.\nSon satır.");
        let out = compress(&text, "warm exact entity lookup için p95 gecikme hedefi kaç milisaniye?", 200);
        assert!(out.contains("Warm exact entity lookup p95 gecikme hedefi: 10 ms."), "{out}");
        assert!(out.chars().count() <= 200 + 4, "{out}");
    }

    #[test]
    fn short_text_is_not_changed() {
        assert_eq!(compress("Kısa metin.", "metin", 100), "Kısa metin.");
    }
}
