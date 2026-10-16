//! Error type of `zcie-parse`.

/// Errors of the parser.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The language has no grammar.
    #[error("unsupported language: {0}")]
    UnsupportedLanguage(String),
    /// The grammar could not be loaded.
    #[error("language error: {0}")]
    Language(String),
    /// Tree-sitter returned no tree.
    #[error("parse failed")]
    ParseFailed,
}

/// Result type of `zcie-parse`.
pub type Result<T> = std::result::Result<T, Error>;
