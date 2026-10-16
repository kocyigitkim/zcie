//! Code entities: files, symbols and commits (`docs/05-data-model.md` section 5).

use serde::{Deserialize, Serialize};

use crate::ids::{CommitId, FileId, SymbolId};
use crate::scope::Scope;

/// One file of a worktree.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FileRecord {
    /// Value schema version.
    pub v: u32,
    /// The file ID. Stable for one worktree path.
    pub file_id: FileId,
    /// Scope with repository and worktree.
    pub scope: Scope,
    /// Path relative to the worktree root, with `/` separators.
    pub path: String,
    /// `rust`, `typescript`, `python`, or `None`.
    pub language: Option<String>,
    /// Content hash `b3:...`.
    pub content_hash: String,
    /// Size in bytes.
    pub size: u64,
    /// True if the file has a 0x00 byte in the first 8 KiB.
    pub is_binary: bool,
    /// Last commit SHA that changed the file, if known.
    #[serde(default)]
    pub git_revision: Option<String>,
    /// Sequence that produced this state.
    pub indexed_seq: u64,
    /// True if the file was deleted.
    pub deleted: bool,
    /// True if the content is in the object store.
    #[serde(default)]
    pub content_stored: bool,
}

/// One named code declaration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SymbolRecord {
    /// Value schema version.
    pub v: u32,
    /// Stable symbol ID.
    pub symbol_id: SymbolId,
    /// Scope with repository and worktree.
    pub scope: Scope,
    /// The file that contains the symbol.
    pub file_id: FileId,
    /// Path relative to the worktree root.
    pub path: String,
    /// Language name.
    pub language: String,
    /// Simple name, for example `attach_to_bone`.
    pub name: String,
    /// Qualified name, for example `WeaponSocket::attach_to_bone`.
    pub qualified_name: String,
    /// Declaration kind.
    pub kind: SymbolKind,
    /// First line of the declaration, 200 characters or fewer.
    pub signature: String,
    /// `[start_line, end_line]`, 1-based.
    pub span: [u32; 2],
    /// Content hash of the file at parse time.
    pub file_hash: String,
    /// Parser quality flag.
    pub parser_quality: ParserQuality,
}

/// The kind of a symbol.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SymbolKind {
    /// Free function.
    Function,
    /// Method of a type.
    Method,
    /// Rust struct.
    Struct,
    /// Class.
    Class,
    /// Enum.
    Enum,
    /// Rust trait.
    Trait,
    /// Interface.
    Interface,
    /// Type alias.
    TypeAlias,
    /// Constant or static.
    Constant,
    /// Module.
    Module,
}

impl SymbolKind {
    /// Returns the serialized name.
    pub fn name(&self) -> &'static str {
        match self {
            SymbolKind::Function => "function",
            SymbolKind::Method => "method",
            SymbolKind::Struct => "struct",
            SymbolKind::Class => "class",
            SymbolKind::Enum => "enum",
            SymbolKind::Trait => "trait",
            SymbolKind::Interface => "interface",
            SymbolKind::TypeAlias => "type_alias",
            SymbolKind::Constant => "constant",
            SymbolKind::Module => "module",
        }
    }
}

/// Parser quality of a symbol.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParserQuality {
    /// The file parsed with no error node.
    Complete,
    /// The file has a syntax error. Some symbols can be missing.
    PartialSyntaxError,
}

/// One Git commit.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CommitRecord {
    /// Value schema version.
    pub v: u32,
    /// `commit_<sha>`.
    pub commit_id: CommitId,
    /// Scope with repository.
    pub scope: Scope,
    /// Full SHA.
    pub sha: String,
    /// Author name.
    pub author: String,
    /// RFC 3339 author time.
    pub author_utc: String,
    /// Full commit message.
    pub message: String,
    /// Changed paths with status letter, for example `M src/a.rs`.
    pub changed_paths: Vec<String>,
    /// Parsed trailers.
    pub trailers: Vec<(String, String)>,
    /// `ZCIE-Task` values that did not resolve to a task.
    #[serde(default)]
    pub unresolved_trailers: Vec<String>,
}
