//! Intent selection (ADR-019, D-068): the next action that a prompt asks for, from fixed phrases
//! and the query class, with no model. The intent selects the evidence of the response.

use zcie_types::{QueryClass, QueryIntent, QueryMode};

/// English verbs that start an instruction to change code.
const EDIT_VERBS: [&str; 16] = [
    "add", "rename", "change", "remove", "delete", "replace", "implement", "fix", "apply", "refactor", "update", "move",
    "insert", "extract", "make", "set",
];
/// Turkish verb stems of an instruction to change code (the verb ends the sentence).
const EDIT_STEMS_TR: [&str; 8] = ["ekle", "değiştir", "düzelt", "sil", "kaldır", "uygula", "güncelle", "taşı"];
/// Phrases of an explanation question.
const EXPLAIN_PHRASES: [&str; 12] = [
    "explain", "how does", "how do ", "how is ", "what does", "why does", "nasıl çalış", "açıkla", "ne yapıyor", "ne hesaplıyor",
    "neden çalış", "nasıl yapıyor",
];
/// Phrases of a question about earlier decisions or history.
const MEMORY_PHRASES: [&str; 8] = ["why did ", "did we ", "we decided", "we agreed", "daha önce", "karar verdik", "karar vermiştik", "neden vazgeç"];

/// Phrases of a flow question.
const TRACE_PHRASES: [&str; 5] = ["trace ", "call chain", "call path", "akışı", "akışını"];

/// True when the first word is an English edit verb, or the first sentence ends with a Turkish
/// edit verb ("…'e bir metot ekle").
fn asks_for_edit(lower: &str) -> bool {
    let first = lower.split_whitespace().next().unwrap_or_default().trim_matches(|c: char| !c.is_alphanumeric());
    if EDIT_VERBS.contains(&first) {
        return true;
    }
    let sentence = lower.split(['.', '?', '!', '\n']).next().unwrap_or_default().trim();
    EDIT_STEMS_TR.iter().any(|stem| sentence.split_whitespace().last().is_some_and(|w| w.starts_with(stem)))
}

/// True for a question about a flow: "trace …" or "from X to Y" with code names.
fn asks_for_trace(lower: &str) -> bool {
    TRACE_PHRASES.iter().any(|p| lower.contains(p)) || (lower.contains(" from ") && lower.contains(" to ") && lower.contains("::"))
}

/// True for a question about decisions or history. "Which commit implemented #89" is history;
/// "in which file is X implemented" is a code location.
fn asks_for_memory(lower: &str, class: QueryClass) -> bool {
    let history = matches!(class, QueryClass::Timeline | QueryClass::Task)
        || (class == QueryClass::Implementation && (lower.contains("commit") || lower.contains('#')));
    history || MEMORY_PHRASES.iter().any(|p| lower.contains(p))
}

/// Selects the intent of a prompt.
pub fn select_intent(question: &str, mode: QueryMode, class: QueryClass) -> QueryIntent {
    let lower = question.to_lowercase();
    match mode {
        QueryMode::Locate => return QueryIntent::Locate,
        QueryMode::Memory | QueryMode::Timeline => return QueryIntent::Memory,
        QueryMode::Map => return QueryIntent::General,
        _ => {}
    }
    if crate::locate::asks_for_location(&lower) {
        QueryIntent::Locate
    } else if asks_for_edit(&lower) {
        QueryIntent::Edit
    } else if asks_for_trace(&lower) {
        QueryIntent::Trace
    } else if asks_for_memory(&lower, class) {
        QueryIntent::Memory
    } else if EXPLAIN_PHRASES.iter().any(|p| lower.contains(p)) {
        QueryIntent::Explain
    } else {
        QueryIntent::General
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn intent(question: &str, class: QueryClass) -> QueryIntent {
        select_intent(question, QueryMode::Auto, class)
    }

    #[test]
    fn prompts_get_their_intent() {
        assert_eq!(intent("Add a public method `num_threads` to IndexWriter", QueryClass::Search), QueryIntent::Edit);
        assert_eq!(intent("Rename the function a_b to c_d everywhere", QueryClass::Search), QueryIntent::Edit);
        assert_eq!(intent("IndexWriter'a yeni bir metot ekle", QueryClass::Search), QueryIntent::Edit);
        assert_eq!(intent("Which file must I change so that x is lower?", QueryClass::Search), QueryIntent::Locate);
        assert_eq!(intent("Explain how garbage collection works", QueryClass::Search), QueryIntent::Explain);
        assert_eq!(intent("Trace a document from IndexWriter::add_document to a segment", QueryClass::Search), QueryIntent::Trace);
        assert_eq!(intent("What did we decide about the merge policy?", QueryClass::Timeline), QueryIntent::Memory);
        assert_eq!(intent("Where is Bm25Weight defined?", QueryClass::SymbolDefinition), QueryIntent::General);
        assert_eq!(intent("Bu fonksiyon nasıl çalışıyor?", QueryClass::Search), QueryIntent::Explain);
    }

    #[test]
    fn a_mode_sets_the_intent() {
        assert_eq!(select_intent("anything", QueryMode::Locate, QueryClass::Search), QueryIntent::Locate);
        assert_eq!(select_intent("anything", QueryMode::Memory, QueryClass::Search), QueryIntent::Memory);
    }
}
