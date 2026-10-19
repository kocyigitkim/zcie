//! Lexical search over code, documents and conversations (Tantivy). The index is derived
//! from the canonical store and can be rebuilt (`docs/adr/ADR-005-derived-search-index.md`).

pub mod error;
pub mod freshness;
pub mod lexical;
pub mod ranking;

pub use error::{Error, Result};
pub use lexical::document::{PASSAGE_CHARS, SearchDoc, SearchOp, passages};
pub use lexical::reader::{SearchHit, SearchQuery};
pub use lexical::schema::{IndexName, STEM_CHARS, drop_apostrophe_suffixes, expand_identifiers, fold_char, stem_text};
pub use lexical::writer::{SearchIndex, SearchSender, SearchWriter, flush};
