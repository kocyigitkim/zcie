//! Tantivy schema (`docs/07-ingest-and-indexing.md` section 9.2).

use tantivy::schema::{FAST, Field, INDEXED, STORED, STRING, Schema, TEXT};

/// The three search indexes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IndexName {
    /// File and symbol records of parsed languages.
    Code,
    /// Markdown and text files.
    Documents,
    /// Messages, test results, commands and memory records.
    Conversations,
}

impl IndexName {
    /// All indexes.
    pub const ALL: [IndexName; 3] = [IndexName::Code, IndexName::Documents, IndexName::Conversations];

    /// Folder name of the index.
    pub fn folder(&self) -> &'static str {
        match self {
            IndexName::Code => "code",
            IndexName::Documents => "documents",
            IndexName::Conversations => "conversations",
        }
    }

    /// Position of the index in arrays.
    pub fn position(&self) -> usize {
        match self {
            IndexName::Code => 0,
            IndexName::Documents => 1,
            IndexName::Conversations => 2,
        }
    }
}

/// Field handles of the schema.
#[derive(Debug, Clone, Copy)]
pub struct Fields {
    /// Entity ref (not tokenized, stored).
    pub entity_ref: Field,
    /// Entity kind.
    pub kind: Field,
    /// Tenant ID.
    pub tenant_id: Field,
    /// Workspace ID.
    pub workspace_id: Field,
    /// Repository ID.
    pub repository_id: Field,
    /// Worktree ID.
    pub worktree_id: Field,
    /// File path.
    pub path: Field,
    /// Name or title.
    pub name: Field,
    /// Full text (not stored).
    pub body: Field,
    /// Stems of the name and the body (`stem_text`), so that inflected words match (ADR-013).
    pub stems: Field,
    /// Sequence of the source state.
    pub seq: Field,
}

/// Builds the schema and returns its fields.
pub fn build_schema() -> (Schema, Fields) {
    let mut builder = Schema::builder();
    let fields = Fields {
        entity_ref: builder.add_text_field("ref", STRING | STORED),
        kind: builder.add_text_field("kind", STRING | STORED),
        tenant_id: builder.add_text_field("tenant_id", STRING),
        workspace_id: builder.add_text_field("workspace_id", STRING),
        repository_id: builder.add_text_field("repository_id", STRING),
        worktree_id: builder.add_text_field("worktree_id", STRING),
        path: builder.add_text_field("path", TEXT | STORED),
        name: builder.add_text_field("name", TEXT | STORED),
        body: builder.add_text_field("body", TEXT),
        stems: builder.add_text_field("stems", TEXT),
        seq: builder.add_u64_field("seq", INDEXED | STORED | FAST),
    };
    (builder.build(), fields)
}

/// Characters of a stem. Truncation to 5 characters is a strong stemmer for Turkish (an
/// agglutinative language: "motorun", "motorlar" → "motor") and works for English too.
pub const STEM_CHARS: usize = 5;

/// Returns the accent-free lowercase form of a letter (Turkish and common Latin letters).
pub fn fold_char(c: char) -> char {
    match c {
        'ç' | 'Ç' => 'c',
        'ğ' | 'Ğ' => 'g',
        'ı' | 'I' | 'İ' | 'î' | 'í' | 'ì' | 'ï' => 'i',
        'ö' | 'Ö' | 'ô' | 'ó' | 'ò' => 'o',
        'ş' | 'Ş' => 's',
        'ü' | 'Ü' | 'û' | 'ú' | 'ù' => 'u',
        'â' | 'á' | 'à' | 'ä' => 'a',
        'é' | 'è' | 'ê' | 'ë' => 'e',
        other => other.to_lowercase().next().unwrap_or(other),
    }
}

/// Removes Turkish suffixes that follow an apostrophe ("API'sine" → "API", "Elif'in" → "Elif").
pub fn drop_apostrophe_suffixes(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut skipping = false;
    for c in text.chars() {
        if c == '\'' || c == '’' {
            skipping = true;
            continue;
        }
        if skipping && c.is_alphabetic() {
            continue;
        }
        skipping = false;
        out.push(c);
    }
    out
}

/// Returns the stems of the words of a text, separated by spaces: apostrophe suffixes removed,
/// identifiers split, letters folded, each word cut to `STEM_CHARS` characters.
pub fn stem_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len() / 2);
    for word in expand_identifiers(&drop_apostrophe_suffixes(text)).split(|c: char| !c.is_alphanumeric()) {
        if word.is_empty() {
            continue;
        }
        let stem: String = word.chars().map(fold_char).take(STEM_CHARS).collect();
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(&stem);
    }
    out
}

/// Adds split forms of identifiers: `attachToBone` and `attach_to_bone` also give `attach to bone`.
pub fn expand_identifiers(text: &str) -> String {
    let mut extra = String::new();
    for word in text.split(|c: char| !c.is_alphanumeric() && c != '_') {
        let split = split_identifier(word);
        if split.contains(' ') {
            extra.push(' ');
            extra.push_str(&split);
        }
    }
    if extra.is_empty() { text.to_string() } else { format!("{text}\n{extra}") }
}

fn split_identifier(word: &str) -> String {
    let mut out = String::new();
    let mut previous_lower = false;
    for c in word.chars() {
        if c == '_' {
            out.push(' ');
            previous_lower = false;
            continue;
        }
        if c.is_uppercase() && previous_lower {
            out.push(' ');
        }
        previous_lower = c.is_lowercase() || c.is_ascii_digit();
        out.extend(c.to_lowercase());
    }
    out.trim().to_string()
}
