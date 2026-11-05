//! Intent selection (ADR-019, D-068): the next action that a prompt asks for, from fixed phrases
//! and the query class, with no model. The intent selects the evidence of the response.

use zcie_types::{QueryClass, QueryIntent, QueryMode};

use crate::keyword_lists::*;
use crate::keywords::{exact_hits, exact_word, fold, has_code_name, has_token, hits, matches, where_word};

/// Selects the intent of a prompt.
pub fn select_intent(question: &str, mode: QueryMode, class: QueryClass) -> QueryIntent {
    match mode {
        QueryMode::Locate => return QueryIntent::Locate,
        QueryMode::Memory | QueryMode::Timeline => return QueryIntent::Memory,
        QueryMode::Map => return QueryIntent::General,
        _ => {}
    }
    // "where is X defined" and "who calls X" are about a definition or a call site, not a location to change.
    let definition_or_callers = matches!(class, QueryClass::SymbolDefinition | QueryClass::Callers);
    if !definition_or_callers && (crate::locate::asks_for_location(question) || asks_where_is_code(question)) {
        QueryIntent::Locate
    } else if asks_for_edit(question, class) {
        QueryIntent::Edit
    } else if class == QueryClass::Timeline {
        QueryIntent::Memory
    } else if !definition_or_callers && asks_for_location_family(question) {
        QueryIntent::Locate
    } else if asks_for_trace(question) {
        QueryIntent::Trace
    } else if asks_for_memory(question, class) {
        QueryIntent::Memory
    } else if asks_for_explain(question) {
        QueryIntent::Explain
    } else {
        QueryIntent::General
    }
}

/// Words of a folded text: whitespace-separated, with the outer punctuation removed. Identifiers
/// keep their underscores ("apply_late_fee" is not "apply").
fn words(text: &str) -> impl DoubleEndedIterator<Item = &str> {
    text.split_whitespace().map(|w| w.trim_matches(|c: char| !c.is_alphanumeric())).filter(|w| !w.is_empty())
}

/// Where is the code or which file: a location. A status question ("ZC-19 nerede kaldı") is not.
fn asks_where_is_code(question: &str) -> bool {
    let location = hits(question, LOCATE_PHRASES) > 0 || LOCATE_WORDS.iter().any(|w| exact_word(question, w));
    location && hits(question, STATUS_PHRASES) == 0
}

/// A "hangi modülde / dosyada / katmanda" question: a noun of a code place with a locative suffix.
fn asks_for_locative_place(question: &str) -> bool {
    let folded = fold(question);
    let locative = |w: &str| LOCATE_NOUNS.iter().any(|n| w.starts_with(n)) && (w.ends_with("de") || w.ends_with("da") || w.ends_with("te") || w.ends_with("ta"));
    (exact_word(question, "hangi") || exact_word(question, "which")) && words(&folded).any(locative)
}

/// "Which file/module/crate … change / computes / implemented / has", and "where would I put /
/// where does the app read": a code place is asked for, not a change.
fn asks_for_location_family(question: &str) -> bool {
    let status = hits(question, STATUS_PHRASES) > 0;
    // "which file does X" is a question about a place; "how does X … the file it is in" is not.
    let which_does = exact_word(question, "which") && exact_word(question, "does");
    let place = hits(question, LOCATE_NOUNS) > 0 && (hits(question, LOCATE_VERBS) > 0 || exact_hits(question, LOCATE_HAS_WORDS) > 0 || which_does);
    let where_verb = where_word(question) && exact_hits(question, WHERE_LOCATE_WORDS) > 0;
    let where_noun = where_word(question) && exact_hits(question, WHERE_LOCATE_NOUNS) > 0;
    !status && (place || where_verb || where_noun || asks_for_locative_place(question))
}

/// True when the first word after the polite prefixes is an English edit verb, or the first
/// sentence ends with a Turkish edit verb. A history question ("hangi commit … ekledi") is not an edit.
fn asks_for_edit(question: &str, class: QueryClass) -> bool {
    let folded = fold(question);
    let all: Vec<&str> = words(&folded).collect();
    let first = all.iter().copied().find(|w| !EDIT_PREFIXES.contains(w)).unwrap_or_default();
    if CREATE_VERBS.contains(&first) {
        // "write a haiku" is not a code change; "write a unit test" is.
        return has_code_name(question) || all.iter().any(|w| CODE_NOUNS.contains(w));
    }
    if EDIT_VERBS.contains(&first) {
        return true;
    }
    if class == QueryClass::Implementation {
        return false;
    }
    let sentence = folded.split(['.', '?', '!', '\n']).next().unwrap_or_default().trim_end();
    if sentence.ends_with("refactor et") || sentence.ends_with("refactor edin") {
        return true;
    }
    let mut tail = words(sentence).rev();
    let Some(mut last) = tail.next() else { return false };
    if QUESTION_PARTICLES.contains(&last) {
        let Some(verb) = tail.next() else { return false };
        last = verb;
    }
    EDIT_WORDS_TR.contains(&last) || EDIT_STEMS_TR.iter().any(|s| last.starts_with(*s))
}

/// True for a question about a flow: "trace …", a flow or sequence, "akışı", "izini sür", a call
/// chain, "from X to Y" with a flow marker, or "from X (-den) up to Y (-e kadar)".
fn asks_for_trace(question: &str) -> bool {
    let folded = format!(" {} ", fold(question));
    let ablative_to_place = has_token(&folded, "kadar") && ABLATIVE_TOKENS.iter().any(|t| has_token(&folded, t));
    hits(&folded, TRACE_PHRASES) > 0
        || TRACE_STEMS_TR.iter().any(|s| matches(&folded, s))
        || exact_hits(&folded, TRACE_WORDS) > 0
        || ablative_to_place
        || (folded.contains(" from ") && folded.contains(" to ") && (TRACE_FROM_TO_MARKERS.iter().any(|k| folded.contains(*k)) || exact_hits(&folded, TRACE_FROM_TO_TOKENS) > 0))
}

/// True for a question about decisions or history: a timeline, task or implementation class, a
/// past-conversation phrase, or "why" about a decision ("why did X replace Y").
fn asks_for_memory(question: &str, class: QueryClass) -> bool {
    let decision_why = (exact_word(question, "why") || exact_word(question, "neden")) && exact_hits(question, DECISION_WORDS) > 0;
    matches!(class, QueryClass::Timeline | QueryClass::Task | QueryClass::Implementation) || hits(question, MEMORY_PHRASES) > 0 || decision_why
}

/// True for "how/why does code work": explain phrases, any "why" or "neden", "nasıl" with a
/// present-tense verb or an explain word ("nasıl … ediyor, kısaca anlat"), and "ne" + a verb.
fn asks_for_explain(question: &str) -> bool {
    let folded = fold(question);
    let all: Vec<&str> = words(&folded).collect();
    let how = has_token(&folded, "nasil") && (all.iter().any(|w| w.ends_with("yor")) || EXPLAIN_HOW_WORDS.iter().any(|w| has_token(&folded, w)));
    let ne_verb = all.windows(2).any(|p| p[0] == "ne" && p[1].ends_with("yor"));
    // "why" is a question about code only with a code name or a code noun ("write a summary of why teams …" is not).
    let why = exact_word(question, "neden") || exact_word(question, "why");
    let progressive = all.iter().any(|w| w.ends_with("yor"));
    let why_about_code = why && (has_code_name(question) || hits(question, EXPLAIN_CODE_NOUNS) > 0 || progressive);
    hits(question, EXPLAIN_PHRASES) > 0 || why_about_code || how || ne_verb
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
        assert_eq!(intent("hangi commit parse_config'i değiştirdi?", QueryClass::Implementation), QueryIntent::Memory);
        assert_eq!(intent("Can you add a test for parse_config?", QueryClass::Search), QueryIntent::Edit);
        assert_eq!(intent("Give the function name and the file path.", QueryClass::Search), QueryIntent::General);
    }

    #[test]
    fn a_mode_sets_the_intent() {
        assert_eq!(select_intent("anything", QueryMode::Locate, QueryClass::Search), QueryIntent::Locate);
        assert_eq!(select_intent("anything", QueryMode::Memory, QueryClass::Search), QueryIntent::Memory);
    }
}
