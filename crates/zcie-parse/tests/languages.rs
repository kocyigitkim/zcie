//! Symbols of the languages of ADR-030 (TS-30-01, TS-30-02).
#![allow(clippy::unwrap_used)]

use zcie_parse::ParserPool;
use zcie_types::SymbolKind;

fn symbols(language: &str, path: &str, source: &str) -> Vec<(String, SymbolKind, [u32; 2])> {
    let parsed = ParserPool::new().parse_file(language, path, source).unwrap();
    parsed.symbols.into_iter().map(|s| (s.qualified_name, s.kind, s.span)).collect()
}

fn s(name: &str, kind: SymbolKind, span: [u32; 2]) -> (String, SymbolKind, [u32; 2]) {
    (name.to_string(), kind, span)
}

#[test]
fn line_extractor_languages() {
    use SymbolKind::*;
    let go = "package main\n\n// Server serves.\ntype Server struct {\n\tport int\n}\n\nfunc (s *Server) Start(ctx context.Context) error {\n\treturn listen(s.port)\n}\n\nfunc main() {\n\tnew(Server).Start(nil)\n}\n";
    assert_eq!(symbols("go", "main.go", go), vec![s("Server", TypeAlias, [4, 6]), s("Start", Function, [8, 10]), s("main", Function, [12, 14])]);

    let java = "public class Billing {\n    private static final int RATE = 7;\n\n    @Override\n    public int apply(int amount) {\n        if (amount > 0) {\n            return amount * RATE;\n        }\n        return 0;\n    }\n}\n";
    assert_eq!(symbols("java", "Billing.java", java), vec![s("Billing", Class, [1, 11]), s("Billing.apply", Method, [5, 10])]);

    let cpp = "#include \"fees.h\"\n\nint Fees::late_fee(int days)\n{\n    return days * kRate;\n}\n\nstatic void log_it(const char* m) {\n    printf(\"%s\", m);\n}\n";
    assert_eq!(symbols("cpp", "fees.cpp", cpp), vec![s("late_fee", Function, [3, 6]), s("log_it", Function, [8, 10])]);
    let parsed = ParserPool::new().parse_file("cpp", "src/fees.cpp", cpp).unwrap();
    assert_eq!(parsed.imports, vec!["./fees.h".to_string()]);
    assert!(parsed.calls.iter().any(|c| c.name == "printf" && c.line == 9));

    let csharp = "namespace Shop {\n    public sealed class Cart {\n        public decimal Total(IEnumerable<Item> items) {\n            return items.Sum(i => i.Price);\n        }\n    }\n}\n";
    assert_eq!(symbols("csharp", "Cart.cs", csharp), vec![s("Shop", Module, [1, 7]), s("Shop.Cart", Class, [2, 6]), s("Shop.Cart.Total", Method, [3, 5])]);

    let kotlin = "data class User(val id: Int)\n\nsuspend fun loadUser(id: Int): User {\n    return api.get(id)\n}\n";
    assert_eq!(symbols("kotlin", "User.kt", kotlin), vec![s("User", Class, [1, 1]), s("loadUser", Function, [3, 5])]);

    let ruby = "module Billing\n  class Invoice\n    def self.build(x)\n      new(x)\n    end\n  end\nend\n";
    assert_eq!(symbols("ruby", "invoice.rb", ruby), vec![s("Billing", Module, [1, 7]), s("Billing.Invoice", Class, [2, 6]), s("Billing.Invoice.build", Method, [3, 5])]);

    let lua = "local function helper(x)\n  return x\nend\n\nfunction M.run(cfg)\n  helper(cfg)\nend\n";
    assert_eq!(symbols("lua", "m.lua", lua), vec![s("helper", Function, [1, 3]), s("run", Function, [5, 7])]);

    let shell = "#!/bin/sh\n# deploy() { is a comment\ndeploy() {\n    build_all \"$1\"\n}\nfunction clean {\n    rm -rf out\n}\n";
    assert_eq!(symbols("shell", "run.sh", shell), vec![s("deploy", Function, [3, 5]), s("clean", Function, [6, 8])]);

    let sql = "-- CREATE TABLE fake (x int);\nCREATE TABLE IF NOT EXISTS public.balances (\n  id bigint\n);\nCREATE OR REPLACE FUNCTION apply_fee(x int) RETURNS int AS $$ SELECT x $$;\n";
    assert_eq!(symbols("sql", "schema.sql", sql), vec![s("balances", Struct, [2, 4]), s("apply_fee", Function, [5, 5])]);
}

#[test]
fn javascript_uses_the_tsx_grammar() {
    let js = "import { a } from './a';\nexport function route(x) {\n  return <div>{a(x)}</div>;\n}\nclass Cache {\n  get(k) { return k; }\n}\n";
    let found = symbols("javascript", "src/route.jsx", js);
    assert!(found.iter().any(|(n, k, _)| n == "route" && *k == SymbolKind::Function));
    assert!(found.iter().any(|(n, k, _)| n == "Cache" && *k == SymbolKind::Class));
}
