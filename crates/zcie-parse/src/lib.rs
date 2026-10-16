//! Tree-sitter parsing of Rust, TypeScript and Python files.
//!
//! See `docs/07-ingest-and-indexing.md` section 8.

pub mod error;
pub mod extractors;
pub mod grammars;
pub mod imports;
pub mod pool;

pub use error::{Error, Result};
pub use imports::resolve_relative_import;
pub use pool::{ParsedFile, ParsedSymbol, ParserPool};
