//! Capsule rules (ADR-017, D-056): no repeated text, and no capsule for a short prompt that names
//! no project fact. Unrelated near-topic text lowers the answers of a model ("Context Rot"), so a
//! response with nothing specific to say says nothing.

use std::collections::HashSet;

use zcie_types::ResultItem;

use crate::relevance::{match_coverage, names_a_fact, stems};

/// Switches of the context-efficiency behavior (`[context]` of `engine.toml`).
#[derive(Debug, Clone, Copy)]
pub struct ContextOptions {
    /// No duplicates, no capsule for short generic prompts, budgets per class.
    pub capsule_rules: bool,
    /// Token budget of the repository map; 0 turns the map off.
    pub repo_map_budget: u32,
    /// Personalized PageRank over the graph for multi-hop questions.
    pub graph_rank: bool,
    /// Extractive compression of snippets.
    pub compress: bool,
}

impl Default for ContextOptions {
    fn default() -> Self {
        Self { capsule_rules: true, repo_map_budget: 500, graph_rank: true, compress: true }
    }
}

/// Most content terms of a short prompt ("bunların planını oluştur" has 3).
const SHORT_PROMPT_TERMS: usize = 3;
/// Most words of a short prompt (the short generic prompts have 1 to 4 words; a question about a
/// fact usually has more: "Kullanıcı bu projenin doğruluğu konusunda ne bekliyor?").
const SHORT_PROMPT_WORDS: usize = 5;
/// Most content terms of a short prompt that gets no capsule with no pointing word ("hatayı bul").
const BARE_PROMPT_TERMS: usize = 2;
/// Words that point to the current conversation ("bunu düzelt", "explain this"), folded.
const POINTING_WORDS: [&str; 14] = ["bunu", "bunun", "buna", "bunda", "sunu", "onu", "onun", "this", "that", "these", "those", "it", "them", "above"];
/// Starts of pointing words with endings ("bunların", "şunları", "onlara").
const POINTING_STARTS: [&str; 3] = ["bunlar", "sunlar", "onlar"];

/// True when the prompt points to the current conversation, not to a project fact.
fn points_to_conversation(question: &str) -> bool {
    question.split(|c: char| !c.is_alphanumeric()).map(|w| w.to_lowercase().chars().map(zcie_search::fold_char).collect::<String>()).any(|w| {
        POINTING_WORDS.contains(&w.as_str()) || POINTING_STARTS.iter().any(|p| w.starts_with(p))
    })
}
/// Shortest snippet that counts as a full text for the duplicate check.
const MIN_DUPLICATE_CHARS: usize = 80;

/// Returns `Some(false)` when the prompt must get no capsule. The prompt has no local answer, at
/// most `SHORT_PROMPT_WORDS` words, at most `SHORT_PROMPT_TERMS` content words and no name, ref,
/// path or version, and one of these:
/// - it points to the current conversation ("bunların planını oluştur", "explain this");
/// - it has at most `BARE_PROMPT_TERMS` content words, and it is an instruction (no "?") or no
///   fact has all of its words ("testleri çalıştır", "neden çalışmıyor?").
///
/// A short question about a fact ("Para tutarlarını hangi tipte tutuyoruz?") keeps its capsule.
/// `None` leaves the decision to the clients (the usual gate).
pub fn capsule_flag(question: &str, items: &[ResultItem], has_answer: bool) -> Option<bool> {
    let terms = stems(question).len();
    let words = question.split_whitespace().count();
    if has_answer || words > SHORT_PROMPT_WORDS || terms > SHORT_PROMPT_TERMS || names_a_fact(question) {
        return None;
    }
    // A short instruction ("testleri çalıştır", "hatayı bul") asks for work now, not for a fact.
    let bare = terms <= BARE_PROMPT_TERMS && (!question.contains('?') || match_coverage(question, items) < 1.0);
    (points_to_conversation(question) || bare).then_some(false)
}

fn text_key(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase()
}

/// Keeps the first item of each full text: two records of one message (two decision sentences of
/// one answer) show the same message once.
pub fn drop_repeated_snippets(items: Vec<ResultItem>) -> Vec<ResultItem> {
    let mut seen = HashSet::new();
    items
        .into_iter()
        .filter(|item| match item.snippet.as_deref().filter(|s| s.chars().count() >= MIN_DUPLICATE_CHARS) {
            Some(snippet) if matches!(item.kind.as_str(), "memory" | "event") => seen.insert(text_key(snippet)),
            _ => true,
        })
        .collect()
}

/// Applies the rules to a composed response and its items (nothing when they are off).
pub fn apply(mut base: zcie_types::QueryResponse, items: Vec<ResultItem>, question: &str, options: ContextOptions) -> (zcie_types::QueryResponse, Vec<ResultItem>) {
    if !options.capsule_rules {
        return (base, items);
    }
    let items = drop_repeated_snippets(items);
    base.capsule = capsule_flag(question, &items, base.answer.is_some());
    (base, items)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_prompts_about_the_conversation_get_no_capsule() {
        for prompt in ["bunların planını oluştur", "bunu düzelt", "hatayı bul", "Şunu açıkla", "explain this"] {
            assert_eq!(capsule_flag(prompt, &[], false), Some(false), "{prompt}");
        }
    }

    #[test]
    fn short_questions_about_facts_and_answers_keep_the_capsule() {
        assert_eq!(capsule_flag("Para tutarlarını hangi tipte tutuyoruz? Tek cümleyle cevap ver.", &[], false), None);
        assert_eq!(capsule_flag("Kullanıcı bu projenin doğruluğu konusunda ne bekliyor?", &[], false), None);
        assert_eq!(capsule_flag("bunu düzelt", &[], true), None, "a local answer");
        assert_eq!(capsule_flag("fix #12", &[], false), None, "a ref");
    }
}
