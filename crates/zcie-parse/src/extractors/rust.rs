//! Rust symbol extractor (`docs/07-ingest-and-indexing.md` section 8.1).

use tree_sitter::Node;
use zcie_types::SymbolKind;

use super::{field_text, make_symbol, named_children, node_text, qualify};
use crate::pool::ParsedFile;

const SEPARATOR: &str = "::";

/// Extracts Rust symbols and `use` imports from a syntax tree.
pub fn extract(root: Node, src: &[u8], out: &mut ParsedFile) {
    walk(root, src, &[], false, out);
}

fn walk(node: Node, src: &[u8], scope: &[String], in_impl: bool, out: &mut ParsedFile) {
    for child in named_children(node) {
        visit(child, src, scope, in_impl, out);
    }
}

fn visit(node: Node, src: &[u8], scope: &[String], in_impl: bool, out: &mut ParsedFile) {
    match node.kind() {
        "function_item" | "function_signature_item" => {
            let kind = if in_impl { SymbolKind::Method } else { SymbolKind::Function };
            push(node, src, scope, kind, out);
        }
        "struct_item" => push(node, src, scope, SymbolKind::Struct, out),
        "enum_item" => push(node, src, scope, SymbolKind::Enum, out),
        "type_item" => push(node, src, scope, SymbolKind::TypeAlias, out),
        "const_item" | "static_item" => push(node, src, scope, SymbolKind::Constant, out),
        "trait_item" => visit_container(node, src, scope, SymbolKind::Trait, true, out),
        "mod_item" => visit_container(node, src, scope, SymbolKind::Module, false, out),
        "impl_item" => visit_impl(node, src, scope, out),
        "use_declaration" => {
            if let Some(text) = field_text(node, "argument", src) {
                out.imports.push(text.to_string());
            }
        }
        _ => walk(node, src, scope, in_impl, out),
    }
}

fn push(node: Node, src: &[u8], scope: &[String], kind: SymbolKind, out: &mut ParsedFile) {
    if let Some(name) = field_text(node, "name", src) {
        let qualified = qualify(scope, name, SEPARATOR);
        out.symbols.push(make_symbol(node, src, name, qualified, kind));
    }
}

fn visit_container(node: Node, src: &[u8], scope: &[String], kind: SymbolKind, in_impl: bool, out: &mut ParsedFile) {
    let Some(name) = field_text(node, "name", src) else {
        return;
    };
    let qualified = qualify(scope, name, SEPARATOR);
    out.symbols.push(make_symbol(node, src, name, qualified, kind));
    if let Some(body) = node.child_by_field_name("body") {
        let mut inner = scope.to_vec();
        inner.push(name.to_string());
        walk(body, src, &inner, in_impl, out);
    }
}

fn visit_impl(node: Node, src: &[u8], scope: &[String], out: &mut ParsedFile) {
    let Some(type_node) = node.child_by_field_name("type") else {
        return;
    };
    let type_name = simple_type_name(node_text(type_node, src));
    if let Some(body) = node.child_by_field_name("body") {
        let mut inner = scope.to_vec();
        if !type_name.is_empty() {
            inner.push(type_name);
        }
        walk(body, src, &inner, true, out);
    }
}

/// Returns the last path segment of a type without generics: `a::Foo<T>` gives `Foo`.
fn simple_type_name(text: &str) -> String {
    let without_generics = text.split('<').next().unwrap_or(text);
    let trimmed = without_generics.trim_start_matches(['&', '*']).trim();
    let trimmed = trimmed.strip_prefix("mut ").unwrap_or(trimmed);
    trimmed.rsplit("::").next().unwrap_or(trimmed).trim().to_string()
}
