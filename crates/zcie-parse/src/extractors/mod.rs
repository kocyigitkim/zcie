//! Per-language symbol extractors and shared helpers.

pub mod python;
pub mod rust;
pub mod typescript;

use tree_sitter::Node;
use zcie_types::SymbolKind;

use crate::pool::ParsedSymbol;

/// Maximum signature length in characters.
pub const MAX_SIGNATURE_CHARS: usize = 200;

/// Returns the source text of a node, or an empty string.
pub fn node_text<'a>(node: Node, src: &'a [u8]) -> &'a str {
    node.utf8_text(src).unwrap_or("")
}

/// Returns the text of the `name` field of a node.
pub fn field_text<'a>(node: Node, field: &str, src: &'a [u8]) -> Option<&'a str> {
    node.child_by_field_name(field)
        .map(|child| node_text(child, src))
        .filter(|text| !text.is_empty())
}

/// Joins scope names and a name with a separator.
pub fn qualify(scope: &[String], name: &str, separator: &str) -> String {
    if scope.is_empty() {
        return name.to_string();
    }
    format!("{}{separator}{name}", scope.join(separator))
}

/// Makes a symbol from a declaration node.
pub fn make_symbol(node: Node, src: &[u8], name: &str, qualified_name: String, kind: SymbolKind) -> ParsedSymbol {
    let first_line = node_text(node, src).lines().next().unwrap_or("").trim();
    ParsedSymbol {
        name: name.to_string(),
        qualified_name,
        kind,
        signature: first_line.chars().take(MAX_SIGNATURE_CHARS).collect(),
        span: [
            node.start_position().row as u32 + 1,
            node.end_position().row as u32 + 1,
        ],
    }
}

/// Returns the named children of a node.
pub fn named_children(node: Node) -> Vec<Node> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor).collect()
}

/// Removes surrounding quotes from a string literal.
pub fn unquote(text: &str) -> String {
    text.trim_matches(|c| c == '"' || c == '\'' || c == '`').to_string()
}
