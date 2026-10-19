//! Constants are symbols (parser version 2): their signature carries the value, so a query can
//! answer "what is the TTL" without a file read.
#![allow(clippy::unwrap_used)]

use zcie_parse::ParserPool;
use zcie_types::SymbolKind;

fn kinds(language: &str, path: &str, source: &str) -> Vec<(String, SymbolKind, String)> {
    let parsed = ParserPool::new().parse_file(language, path, source).unwrap();
    parsed.symbols.into_iter().map(|s| (s.name, s.kind, s.signature)).collect()
}

fn has(found: &[(String, SymbolKind, String)], name: &str, kind: SymbolKind) -> bool {
    found.iter().any(|(n, k, _)| n == name && *k == kind)
}

#[test]
fn typescript_top_level_const_is_a_constant() {
    let source = "export const CACHE_TTL_SECONDS = 420;\nlet counter = 1;\nexport const add = (a: number) => a;\nconst { a, b } = pair;\nfunction f() {\n  const inner = 2;\n}\n";
    let found = kinds("typescript", "src/policy.ts", source);
    assert!(has(&found, "CACHE_TTL_SECONDS", SymbolKind::Constant), "{found:?}");
    assert!(has(&found, "add", SymbolKind::Function));
    assert!(found.iter().any(|(n, _, s)| n == "CACHE_TTL_SECONDS" && s.contains("420")));
    for absent in ["counter", "inner", "a", "{ a, b }"] {
        assert!(!found.iter().any(|(n, _, _)| n == absent), "{absent} is not a symbol: {found:?}");
    }
}

#[test]
fn python_module_level_upper_case_assignment_is_a_constant() {
    let source = "MAX_BURN_RETRIES = 7\nTIMEOUT: float = 2.5\nlogger = get_logger()\nclass Job:\n    LIMIT = 3\n\ndef run():\n    INNER = 1\n";
    let found = kinds("python", "orbit/scheduler.py", source);
    assert!(has(&found, "MAX_BURN_RETRIES", SymbolKind::Constant), "{found:?}");
    assert!(has(&found, "TIMEOUT", SymbolKind::Constant));
    assert!(found.iter().any(|(n, _, s)| n == "MAX_BURN_RETRIES" && s == "MAX_BURN_RETRIES = 7"));
    for absent in ["logger", "LIMIT", "INNER"] {
        assert!(!found.iter().any(|(n, _, _)| n == absent), "{absent} is not a module constant: {found:?}");
    }
}
