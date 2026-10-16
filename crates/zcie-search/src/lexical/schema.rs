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
        seq: builder.add_u64_field("seq", INDEXED | STORED | FAST),
    };
    (builder.build(), fields)
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
