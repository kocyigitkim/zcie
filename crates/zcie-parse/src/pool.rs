//! Parser entry point and parse results.

use tree_sitter::Parser;
use zcie_types::{ParserQuality, SymbolKind};

use crate::error::{Error, Result};
use crate::extractors;
use crate::grammars::language_for;

/// One symbol found by the parser.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedSymbol {
    /// Simple name.
    pub name: String,
    /// Name with enclosing type and module names.
    pub qualified_name: String,
    /// Declaration kind.
    pub kind: SymbolKind,
    /// First line of the declaration, 200 characters or fewer.
    pub signature: String,
    /// `[start_line, end_line]`, 1-based.
    pub span: [u32; 2],
}

/// The result of one file parse.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedFile {
    /// Symbols in source order.
    pub symbols: Vec<ParsedSymbol>,
    /// Raw import module or path strings.
    pub imports: Vec<String>,
    /// Names that the file uses and does not define, with their counts, the most used first.
    pub references: Vec<(String, u32)>,
    /// Call sites in source order (ADR-019, D-071).
    pub calls: Vec<zcie_types::CallSite>,
    /// `PartialSyntaxError` if the tree has an error node.
    pub quality: ParserQuality,
}

/// Parses files. It makes a new Tree-sitter parser for each call, so it has no shared state.
#[derive(Debug, Default, Clone, Copy)]
pub struct ParserPool;

impl ParserPool {
    /// Makes a parser pool.
    pub fn new() -> ParserPool {
        ParserPool
    }

    /// Parses `source` of `language` (`rust`, `typescript`, `python`) and extracts symbols and imports.
    pub fn parse_file(&self, language: &str, path: &str, source: &str) -> Result<ParsedFile> {
        let grammar = language_for(language, path)
            .ok_or_else(|| Error::UnsupportedLanguage(language.to_string()))?;
        let mut parser = Parser::new();
        parser
            .set_language(&grammar)
            .map_err(|e| Error::Language(e.to_string()))?;
        let tree = parser.parse(source, None).ok_or(Error::ParseFailed)?;
        let root = tree.root_node();
        let mut parsed = ParsedFile {
            symbols: Vec::new(),
            imports: Vec::new(),
            references: Vec::new(),
            calls: Vec::new(),
            quality: if root.has_error() {
                ParserQuality::PartialSyntaxError
            } else {
                ParserQuality::Complete
            },
        };
        let src = source.as_bytes();
        match language {
            "rust" => extractors::rust::extract(root, src, &mut parsed),
            "typescript" => extractors::typescript::extract(root, src, &mut parsed),
            _ => extractors::python::extract(root, src, &mut parsed),
        }
        let defined: std::collections::HashSet<&str> = parsed.symbols.iter().map(|s| s.name.as_str()).collect();
        parsed.references = crate::references::used_names(root, src, &defined);
        parsed.calls = crate::calls::call_sites(root, src);
        Ok(parsed)
    }
}
