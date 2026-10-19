//! Term coverage across projects (ADR-009).
//!
//! Ranking scores of different projects are not comparable: each project scales its own scores.
//! A query over all projects therefore ranks by the share of specific question terms that a result
//! contains. A term that occurs in the results of few projects has a high weight.

use std::collections::HashSet;

use zcie_types::{QueryResponse, ResultItem};

/// Question words that do not identify content (English and Turkish, lowercase).
/// Meta words such as "function", "file" or "commit" name the kind of an answer, not its content.
/// Command verbs ("run", "fix", "add") name the action of a prompt, not its subject. Turkish question
/// and answer-format words ("hangi", "kaç", "sadece sayıyı yaz") are not content either. The words
/// are compared after letter folding (`zcie_search::fold_char`).
const STOP_WORDS: &[&str] = &[
    "the", "and", "for", "that", "this", "with", "what", "which", "where", "when", "why", "how", "who", "does", "did",
    "are", "was", "were", "our", "my", "all", "every", "across", "any", "use", "uses", "used", "using", "define",
    "defines", "defined", "function", "functions", "name", "names", "project", "projects", "file", "files", "give",
    "only", "show", "tell", "list", "value", "values", "path", "constant", "many", "much", "get", "from", "into",
    "commit", "commits", "change", "changed", "changes", "line", "lines", "run", "fix", "add", "make", "please",
    "explain", "check", "update", "remove", "delete", "create", "write", "read", "open", "can", "you", "user",
    "hangi", "nedir", "neydi", "nasil", "nerede", "kim", "kimin", "kac", "ne", "neler", "icin", "ile", "gibi",
    "bu", "su", "mi", "mu", "var", "yok", "olan", "olarak", "daha", "cok", "ve", "veya", "ya", "da", "de",
    "devam", "lutfen", "bunu", "sunu", "bana", "biz", "siz", "ben", "sen", "tum", "butun", "evet", "hayir",
    "tek", "ver", "yaz", "kisa", "say", "sayi", "bir",
];
/// Stems of Turkish meta words (all inflections): "kullanıcının", "istedi", "projede", "cevap ver".
const STOP_STEMS: &[&str] = &[
    "kulla", "isted", "istiy", "istem", "isteg", "proje", "cevap", "cumle", "kisac", "sadec", "sayiy", "madde", "liste",
    "simdi", "soyle", "yazin", "konus", "hatir", "beyan", "koydu", "koyac", "yapti", "yapac", "yapiy", "verdi",
    "verec", "secti", "tutuy", "tutac", "olaca", "olmal", "edece", "ediyo", "etmel", "sonra",
];
/// Characters of a stem (the same as the stem field of the search index).
const STEM_CHARS: usize = zcie_search::STEM_CHARS;
/// Shortest word that counts as a term.
const MIN_TERM_CHARS: usize = 3;
/// Fewest question terms that one result must contain for a non-zero match coverage.
const MIN_MATCHED_TERMS: usize = 2;
/// Weight of a question term that only the context of a follow-up message has.
const CONTEXT_TERM_WEIGHT: f32 = 0.5;
/// Items with less than this share of the best coverage are dropped.
const KEEP_SHARE: f32 = 0.5;

/// Returns the lowercase stems of a text. Splits words at non-alphanumeric characters
/// (snake_case, kebab-case, paths) and at camelCase boundaries.
pub fn stems(text: &str) -> HashSet<String> {
    let mut out = HashSet::new();
    let mut word = String::new();
    let mut previous_lower = false;
    for c in zcie_search::drop_apostrophe_suffixes(&text.replace('İ', "I")).chars().chain(std::iter::once(' ')) {
        let boundary = !c.is_alphanumeric() || (c.is_uppercase() && previous_lower);
        if boundary && !word.is_empty() {
            push_stem(&mut out, &word);
            word.clear();
        }
        if c.is_alphanumeric() {
            word.extend(c.to_lowercase());
        }
        previous_lower = c.is_lowercase() || c.is_ascii_digit();
    }
    out
}

fn push_stem(out: &mut HashSet<String>, word: &str) {
    let folded: String = word.chars().map(zcie_search::fold_char).collect();
    if folded.chars().count() < MIN_TERM_CHARS || STOP_WORDS.contains(&folded.as_str()) {
        return;
    }
    let stem: String = base_form(&folded).chars().take(STEM_CHARS).collect();
    if !STOP_STEMS.contains(&stem.as_str()) {
        out.insert(stem);
    }
}

/// Removes a plural or verb ending: "applies" -> "apply", "retries" -> "retry", "fees" -> "fee".
fn base_form(word: &str) -> String {
    if let Some(stem) = word.strip_suffix("ies").filter(|s| s.chars().count() >= 3) {
        return format!("{stem}y");
    }
    match word.strip_suffix('s') {
        Some(stem) if stem.chars().count() >= 3 && !stem.ends_with('s') => stem.to_string(),
        _ => word.to_string(),
    }
}

/// Characters of the snippet of a long memory record or event (ADR-013).
pub const SNIPPET_WINDOW_CHARS: usize = 400;
/// Most window starts that `best_window` tries in one text.
const MAX_WINDOW_STARTS: usize = 400;

/// Returns the part of a long text (at most `width` characters, from a sentence or line start)
/// with the most question terms. A long pasted document thus shows the passage that answers the
/// question, not its first lines. A short text is returned as it is.
pub fn best_window(text: &str, question: &str, width: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= width {
        return text.to_string();
    }
    let terms = stems(question);
    let mut starts: Vec<usize> = std::iter::once(0)
        .chain(chars.iter().enumerate().filter(|(i, c)| matches!(c, '.' | '!' | '?' | '\n') && i + 1 < chars.len()).map(|(i, _)| i + 1))
        .collect();
    let step = starts.len().div_ceil(MAX_WINDOW_STARTS).max(1);
    starts = starts.into_iter().step_by(step).collect();
    let window = |start: usize| chars[start..(start + width).min(chars.len())].iter().collect::<String>();
    let score = |start: &usize| stems(&window(*start)).intersection(&terms).count();
    // On a tie the later start wins: the window then begins with the matching sentence and keeps
    // the text after it, instead of ending in the middle of the match.
    let best = starts.iter().copied().max_by(|a, b| score(a).cmp(&score(b)).then(a.cmp(b))).unwrap_or(0);
    let end = (best + width).min(chars.len());
    let prefix = if best > 0 { "… " } else { "" };
    let suffix = if end < chars.len() { " …" } else { "" };
    format!("{prefix}{}{suffix}", window(best).trim())
}

/// Shortest text that is not treated as a stored question even if its first sentence asks.
const QUESTION_MAX_CHARS: usize = 300;

/// True for a short stored message whose first sentence is a question: a prompt, not a fact.
pub fn looks_like_question(text: &str) -> bool {
    let first = text.split(['.', '!', '\n']).next().unwrap_or_default();
    text.chars().count() < QUESTION_MAX_CHARS && first.contains('?')
}

/// True for results that state a fact: code, commits, tasks, decisions and what the user said.
/// Raw conversation events (their memory records cover them) and stored questions do not count.
fn is_fact(item: &ResultItem) -> bool {
    let label = item.evidence_status.as_deref().unwrap_or_default();
    let text = item.snippet.as_deref().unwrap_or(&item.title);
    match item.kind.as_str() {
        "event" => false,
        "memory" => !label.starts_with("observation") && !looks_like_question(text),
        _ => true,
    }
}

/// Returns the share of `terms` (from `stems(question)`) in a result: title, path, and for a long
/// text the window that the snippet would show. Lexical scores of different indexes are not
/// comparable; this share is, so the ranking uses it too (ADR-013).
pub fn item_coverage(terms: &HashSet<String>, question: &str, item: &ResultItem) -> f32 {
    if terms.is_empty() {
        return 0.0;
    }
    let window = item.snippet.as_deref().map(|s| best_window(s, question, SNIPPET_WINDOW_CHARS)).unwrap_or_default();
    let text = format!("{} {} {}", item.title, item.path.as_deref().unwrap_or_default(), window);
    let own = matched_terms(terms, &stems(&text)) as f32;
    // The context of a follow-up message ("bunu da yap") names its topic; a term that only the
    // context has counts half, so that the message before it (which has the term) ranks first.
    let with_context = item.context.as_deref().map_or(own, |c| matched_terms(terms, &stems(&format!("{text} {c}"))) as f32);
    (own + (with_context - own) * CONTEXT_TERM_WEIGHT) / terms.len() as f32
}

/// Counts the terms that the text has: the same stem, a stem with a common prefix (short Turkish
/// roots and their inflected forms), or the same word in the other language ("araç" and "tool").
fn matched_terms(terms: &HashSet<String>, text_stems: &HashSet<String>) -> usize {
    let has = |term: &str| text_stems.contains(term) || text_stems.iter().any(|s| crate::synonyms::same_stem(s, term));
    terms.iter().filter(|t| has(t) || crate::synonyms::equivalents(t).any(has)).count()
}

/// Returns the best share of the question terms that one result contains (0.0 to 1.0).
/// A client can use it to decide if the results answer the question (ADR-009 section 5).
/// One matching term is not enough evidence: the value is 0.0 if the best result has fewer than
/// `MIN_MATCHED_TERMS` terms ("run the tests" must not match a symbol `run_tests`).
pub fn match_coverage(question: &str, results: &[ResultItem]) -> f32 {
    let terms = stems(question);
    if terms.is_empty() {
        return 0.0;
    }
    let facts: Vec<&ResultItem> = results.iter().filter(|item| is_fact(item)).collect();
    let best = facts.iter().map(|item| matched_terms(&terms, &item_stems(item))).max().unwrap_or(0);
    // A name (PostgreSQL, Elif, İyzico) or a version in the question that a statement has is strong
    // evidence on its own: "Hangi PostgreSQL sürümünü kullanıyoruz?" shares one word with its answer.
    // Code has its own rule (a symbol name in the prompt), so only statements count here.
    let names = name_terms(question);
    let statement = |item: &&&ResultItem| matches!(item.kind.as_str(), "memory" | "task" | "commit");
    let names_match = !names.is_empty() && facts.iter().filter(statement).any(|item| matched_terms(&names, &item_stems(item)) > 0);
    if best < MIN_MATCHED_TERMS {
        return if names_match { NAME_MATCH_COVERAGE } else { 0.0 };
    }
    let share = best as f32 / terms.len() as f32;
    let share = if names_match { share.max(NAME_MATCH_COVERAGE) } else { share };
    (share * 100.0).round() / 100.0
}

/// Coverage that a matching name or version gives at least.
const NAME_MATCH_COVERAGE: f32 = 0.6;

/// Common format and technology words written in capitals. They are not names of the project:
/// "JSON ile YAML arasındaki fark ne?" is a general question.
const GENERIC_NAMES: &[&str] = &[
    "json", "yaml", "toml", "xml", "csv", "html", "css", "http", "https", "url", "uri", "api", "rest", "sql", "cli",
    "readme", "todo", "utf", "ascii", "regex", "jwt", "tcp", "udp", "dns", "ssh", "ssl", "tls", "cpu", "gpu", "ram",
];

/// Returns the stems of the names in a question: words with a capital letter that do not start a
/// sentence (the first word always has one), and words with letters and digits ("v17", "s3").
/// Common format and technology words (`GENERIC_NAMES`) are not names.
fn name_terms(question: &str) -> HashSet<String> {
    let mut out = HashSet::new();
    let mut sentence_start = true;
    for word in question.split_whitespace() {
        let core: String = word.chars().filter(|c| c.is_alphanumeric() || *c == '\'').collect();
        let core = core.split('\'').next().unwrap_or_default();
        let capital = core.chars().skip(1).any(char::is_uppercase) || !sentence_start && core.chars().next().is_some_and(char::is_uppercase);
        let mixed = core.chars().any(|c| c.is_ascii_digit()) && core.chars().any(char::is_alphabetic);
        let generic = GENERIC_NAMES.contains(&core.to_lowercase().as_str());
        if core.chars().count() >= MIN_TERM_CHARS && (capital || mixed) && !generic {
            out.extend(stems(core));
        }
        sentence_start = word.ends_with(['.', '?', '!', ':']);
    }
    out
}

/// True if the question names something specific: a name or a version (`name_terms`), a path, a
/// ref (`#89`, `ABC-12`) or a project ID.
pub fn names_a_fact(question: &str) -> bool {
    let specific = |word: &str| word.contains('/') || word.starts_with('#') || word.starts_with("proj_") || word.starts_with("ws_") || (word.contains('-') && word.chars().any(|c| c.is_ascii_digit()));
    !name_terms(question).is_empty() || question.split_whitespace().any(specific)
}

fn item_stems(item: &ResultItem) -> HashSet<String> {
    let snippet = item.snippet.as_deref().unwrap_or_default();
    let text = format!("{} {} {snippet} {}", item.title, item.path.as_deref().unwrap_or_default(), item.context.as_deref().unwrap_or_default());
    stems(&text)
}

/// Returns (term, weight). Weight = ln((projects + 1) / (projects with the term + 0.5)).
fn weights(terms: &HashSet<String>, items: &[Vec<HashSet<String>>]) -> Vec<(String, f32)> {
    let projects = items.len() as f32;
    terms
        .iter()
        .map(|term| {
            let with_term = items.iter().filter(|project| project.iter().any(|s| s.contains(term))).count() as f32;
            (term.clone(), ((projects + 1.0) / (with_term + 0.5)).ln().max(0.0))
        })
        .collect()
}

/// Ranks the responses of a query over all projects by term coverage. Keeps only items with at
/// least half of the best coverage; drops projects with no such item; puts the best project first.
/// Changes nothing if the question has no content term or no result contains one.
pub fn rank_by_coverage(question: &str, responses: &mut Vec<(String, QueryResponse)>) {
    let terms = stems(question);
    let items: Vec<Vec<HashSet<String>>> = responses.iter().map(|(_, r)| r.results.iter().map(item_stems).collect()).collect();
    let weights = weights(&terms, &items);
    let total: f32 = weights.iter().map(|(_, w)| w).sum();
    if total <= 0.0 {
        return;
    }
    let cover = |s: &HashSet<String>| weights.iter().filter(|(t, _)| s.contains(t)).map(|(_, w)| w).sum::<f32>() / total;
    let coverage: Vec<Vec<f32>> = items.iter().map(|project| project.iter().map(cover).collect()).collect();
    let best = coverage.iter().flatten().copied().fold(0.0_f32, f32::max);
    if best <= 0.0 {
        return;
    }
    let mut ranked: Vec<(f32, (String, QueryResponse))> = Vec::new();
    for ((name, mut response), project) in responses.drain(..).zip(coverage) {
        let mut kept: Vec<(f32, ResultItem)> = project.into_iter().zip(response.results).filter(|(c, _)| *c >= best * KEEP_SHARE).collect();
        kept.sort_by(|a, b| b.0.total_cmp(&a.0));
        let top = kept.first().map_or(0.0, |(c, _)| *c);
        response.results = kept.into_iter().map(|(_, item)| item).collect();
        ranked.push((top, (name, response)));
    }
    ranked.sort_by(|a, b| b.0.total_cmp(&a.0));
    responses.extend(ranked.into_iter().filter(|(top, _)| *top > 0.0).map(|(_, pair)| pair));
}
