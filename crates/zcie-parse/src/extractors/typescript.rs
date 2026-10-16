//! TypeScript and TSX symbol extractor (`docs/07-ingest-and-indexing.md` section 8.1).

use tree_sitter::Node;
use zcie_types::SymbolKind;

use super::{field_text, make_symbol, named_children, qualify, unquote};
use crate::pool::ParsedFile;

const SEPARATOR: &str = ".";

/// Extracts TypeScript symbols and import sources from a syntax tree.
pub fn extract(root: Node, src: &[u8], out: &mut ParsedFile) {
    for child in named_children(root) {
        visit(child, src, &[], true, out);
    }
}

fn visit(node: Node, src: &[u8], scope: &[String], top_level: bool, out: &mut ParsedFile) {
    match node.kind() {
        "function_declaration" | "generator_function_declaration" | "function_signature" => {
            push(node, src, scope, SymbolKind::Function, out);
        }
        "method_definition" | "method_signature" | "abstract_method_signature" => {
            push(node, src, scope, SymbolKind::Method, out);
        }
        "class_declaration" | "abstract_class_declaration" => visit_class(node, src, scope, out),
        "interface_declaration" => push(node, src, scope, SymbolKind::Interface, out),
        "enum_declaration" => push(node, src, scope, SymbolKind::Enum, out),
        "type_alias_declaration" => push(node, src, scope, SymbolKind::TypeAlias, out),
        "lexical_declaration" if top_level => visit_lexical(node, src, scope, out),
        "export_statement" => visit_children(node, src, scope, top_level, out),
        "import_statement" => {
            if let Some(source) = field_text(node, "source", src) {
                out.imports.push(unquote(source));
            }
        }
        "statement_block" | "arrow_function" | "function_expression" => {}
        _ => visit_children(node, src, scope, false, out),
    }
}

fn visit_children(node: Node, src: &[u8], scope: &[String], top_level: bool, out: &mut ParsedFile) {
    for child in named_children(node) {
        visit(child, src, scope, top_level, out);
    }
}

fn push(node: Node, src: &[u8], scope: &[String], kind: SymbolKind, out: &mut ParsedFile) {
    if let Some(name) = field_text(node, "name", src) {
        let qualified = qualify(scope, name, SEPARATOR);
        out.symbols.push(make_symbol(node, src, name, qualified, kind));
    }
}

fn visit_class(node: Node, src: &[u8], scope: &[String], out: &mut ParsedFile) {
    let Some(name) = field_text(node, "name", src) else {
        return;
    };
    let qualified = qualify(scope, name, SEPARATOR);
    out.symbols.push(make_symbol(node, src, name, qualified, SymbolKind::Class));
    if let Some(body) = node.child_by_field_name("body") {
        let mut inner = scope.to_vec();
        inner.push(name.to_string());
        visit_children(body, src, &inner, false, out);
    }
}

/// Handles `const name = (...) => ...` at the top level.
fn visit_lexical(node: Node, src: &[u8], scope: &[String], out: &mut ParsedFile) {
    for declarator in named_children(node) {
        if declarator.kind() != "variable_declarator" {
            continue;
        }
        let is_function = declarator
            .child_by_field_name("value")
            .is_some_and(|value| matches!(value.kind(), "arrow_function" | "function_expression"));
        if !is_function {
            continue;
        }
        if let Some(name) = field_text(declarator, "name", src) {
            let qualified = qualify(scope, name, SEPARATOR);
            out.symbols.push(make_symbol(node, src, name, qualified, SymbolKind::Function));
        }
    }
}
