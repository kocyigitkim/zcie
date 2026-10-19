//! Python symbol extractor (`docs/07-ingest-and-indexing.md` section 8.1).

use tree_sitter::Node;
use zcie_types::SymbolKind;

use super::{field_text, make_symbol, named_children, node_text, qualify};
use crate::pool::ParsedFile;

const SEPARATOR: &str = ".";

/// Extracts Python symbols and imports from a syntax tree.
pub fn extract(root: Node, src: &[u8], out: &mut ParsedFile) {
    walk(root, src, &[], false, out);
}

fn walk(node: Node, src: &[u8], scope: &[String], in_class: bool, out: &mut ParsedFile) {
    for child in named_children(node) {
        visit(child, src, scope, in_class, out);
    }
}

fn visit(node: Node, src: &[u8], scope: &[String], in_class: bool, out: &mut ParsedFile) {
    match node.kind() {
        "function_definition" => {
            let kind = if in_class { SymbolKind::Method } else { SymbolKind::Function };
            if let Some(name) = field_text(node, "name", src) {
                let qualified = qualify(scope, name, SEPARATOR);
                out.symbols.push(make_symbol(node, src, name, qualified, kind));
            }
        }
        "class_definition" => visit_class(node, src, scope, out),
        "decorated_definition" => {
            if let Some(definition) = node.child_by_field_name("definition") {
                visit(definition, src, scope, in_class, out);
            }
        }
        "import_statement" => push_import_names(node, src, out),
        "import_from_statement" => {
            if let Some(module) = field_text(node, "module_name", src) {
                out.imports.push(module.to_string());
            }
        }
        "expression_statement" if !in_class && scope.is_empty() => push_constant(node, src, out),
        "lambda" => {}
        _ => walk(node, src, scope, in_class, out),
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
        walk(body, src, &inner, true, out);
    }
}

/// Adds a module-level constant: an assignment to an UPPER_CASE name (PEP 8), for example
/// `MAX_RETRIES = 7` or `TIMEOUT: float = 2.5`.
fn push_constant(node: Node, src: &[u8], out: &mut ParsedFile) {
    let Some(assignment) = named_children(node).into_iter().find(|c| c.kind() == "assignment") else { return };
    let Some(left) = assignment.child_by_field_name("left").filter(|l| l.kind() == "identifier") else { return };
    let name = node_text(left, src);
    let upper = name.chars().any(|c| c.is_ascii_uppercase()) && name.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_');
    if upper {
        out.symbols.push(make_symbol(node, src, name, name.to_string(), SymbolKind::Constant));
    }
}

/// Adds each module of `import a.b, c as d`.
fn push_import_names(node: Node, src: &[u8], out: &mut ParsedFile) {
    for child in named_children(node) {
        let module = match child.kind() {
            "dotted_name" => Some(node_text(child, src)),
            "aliased_import" => field_text(child, "name", src),
            _ => None,
        };
        if let Some(module) = module.filter(|m| !m.is_empty()) {
            out.imports.push(module.to_string());
        }
    }
}
