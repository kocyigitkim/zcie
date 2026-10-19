//! Tree-sitter parsing of Rust, TypeScript and Python files.
//!
//! See `docs/07-ingest-and-indexing.md` section 8.

pub mod calls;
pub mod error;
pub mod extractors;
pub mod grammars;
pub mod imports;
pub mod pool;
pub mod references;

pub use error::{Error, Result};

/// Version of the symbol extractors. Increase it when an extractor finds new symbols:
/// files indexed with an older version are parsed again, also if their content is the same.
/// Version 2: TypeScript top-level `const` and Python module-level UPPER_CASE constants.
/// Version 3: the names that a file uses (`ParsedFile::references`, ADR-017).
/// Version 4: the call sites of a file (`ParsedFile::calls`, ADR-019).
pub const PARSER_VERSION: u32 = 4;
pub use imports::resolve_relative_import;
pub use pool::{ParsedFile, ParsedSymbol, ParserPool};
