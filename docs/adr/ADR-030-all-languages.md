# ADR-030 — Symbols for all common programming languages

- Status: Accepted
- Date: 2026-10-23
- Tasks: D-102 to D-104 of `docs/12-development-plan.md`
- Changes: ADR-021 section D-080 (no new language before a measured need). The user asked for all languages (2026-10-23).

## Context
- Every text file that is not binary and not excluded is already in the lexical index (the `Documents` index, as passages for long files). Text formats (Markdown, JSON, YAML, TOML, XML, SQL, CSV, …) need no change to be found.
- Only Rust, TypeScript, Python (Tree-sitter) and HFL (lines, ADR-028) get symbols, line ranges and call sites. The projects of the user also have JavaScript (4,406 files in hellfurylanguage), C++, C#, SQL and shell files.

## Decision
1. **JavaScript (D-102).** `.js`, `.jsx`, `.mjs` and `.cjs` get the language `javascript`. It uses the TSX grammar of `tree-sitter-typescript` and the TypeScript extractor (TSX parses JavaScript and JSX).
2. **Line extractor for other languages (D-103).** One extractor in `extractors/lines.rs` serves HFL and these languages: Go, Java, Kotlin, Scala, Swift, C, C++, C#, PHP, Ruby, Lua, Dart, shell, PowerShell, SQL, R, Julia, Elixir, Perl, Zig, Nim, GDScript, Pascal, Groovy. Per language it knows the line comment markers. It finds:
   - keyword declarations (`fn`, `func`, `function`, `fun`, `def`, `sub`, `proc`, `class`, `struct`, `interface`, `trait`, `enum`, `type`, `module`, `namespace`, `object`, `record`, top-level `const`) after modifiers (`public`, `static`, `export`, `local`, …);
   - Go methods (`func (r *T) Name(`), shell functions (`name() {`), R functions (`name <- function(`), SQL `CREATE … TABLE|VIEW|FUNCTION|PROCEDURE|TRIGGER|TYPE name`;
   - in C, C++, C#, Java and Dart: a line `type name(args)` that ends with `{` or `)` and has no `=` before `(` and no `;` at the end;
   - `#include "x"` in C and C++ as an import.
   A declaration ends before the next code line with the same or a smaller indent; a closing line (`}`, `end`) at the same indent is the last line of the declaration. A line with only `{` does not end it.
3. **All languages by default (D-104).** The default of `index.languages` is `["*"]`: every language that ZCIE knows. A list of names still limits the languages. `PARSER_VERSION` becomes 6, so all files are parsed again once.

## Consequences
- The line extractor gives no syntax tree. It can find a false symbol (a macro call that looks like a function) or miss one (a declaration split over lines). `quality` stays `complete`.
- Tree-sitter grammars for Go, Java, C, C++ or C# can replace the line extractor for one language later, when a benchmark shows a wrong or missing symbol.
- After the update the daemon parses all indexed files again (one time).
