//! Call sites of a file (ADR-019, D-071): the called name, its qualifier and its line, for Rust,
//! TypeScript and Python. The query resolves them to definitions (same file, import, qualified
//! path); this module only reads the syntax.

use tree_sitter::Node;
use zcie_types::CallSite;

use crate::extractors::node_text;

/// Node kinds of a call.
const CALL_KINDS: [&str; 2] = ["call_expression", "call"];
/// Most call sites that one file keeps.
pub const MAX_CALLS: usize = 2_000;

/// Returns (name, qualifier, method) of the function part of a call.
fn callee(function: Node, src: &[u8]) -> Option<(String, Option<String>, bool)> {
    let field = |name: &str| function.child_by_field_name(name).map(|n| node_text(n, src).to_string());
    match function.kind() {
        "identifier" => Some((node_text(function, src).to_string(), None, false)),
        // Rust `a::b::name`.
        "scoped_identifier" => Some((field("name")?, field("path"), false)),
        // Rust `value.name`, TypeScript `value.name`.
        "field_expression" => Some((field("field")?, field("value"), true)),
        "member_expression" => Some((field("property")?, field("object"), true)),
        // Python `module.name` or `value.name`.
        "attribute" => Some((field("attribute")?, field("object"), true)),
        // Rust `name::<T>()`.
        "generic_function" => function.child_by_field_name("function").and_then(|f| callee(f, src)),
        _ => None,
    }
}

/// Call sites inside a Rust macro (`assert_eq!(apply(1), 2)`): the arguments of a macro are a token
/// tree, so a call is an identifier followed by a parenthesized token tree.
fn macro_calls(tree: Node, src: &[u8], out: &mut Vec<CallSite>) {
    let children: Vec<Node> = {
        let mut cursor = tree.walk();
        tree.children(&mut cursor).collect()
    };
    for (i, child) in children.iter().enumerate() {
        if child.kind() == "token_tree" {
            macro_calls(*child, src, out);
            let called = i.checked_sub(1).map(|p| children[p]).filter(|p| p.kind() == "identifier");
            let opens = child.child(0).is_some_and(|c| node_text(c, src) == "(");
            if let (Some(name), true) = (called, opens) {
                let qualifier = i.checked_sub(3).filter(|_| i >= 2 && node_text(children[i - 2], src) == "::").map(|q| node_text(children[q], src).to_string());
                out.push(CallSite { name: node_text(name, src).to_string(), qualifier, method: false, line: name.start_position().row as u32 + 1 });
            }
        }
    }
}

/// Returns the call sites of a syntax tree in source order.
pub fn call_sites(root: Node, src: &[u8]) -> Vec<CallSite> {
    let mut out = Vec::new();
    let mut cursor = root.walk();
    let mut visited_children = false;
    loop {
        let node = cursor.node();
        if !visited_children && CALL_KINDS.contains(&node.kind()) && out.len() < MAX_CALLS
            && let Some((name, qualifier, method)) = node.child_by_field_name("function").and_then(|f| callee(f, src))
        {
            out.push(CallSite { name, qualifier, method, line: node.start_position().row as u32 + 1 });
        }
        if !visited_children && node.kind() == "macro_invocation" && out.len() < MAX_CALLS
            && let Some(tree) = node.children(&mut node.walk()).find(|c| c.kind() == "token_tree")
        {
            macro_calls(tree, src, &mut out);
        }
        if !visited_children && cursor.goto_first_child() {
            continue;
        }
        if cursor.goto_next_sibling() {
            visited_children = false;
            continue;
        }
        if !cursor.goto_parent() {
            break;
        }
        visited_children = true;
    }
    out
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use crate::ParserPool;

    fn calls(language: &str, path: &str, source: &str) -> Vec<(String, Option<String>, bool, u32)> {
        let parsed = ParserPool::new().parse_file(language, path, source).expect("parse");
        parsed.calls.into_iter().map(|c| (c.name, c.qualifier, c.method, c.line)).collect()
    }

    #[test]
    fn calls_of_each_language_are_found() {
        let rust = "fn a() {\n    fees::apply(1);\n    x.run();\n    assert_eq!(apply_late_fee(1, 2), 3);\n}\n";
        assert_eq!(calls("rust", "a.rs", rust), vec![
            ("apply".into(), Some("fees".into()), false, 2),
            ("run".into(), Some("x".into()), true, 3),
            ("apply_late_fee".into(), None, false, 4),
        ]);
        let ts = "function a() {\n  route(x);\n  cache.get(k);\n}\n";
        assert_eq!(calls("typescript", "a.ts", ts), vec![("route".into(), None, false, 2), ("get".into(), Some("cache".into()), true, 3)]);
        let py = "def a():\n    burn(1)\n    queue.put(2)\n";
        assert_eq!(calls("python", "a.py", py), vec![("burn".into(), None, false, 2), ("put".into(), Some("queue".into()), true, 3)]);
    }
}
