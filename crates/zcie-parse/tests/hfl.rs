//! HFL line extractor (ADR-028, D-090, TS-28-01).
#![allow(clippy::unwrap_used)]

use zcie_parse::ParserPool;
use zcie_types::SymbolKind;

const SOURCE: &str = r#"# Test: access
import "../src/access.hfl"

const CB_MODE: int = 1
global var g_error: string = ""

struct TextAccess:
    scale_pct: int

class Walker:
    public func step(dt: float) -> int:
        return move(dt)

func text_px(base_px: int, acc: TextAccess) -> int:
    # call("not a call")
    var s: string = "byte_of(1)"
    return design_to_device(float(base_px), acc.scale_pct)

native func clock_ns() -> int
"#;

#[test]
fn hfl_declarations_imports_and_calls() {
    let parsed = ParserPool::new().parse_file("hfl", "modules/a/tests/t.hfl", SOURCE).unwrap();
    let symbols: Vec<(&str, SymbolKind, String, [u32; 2])> =
        parsed.symbols.iter().map(|s| (s.name.as_str(), s.kind, s.qualified_name.clone(), s.span)).collect();
    assert_eq!(symbols, vec![
        ("CB_MODE", SymbolKind::Constant, "CB_MODE".into(), [4, 4]),
        ("g_error", SymbolKind::Constant, "g_error".into(), [5, 5]),
        ("TextAccess", SymbolKind::Struct, "TextAccess".into(), [7, 8]),
        ("Walker", SymbolKind::Class, "Walker".into(), [10, 12]),
        ("step", SymbolKind::Method, "Walker.step".into(), [11, 12]),
        ("text_px", SymbolKind::Function, "text_px".into(), [14, 17]),
        ("clock_ns", SymbolKind::Function, "clock_ns".into(), [19, 19]),
    ]);
    assert_eq!(parsed.imports, vec!["../src/access.hfl".to_string()]);
    let calls: Vec<(&str, Option<&str>, u32)> = parsed.calls.iter().map(|c| (c.name.as_str(), c.qualifier.as_deref(), c.line)).collect();
    // Strings and comments hold no calls; a declaration is not a call of itself.
    assert_eq!(calls, vec![("move", None, 12), ("design_to_device", None, 17)]);
    assert!(parsed.references.iter().any(|(n, _)| n == "design_to_device"));
    assert!(!parsed.references.iter().any(|(n, _)| n == "text_px" || n == "func"));
}
