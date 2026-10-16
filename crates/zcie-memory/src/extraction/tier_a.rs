//! Tier A deterministic patterns (`docs/08-memory-and-graph.md` section 3.1).
//!
//! This module only finds text matches. It does not read or write the store.

use std::collections::BTreeSet;
use std::sync::OnceLock;

use regex::Regex;

/// Maximum number of error lines (X-06) per event.
const MAX_ERROR_LINES: usize = 5;
/// Minimum length of a symbol-like name (X-02).
const MIN_SYMBOL_LEN: usize = 4;

/// All matches of the Tier A patterns in one text.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Matches {
    /// X-01 file paths.
    pub paths: Vec<String>,
    /// X-02 symbol-like names.
    pub symbol_names: Vec<String>,
    /// X-03 external task refs, for example `#89` or `JIRA-12`.
    pub external_task_refs: Vec<String>,
    /// X-04 internal task IDs (`task_...`).
    pub internal_task_ids: Vec<String>,
    /// X-05 hex strings that can be commit SHAs.
    pub shas: Vec<String>,
    /// X-06 error lines.
    pub error_lines: Vec<String>,
    /// X-07 sentences with a decision marker.
    pub decisions: Vec<String>,
    /// X-08 sentences that start with a preference marker.
    pub preferences: Vec<String>,
}

struct Patterns {
    path: Regex,
    symbol: Regex,
    hash_ref: Regex,
    key_ref: Regex,
    task_id: Regex,
    sha: Regex,
    error_line: Regex,
    decision: Regex,
    preference: Regex,
}

fn patterns() -> Option<&'static Patterns> {
    static PATTERNS: OnceLock<Option<Patterns>> = OnceLock::new();
    PATTERNS.get_or_init(build_patterns).as_ref()
}

fn build_patterns() -> Option<Patterns> {
    Some(Patterns {
        path: Regex::new(r"(?:[\w.-]+/)+[\w.-]+\.\w{1,8}").ok()?,
        symbol: Regex::new(r"\b[A-Za-z_][A-Za-z0-9_]*(?:(?:::|\.)[A-Za-z_][A-Za-z0-9_]*)*\b").ok()?,
        hash_ref: Regex::new(r"(?:^|\s|\()#(\d{1,7})\b").ok()?,
        key_ref: Regex::new(r"\b([A-Z][A-Z0-9]{1,9}-\d{1,7})\b").ok()?,
        task_id: Regex::new(r"\btask:(task_[0-9A-HJKMNP-TV-Z]{26})\b").ok()?,
        sha: Regex::new(r"\b[0-9a-f]{7,40}\b").ok()?,
        error_line: Regex::new(
            r"(?m)^.*(?:error\[E\d{4}\]|panicked at|Traceback \(most recent call last\)|\b\w+(?:Error|Exception)\b:).*$",
        )
        .ok()?,
        decision: Regex::new(r"(?i)\b(?:we decided|decision:|we will use|karar:|karar verdik)").ok()?,
        preference: Regex::new(r"(?i)^(?:always|never|prefer|her zaman|asla|tercih)\b").ok()?,
    })
}

/// Finds all Tier A matches in a text.
pub fn find_matches(text: &str) -> Matches {
    let Some(p) = patterns() else {
        tracing::error!("tier A patterns failed to compile");
        return Matches::default();
    };
    let paths = unique(p.path.find_iter(text).map(|m| m.as_str().to_string()));
    Matches {
        symbol_names: unique(
            p.symbol
                .find_iter(text)
                .map(|m| m.as_str().to_string())
                .filter(|s| is_symbol_like(s) && !paths.iter().any(|path| path.contains(s.as_str()))),
        ),
        paths,
        external_task_refs: unique(
            p.hash_ref
                .captures_iter(text)
                .filter_map(|c| c.get(1).map(|m| format!("#{}", m.as_str())))
                .chain(p.key_ref.captures_iter(text).filter_map(|c| c.get(1).map(|m| m.as_str().to_string()))),
        ),
        internal_task_ids: unique(p.task_id.captures_iter(text).filter_map(|c| c.get(1).map(|m| m.as_str().to_string()))),
        shas: unique(p.sha.find_iter(text).map(|m| m.as_str().to_string()).filter(|s| has_letter_and_digit(s))),
        error_lines: unique(p.error_line.find_iter(text).map(|m| m.as_str().trim().to_string())).into_iter().take(MAX_ERROR_LINES).collect(),
        decisions: sentences(text).filter(|s| p.decision.is_match(s)).collect(),
        preferences: sentences(text).filter(|s| p.preference.is_match(s)).collect(),
    }
}

/// Returns true for a name with 4 or more characters and a `_`, `::`, `.` or an inner capital letter.
pub fn is_symbol_like(name: &str) -> bool {
    if name.len() < MIN_SYMBOL_LEN {
        return false;
    }
    let inner_capital = name.chars().skip(1).any(|c| c.is_ascii_uppercase())
        && name.chars().any(|c| c.is_ascii_lowercase());
    name.contains('_') || name.contains("::") || name.contains('.') || inner_capital
}

fn has_letter_and_digit(value: &str) -> bool {
    value.chars().any(|c| c.is_ascii_digit()) && value.chars().any(|c| c.is_ascii_alphabetic())
}

/// Splits a text into sentences. A sentence ends at `.`, `!` or `?` followed by white space,
/// or at a line end. A dot inside a number or a name (`0.75`, `a.rs`) does not end a sentence.
fn sentences(text: &str) -> impl Iterator<Item = String> + '_ {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut current = String::new();
    for (index, c) in chars.iter().enumerate() {
        let next_is_space = chars.get(index + 1).is_none_or(|n| n.is_whitespace());
        if *c == '\n' || (matches!(c, '.' | '!' | '?') && next_is_space) {
            out.push(std::mem::take(&mut current));
            continue;
        }
        current.push(*c);
    }
    out.push(current);
    out.into_iter().map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

fn unique(values: impl Iterator<Item = String>) -> Vec<String> {
    let mut seen = BTreeSet::new();
    values.filter(|v| seen.insert(v.clone())).collect()
}
