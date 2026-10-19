//! The text that represents an entity in the vector index. Identifiers are split into words
//! ("applyLateFee" and "apply_late_fee" give "apply late fee"), because the model knows words.

/// Most characters of the text of one entity.
const MAX_TEXT_CHARS: usize = 600;
/// Most comment characters taken from the source of a symbol.
const MAX_COMMENT_CHARS: usize = 300;

/// Splits identifiers into words: snake_case, kebab-case, paths and camelCase.
pub fn split_identifiers(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 8);
    let mut previous_lower = false;
    for c in text.chars() {
        if c == '_' || c == '-' || c == '/' || c == '.' || c == ':' {
            out.push(' ');
            previous_lower = false;
            continue;
        }
        if c.is_uppercase() && previous_lower {
            out.push(' ');
        }
        previous_lower = c.is_lowercase() || c.is_ascii_digit();
        out.push(c);
    }
    out
}

fn is_comment(line: &str) -> bool {
    let line = line.trim_start();
    ["///", "//", "#", "*", "/**", "\"\"\"", "'''"].iter().any(|p| line.starts_with(p))
}

/// True if the first sentence of a stored text ends with a question mark.
fn is_question(text: &str) -> bool {
    text.split(['.', '!', '\n']).next().is_some_and(|first| first.contains('?'))
}

/// The fields of a search document that the vector text uses.
#[derive(Debug, Clone, Copy)]
pub struct Entity<'a> {
    /// Entity kind, for example `symbol`.
    pub kind: &'a str,
    /// Name or title.
    pub name: &'a str,
    /// File path, or empty.
    pub path: &'a str,
    /// Full text.
    pub body: &'a str,
}

/// Returns the text to embed for a search document, or `None` for kinds that are not embedded
/// (files and raw events: they are long and the lexical index finds them).
pub fn entity_text(entity: &Entity) -> Option<String> {
    let Entity { kind, name, path, body } = *entity;
    let text = match kind {
        "symbol" => {
            let signature = body.lines().next().unwrap_or_default();
            let comments: String = body.lines().skip(1).filter(|l| is_comment(l)).collect::<Vec<_>>().join(" ");
            let comments: String = comments.chars().take(MAX_COMMENT_CHARS).collect();
            // The context line of the indexer (containers and the file comment, ADR-017).
            let context = body.lines().find_map(|l| l.strip_prefix("context: ")).unwrap_or_default();
            format!("{} {} {} {} {}", split_identifiers(name), split_identifiers(signature), comments, split_identifiers(path), context)
        }
        // An observation repeats chat, and a stored question would match later questions, not facts.
        // A stated requirement ("we use PostgreSQL 16; we rejected MySQL") is a fact: it is embedded.
        "memory" if body.starts_with("observation") || body.starts_with("user_requirement") && is_question(body) => return None,
        "memory" | "task" | "commit" => format!("{name} {body}"),
        _ => return None,
    };
    Some(text.chars().take(MAX_TEXT_CHARS).collect())
}
