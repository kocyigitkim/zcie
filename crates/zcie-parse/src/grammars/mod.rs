//! Grammar selection by language name and file path.

use tree_sitter::Language;

/// Returns the grammar for a language name and a path.
///
/// `typescript` uses the TSX grammar for `.tsx` files.
pub fn language_for(language: &str, path: &str) -> Option<Language> {
    match language {
        "rust" => Some(tree_sitter_rust::LANGUAGE.into()),
        "python" => Some(tree_sitter_python::LANGUAGE.into()),
        "typescript" if path.ends_with(".tsx") => Some(tree_sitter_typescript::LANGUAGE_TSX.into()),
        "typescript" => Some(tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()),
        _ => None,
    }
}
