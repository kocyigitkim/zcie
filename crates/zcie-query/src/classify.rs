//! Normalization and rule-based classification (`docs/09-query-and-context.md` section 3).

use std::sync::OnceLock;

use regex::Regex;
use zcie_types::{QueryClass, QueryMode};

/// Keywords of the `SymbolDefinition` class.
pub const DEFINITION_KEYWORDS: &[&str] = &["definition of", "defined", "nerede tanımlı", "tanımı"];
/// Keywords of the `Callers` class.
pub const CALLER_KEYWORDS: &[&str] = &["who calls", "callers of", "kim çağırıyor"];
/// Keywords of the `Timeline` class.
pub const TIMELINE_KEYWORDS: &[&str] = &[
    "what did i ask",
    "what did we do",
    "what changed since",
    "history of",
    "timeline",
    "daha önce",
    "ne yapmıştık",
    "ne yaptık",
    "did we decide",
    "we decided",
    "decision",
    "agreed",
    "any rule",
    "why did we",
    "karar",
    "neden",
    "kural",
];
/// Keywords of the `Implementation` class.
pub const IMPLEMENTATION_KEYWORDS: &[&str] = &["which commit", "implemented", "implements", "hangi commit"];

/// Trims the question and replaces runs of white space with one space.
pub fn normalize(question: &str) -> String {
    question.split_whitespace().collect::<Vec<_>>().join(" ")
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

/// Returns true if the question has a task ref.
pub fn has_task_ref(question: &str) -> bool {
    task_ref_pattern().is_some_and(|p| p.is_match(question))
}

/// Classifies a question. `is_symbol` tells if a single token is an exact symbol name.
pub fn classify(question: &str, mode: QueryMode, is_symbol: &dyn Fn(&str) -> bool) -> QueryClass {
    let normalized = normalize(question);
    let lower = normalized.to_lowercase();
    if ref_pattern().is_some_and(|p| p.is_match(&normalized)) {
        return QueryClass::ExactRef;
    }
    match mode {
        QueryMode::Search | QueryMode::Memory => return QueryClass::Search,
        QueryMode::Timeline => return QueryClass::Timeline,
        QueryMode::Exact if !has_task_ref(&normalized) => return QueryClass::SymbolDefinition,
        _ => {}
    }
    if contains_any(&lower, DEFINITION_KEYWORDS) || (lower.starts_with("where is ") && lower.contains("defined")) {
        return QueryClass::SymbolDefinition;
    }
    if contains_any(&lower, CALLER_KEYWORDS) {
        return QueryClass::Callers;
    }
    if contains_any(&lower, TIMELINE_KEYWORDS) {
        return QueryClass::Timeline;
    }
    if contains_any(&lower, IMPLEMENTATION_KEYWORDS) {
        return QueryClass::Implementation;
    }
    if has_task_ref(&normalized) {
        return QueryClass::Task;
    }
    if !normalized.contains(' ') && is_symbol(&normalized) {
        return QueryClass::SymbolDefinition;
    }
    QueryClass::Search
}

fn contains_any(text: &str, keywords: &[&str]) -> bool {
    keywords.iter().any(|k| text.contains(k))
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
