//! Normalization and rule-based classification (`docs/09-query-and-context.md` section 3).
//!
//! Each class scores its keywords (a phrase counts 2, a stem or a whole word 1). The highest score
//! wins; ties keep the order of `classify`.

use std::sync::OnceLock;

use regex::Regex;
use zcie_types::{QueryClass, QueryMode};

use crate::keyword_lists::*;
use crate::keywords::{exact_hits, exact_word, fold, has_code_name, hits, matches, near_hits, near_word, where_word};

/// Trims the question and replaces runs of white space with one space.
pub fn normalize(question: &str) -> String {
    question.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Returns true if the question has a task ref.
pub fn has_task_ref(question: &str) -> bool {
    task_ref_pattern().is_some_and(|p| p.is_match(question))
}

fn ref_pattern() -> Option<&'static Regex> {
    static PATTERN: OnceLock<Option<Regex>> = OnceLock::new();
    PATTERN.get_or_init(|| Regex::new(r"^(ev|file|sym|mem|task|commit):\S+$").ok()).as_ref()
}

fn task_ref_pattern() -> Option<&'static Regex> {
    static PATTERN: OnceLock<Option<Regex>> = OnceLock::new();
    PATTERN
        .get_or_init(|| Regex::new(r"(?:^|\s|\()#\d{1,7}\b|\b[A-Z][A-Z0-9]{1,9}-\d{1,7}\b|\btask:task_[0-9A-Z]{26}\b").ok())
        .as_ref()
}

/// Classifies a question. `is_symbol` tells if a single token is an exact symbol name.
pub fn classify(question: &str, mode: QueryMode, is_symbol: &dyn Fn(&str) -> bool) -> QueryClass {
    let normalized = normalize(question);
    if ref_pattern().is_some_and(|p| p.is_match(&normalized)) {
        return QueryClass::ExactRef;
    }
    match mode {
        QueryMode::Search | QueryMode::Memory => return QueryClass::Search,
        QueryMode::Timeline => return QueryClass::Timeline,
        QueryMode::Exact if !has_task_ref(&normalized) => return QueryClass::SymbolDefinition,
        _ => {}
    }
    let candidates = [
        (QueryClass::SymbolDefinition, definition_score(&normalized)),
        (QueryClass::Callers, callers_score(&normalized)),
        (QueryClass::Timeline, timeline_score(&normalized)),
        (QueryClass::Implementation, implementation_score(&normalized)),
        (QueryClass::Task, u32::from(has_task_ref(&normalized))),
    ];
    let mut best = (QueryClass::Search, 0);
    for (class, score) in candidates {
        if score > best.1 {
            best = (class, score);
        }
    }
    if best.1 > 0 {
        return best.0;
    }
    if !normalized.contains(' ') && is_symbol(&normalized) {
        return QueryClass::SymbolDefinition;
    }
    QueryClass::Search
}

/// True if a "where" (or a typo of it) occurs and one of `words` occurs in the question.
fn where_with(text: &str, words: &[&str]) -> bool {
    where_word(text) && near_hits(text, words) > 0
}

/// True for a short question with a code name and a where or nerede: "tenant_scope trait'i nerede?".
/// A callers word in the question makes it a callers question instead.
fn short_code_name_query(text: &str) -> bool {
    text.split_whitespace().count() <= 6 && (exact_word(text, "nerede") || where_word(text)) && has_code_name(text) && exact_hits(text, CALLER_WHERE) == 0
}

fn definition_score(text: &str) -> u32 {
    let declaration = exact_hits(text, DEFINITION_ASK) > 0 && exact_hits(text, DEFINITION_SHAPE) > 0 && has_code_name(text);
    2 * hits(text, DEFINITION_PHRASES)
        + hits(text, DEFINITION_STEMS)
        + 2 * u32::from(where_with(text, DEFINITION_WHERE))
        + 2 * exact_hits(text, DEFINITION_VERBS)
        + u32::from(declaration)
        + 2 * u32::from(short_code_name_query(text))
}

fn callers_score(text: &str) -> u32 {
    let cue = exact_hits(text, CALLER_CUES) > 0;
    let verb = hits(text, CALLER_VERB_STEMS) > 0 || exact_hits(text, CALLER_VERB_WORDS) > 0;
    let use_at_place = exact_hits(text, CALLER_USE_WORDS) > 0 && exact_hits(text, CALLER_PLACES) > 0;
    2 * hits(text, CALLER_PHRASES)
        + hits(text, CALLER_STEMS)
        + exact_hits(text, CALLER_WORDS)
        + 2 * near_hits(text, CALLER_NEAR)
        + 2 * u32::from(where_with(text, CALLER_WHERE))
        + 2 * u32::from(cue && verb)
        + 2 * u32::from(use_at_place)
        + 2 * u32::from(call_on_code(text))
}

/// A call or use verb on a named code thing ("how is AudioMixer called", "why does X use Y"), not a call chain.
fn call_on_code(text: &str) -> bool {
    // A flow (a call chain, "trace … from A to B") or an imperative edit ("Remove … nothing uses it") is not a callers question.
    let flow = hits(text, CALL_FLOW_PHRASES) > 0 || matches(text, "trace") || (text.contains(" from ") && text.contains(" to "));
    let imperative = fold(text).split_whitespace().next().is_some_and(|w| EDIT_VERBS.contains(&w));
    let turkish_use = matches(text, "kullaniyor") && exact_hits(text, USE_WHY_CUES) > 0;
    !flow && !imperative && has_code_name(text) && (exact_hits(text, CALL_WORDS) > 0 || hits(text, CALL_STEMS) > 0 || turkish_use)
}

fn timeline_score(text: &str) -> u32 {
    // "history of" is a task's status history when a task ref is present (a task, not the past conversation).
    let history_of_task = has_task_ref(text) && matches(text, "history of");
    let phrases = hits(text, TIMELINE_PHRASES).saturating_sub(u32::from(history_of_task));
    let user_intent = matches(text, "kullanici") && hits(text, USER_INTENT) > 0;
    let team_said = (matches(text, "ekip") || matches(text, "takim")) && hits(text, TEAM_SAID) > 0;
    2 * phrases
        + hits(text, TIMELINE_STEMS)
        + exact_hits(text, TIMELINE_WORDS)
        + 2 * u32::from(user_intent)
        + 2 * u32::from(team_said)
        + 2 * u32::from(asks_past_we(text))
}

/// A past-we verb ("onayladık", "bıraktık", "konuştuk") with a question cue ("hangi", "neden", "geçen").
fn asks_past_we(text: &str) -> bool {
    let past = fold(text).split(|c: char| !c.is_alphanumeric()).any(|w| PAST_WE_SUFFIXES.iter().any(|s| w.ends_with(*s)));
    past && exact_hits(text, PAST_CUES) > 0
}

fn implementation_score(text: &str) -> u32 {
    let noun = near_word(text, "commit")
        || exact_hits(text, IMPLEMENTATION_WORD_NOUNS) > 0
        || hits(text, IMPLEMENTATION_PHRASE_NOUNS) > 0;
    let verb = hits(text, IMPLEMENTATION_VERBS) > 0;
    // "when did X land" / "when was X added": the change that brought X in.
    let when_landed = (matches(text, "when did") || matches(text, "when was")) && (exact_hits(text, WHEN_IMPL_WORDS) > 0 || verb);
    3 * hits(text, IMPLEMENTATION_PHRASES) + 2 * u32::from(noun && verb) + 2 * u32::from(when_landed)
}

/// Returns the symbol name in a definition question, or the question itself.
pub fn symbol_name_in(question: &str) -> String {
    let words: Vec<&str> = question
        .split(|c: char| c.is_whitespace() || c == '?' || c == '`' || c == '\'' || c == '"')
        .filter(|w| !w.is_empty())
        .collect();
    let stop = [
        "where", "is", "defined", "definition", "of", "the", "nerede", "tanımlı", "tanımı", "who", "calls", "callers", "kim",
        "çağırıyor", "struct", "fn", "function", "class", "method", "trait", "enum", "impl", "def", "interface", "type",
        "const", "module", "mod", "find", "show",
    ];
    let best = words
        .iter()
        .filter(|w| !stop.contains(&w.to_lowercase().as_str()))
        .max_by_key(|w| (w.contains('_') || w.contains("::") || w.chars().skip(1).any(char::is_uppercase), w.len()));
    best.map(|w| w.trim_end_matches("()").to_string()).unwrap_or_else(|| question.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn class(question: &str) -> QueryClass {
        classify(question, QueryMode::Auto, &|_| false)
    }

    #[test]
    fn definitions_callers_and_timeline_need_their_cues() {
        assert_eq!(class("Where is apply_late_fee defined?"), QueryClass::SymbolDefinition);
        assert_eq!(class("The config values are defined in TOML, how does it load?"), QueryClass::Search);
        assert_eq!(class("bu function'ı kim call ediyor: apply_late_fee"), QueryClass::Callers);
        assert_eq!(class("bu kararlı bir cache, nasıl çalışıyor?"), QueryClass::Search);
        assert_eq!(class("bu neden çalışmıyor"), QueryClass::Search);
        assert_eq!(class("Kafka planını neden bıraktık?"), QueryClass::Timeline);
        assert_eq!(class("Is parse_config implemented yet?"), QueryClass::Search);
        assert_eq!(class("hangi commit ekledi apply_late_fee'yi?"), QueryClass::Implementation);
        assert_eq!(class("what's the status history of ZC-204"), QueryClass::Task);
        assert_eq!(class("Where would the tenant be decided, there?"), QueryClass::Search);
    }
}
