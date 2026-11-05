//! Line extractor for languages with no Tree-sitter grammar in ZCIE (ADR-028 HFL, ADR-030 others).
//! It reads lines: a declaration ends before the next code line with the same or a smaller indent;
//! a closing line (`}`, `end`) at that indent is the last line of the declaration.

use std::collections::{HashMap, HashSet};

use zcie_types::{CallSite, ParserQuality, SymbolKind};

use super::{MAX_SIGNATURE_CHARS, qualify, unquote};
use crate::calls::MAX_CALLS;
use crate::pool::{ParsedFile, ParsedSymbol};
use crate::references::MAX_NAMES;

const SEPARATOR: &str = ".";
const MODIFIERS: [&str; 30] = [
    "public", "private", "protected", "internal", "native", "static", "override", "virtual", "export", "default",
    "abstract", "final", "sealed", "open", "data", "async", "inline", "extern", "unsafe", "pub", "local", "global",
    "readonly", "suspend", "partial", "noinline", "constexpr", "pure", "elemental", "recursive",
];
/// Words that are never a called or used name.
const KEYWORDS: [&str; 64] = [
    "func", "fn", "fun", "function", "def", "sub", "proc", "var", "let", "val", "if", "elif", "else", "elsif", "unless",
    "while", "for", "foreach", "do", "switch", "case", "match", "when", "return", "import", "include", "require",
    "global", "const", "native", "public", "private", "struct", "class", "enum", "true", "false", "and", "or", "not",
    "in", "break", "continue", "forge", "override", "virtual", "static", "int", "float", "string", "bool", "void",
    "new", "sizeof", "typeof", "catch", "try", "throw", "then", "end", "nil", "null", "self", "this",
];
/// First words of a C-like line that is not a function declaration.
const NOT_DECLARATION: [&str; 14] =
    ["if", "for", "while", "switch", "return", "else", "catch", "do", "case", "throw", "new", "delete", "using", "goto"];

/// Languages that this extractor serves (the names of `zcie_fs::language_for_path`).
pub const LANGUAGES: [&str; 26] = [
    "hfl", "go", "java", "kotlin", "scala", "swift", "c", "cpp", "csharp", "php", "ruby", "lua", "dart", "shell",
    "powershell", "sql", "r", "julia", "elixir", "perl", "zig", "nim", "gdscript", "pascal", "groovy", "objc",
];

/// Returns true if this extractor serves the language.
pub fn supports(language: &str) -> bool {
    LANGUAGES.contains(&language)
}

fn comment_markers(language: &str) -> &'static [&'static str] {
    match language {
        "hfl" | "shell" | "ruby" | "r" | "perl" | "powershell" | "elixir" | "julia" | "nim" | "gdscript" => &["#"],
        "sql" | "lua" => &["--"],
        "php" => &["//", "#"],
        _ => &["//"],
    }
}

fn c_like(language: &str) -> bool {
    matches!(language, "c" | "cpp" | "csharp" | "java" | "dart" | "objc")
}

/// Parses a source of a language of `LANGUAGES`.
pub fn parse(language: &str, source: &str) -> ParsedFile {
    let mut out = ParsedFile { symbols: Vec::new(), imports: Vec::new(), references: Vec::new(), calls: Vec::new(), quality: ParserQuality::Complete };
    let comments = comment_markers(language);
    // Open declarations: (indent, index in `out.symbols`).
    let mut open: Vec<(usize, usize)> = Vec::new();
    let mut last_code_line = 0u32;
    let mut used: Vec<String> = Vec::new();
    for (index, raw) in source.lines().enumerate() {
        let line_no = index as u32 + 1;
        let code = strip_strings_and_comment(raw, comments);
        let trimmed_code = code.trim();
        if trimmed_code.is_empty() {
            continue;
        }
        let indent = raw.len() - raw.trim_start().len();
        // A brace on its own line opens the block of the declaration above (Allman style).
        if trimmed_code == "{" {
            last_code_line = line_no;
            continue;
        }
        let closing = trimmed_code.starts_with(['}', ')']) || trimmed_code == "end" || trimmed_code.starts_with("end ") || trimmed_code.starts_with("end;");
        while open.last().is_some_and(|(i, _)| *i >= indent) {
            if let Some((i, symbol)) = open.pop() {
                out.symbols[symbol].span[1] = if closing && i == indent { line_no } else { last_code_line };
            }
        }
        last_code_line = line_no;
        let trimmed = raw.trim();
        if let Some(import) = import_of(language, trimmed) {
            out.imports.push(import);
            continue;
        }
        let declared = declaration(language, trimmed_code, open.is_empty());
        if let Some((kind, name)) = &declared {
            let in_type = open.last().is_some_and(|(_, s)| matches!(out.symbols[*s].kind, SymbolKind::Class | SymbolKind::Struct | SymbolKind::Interface | SymbolKind::Trait));
            let kind = if *kind == SymbolKind::Function && in_type { SymbolKind::Method } else { *kind };
            let scope: Vec<String> = open.iter().map(|(_, s)| out.symbols[*s].name.clone()).collect();
            out.symbols.push(ParsedSymbol {
                name: name.clone(),
                qualified_name: qualify(&scope, name, SEPARATOR),
                kind,
                signature: trimmed.chars().take(MAX_SIGNATURE_CHARS).collect(),
                span: [line_no, line_no],
            });
            open.push((indent, out.symbols.len() - 1));
        }
        let own = declared.map(|(_, name)| name);
        scan_names(&code, line_no, own.as_deref(), &mut out.calls, &mut used);
    }
    for (_, symbol) in open {
        out.symbols[symbol].span[1] = last_code_line;
    }
    out.calls.truncate(MAX_CALLS);
    let defined: HashSet<&str> = out.symbols.iter().map(|s| s.name.as_str()).collect();
    let mut counts: HashMap<String, u32> = HashMap::new();
    for name in used.into_iter().filter(|n| n.chars().count() >= 3 && !defined.contains(n.as_str())) {
        *counts.entry(name).or_default() += 1;
    }
    let mut names: Vec<(String, u32)> = counts.into_iter().collect();
    names.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    names.truncate(MAX_NAMES);
    out.references = names;
    out
}

/// Returns the import path of an import line: HFL `import "x"`, C and C++ `#include "x"` (as `./x`).
fn import_of(language: &str, trimmed: &str) -> Option<String> {
    match language {
        "hfl" => trimmed.strip_prefix("import ").map(|p| unquote(p.trim())),
        "c" | "cpp" | "objc" => {
            let rest = trimmed.strip_prefix("#include")?.trim();
            let path = rest.strip_prefix('"')?.split('"').next()?;
            Some(if path.starts_with("../") { path.to_string() } else { format!("./{path}") })
        }
        _ => None,
    }
}

/// Returns the kind and the name of a declaration line. `top` is true outside all declarations.
fn declaration(language: &str, code: &str, top: bool) -> Option<(SymbolKind, String)> {
    if language == "sql" {
        return sql_declaration(code);
    }
    let words: Vec<&str> = code.split_whitespace().collect();
    let rest: Vec<&str> = words.iter().copied().skip_while(|w| MODIFIERS.contains(w) && !(language == "hfl" && *w == "global")).collect();
    let first = *rest.first()?;
    let kind = match first {
        "fn" | "func" | "function" | "fun" | "def" | "defp" | "sub" | "proc" | "procedure" | "method" => Some(SymbolKind::Function),
        "class" | "object" | "actor" => Some(SymbolKind::Class),
        "struct" | "record" | "union" => Some(SymbolKind::Struct),
        "interface" | "protocol" => Some(SymbolKind::Interface),
        "trait" => Some(SymbolKind::Trait),
        "enum" => Some(SymbolKind::Enum),
        "type" | "typedef" => Some(SymbolKind::TypeAlias),
        "module" | "namespace" | "defmodule" => Some(SymbolKind::Module),
        "const" if top => Some(SymbolKind::Constant),
        "global" if rest.get(1) == Some(&"var") => return name_of(rest.get(2)?).map(|n| (SymbolKind::Constant, n)),
        _ => None,
    };
    if let Some(kind) = kind {
        let mut next = rest.get(1).copied()?;
        // Go method: `func (r *T) Name(`.
        if language == "go" && next.starts_with('(') {
            let after = code.split_once(')')?.1.trim_start();
            next = after.split(|c: char| c == '(' || c.is_whitespace()).next()?;
        }
        // Ruby `def self.name`, Lua `function M.name`, Kotlin `fun Type.name`: the last segment.
        let next = next.rsplit(['.', ':']).find(|s| !s.is_empty()).unwrap_or(next);
        return name_of(next).map(|n| (kind, n));
    }
    // Shell `name() {`, R `name <- function(`.
    if language == "shell" {
        let (name, tail) = code.split_once("()")?;
        return (tail.trim_start().starts_with('{') && !name.trim().contains(' ')).then(|| name_of(name.trim())).flatten().map(|n| (SymbolKind::Function, n));
    }
    if language == "r" {
        let (name, tail) = code.split_once("<-")?;
        return tail.trim_start().starts_with("function").then(|| name_of(name.trim())).flatten().map(|n| (SymbolKind::Function, n));
    }
    if c_like(language) {
        return c_function(code).map(|n| (SymbolKind::Function, n));
    }
    None
}

/// A C-like function declaration: `type name(args)` that ends with `{` or `)`, has no `=` before
/// `(` and no `;` at the end. The name is the last segment (`Foo::bar` gives `bar`).
fn c_function(code: &str) -> Option<String> {
    let line = code.trim_end();
    if line.ends_with(';') || line.starts_with('#') || line.starts_with('@') || !(line.ends_with('{') || line.ends_with(')') || line.ends_with("const")) {
        return None;
    }
    let (head, _) = line.split_once('(')?;
    if head.contains('=') || head.contains('.') || NOT_DECLARATION.contains(&head.split_whitespace().next()?) {
        return None;
    }
    let mut parts = head.split_whitespace();
    let name_part = parts.next_back()?;
    // A type before the name is required (`foo(x)` alone is a call).
    parts.next()?;
    let name = name_part.trim_start_matches(['*', '&']).rsplit("::").next()?;
    name_of(name)
}

fn sql_declaration(code: &str) -> Option<(SymbolKind, String)> {
    let upper = code.to_ascii_uppercase();
    let words: Vec<&str> = upper.split_whitespace().collect();
    if words.first() != Some(&"CREATE") {
        return None;
    }
    let skip = ["CREATE", "OR", "REPLACE", "TEMP", "TEMPORARY", "UNIQUE", "MATERIALIZED", "IF", "NOT", "EXISTS"];
    let at = words.iter().position(|w| !skip.contains(w))?;
    let kind = match words[at] {
        "TABLE" | "VIEW" => SymbolKind::Struct,
        "FUNCTION" | "PROCEDURE" | "TRIGGER" => SymbolKind::Function,
        "TYPE" => SymbolKind::TypeAlias,
        _ => return None,
    };
    let original: Vec<&str> = code.split_whitespace().collect();
    let mut index = at + 1;
    while index < words.len() && skip.contains(&words[index]) {
        index += 1;
    }
    let raw = original.get(index)?.split('(').next()?;
    let name = raw.rsplit('.').next()?.trim_matches(['"', '`', '[', ']']);
    name_of(name).map(|n| (kind, n))
}

/// The identifier at the start of a word, or `None`.
fn name_of(word: &str) -> Option<String> {
    let name: String = word.chars().take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '$').collect();
    (!name.is_empty() && !name.chars().next().is_some_and(|c| c.is_ascii_digit())).then_some(name)
}

/// Replaces the text of string literals with spaces and removes a line comment.
fn strip_strings_and_comment(line: &str, comments: &[&str]) -> String {
    let mut out = String::with_capacity(line.len());
    let mut quote: Option<char> = None;
    let mut escaped = false;
    let chars: Vec<char> = line.chars().collect();
    for (i, &c) in chars.iter().enumerate() {
        match quote {
            Some(q) => {
                if escaped {
                    escaped = false;
                } else if c == '\\' {
                    escaped = true;
                } else if c == q {
                    quote = None;
                }
                out.push(if quote.is_none() { c } else { ' ' });
            }
            None => {
                // `#include` and `#define` are not comments in C; `#` is a comment only where it is the marker.
                if comments.iter().any(|m| m.chars().enumerate().all(|(k, mc)| chars.get(i + k) == Some(&mc))) {
                    break;
                }
                if c == '"' || c == '\'' {
                    quote = Some(c);
                }
                out.push(c);
            }
        }
    }
    out
}

/// Adds the call sites and the used identifiers of one line.
fn scan_names(code: &str, line: u32, own: Option<&str>, calls: &mut Vec<CallSite>, used: &mut Vec<String>) {
    let chars: Vec<char> = code.chars().collect();
    let mut i = 0;
    let mut previous: Option<String> = None;
    while i < chars.len() {
        if !(chars[i].is_alphabetic() || chars[i] == '_') {
            if !chars[i].is_whitespace() && chars[i] != '.' {
                previous = None;
            }
            i += 1;
            continue;
        }
        let start = i;
        while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
            i += 1;
        }
        let word: String = chars[start..i].iter().collect();
        let after_dot = start > 0 && (chars[start - 1] == '.' || (start > 1 && chars[start - 1] == '>' && chars[start - 2] == '-'));
        if KEYWORDS.contains(&word.as_str()) {
            previous = None;
            continue;
        }
        if chars.get(i) == Some(&'(') && Some(word.as_str()) != own {
            let qualifier = if after_dot { previous.clone() } else { None };
            calls.push(CallSite { name: word.clone(), method: qualifier.is_some(), qualifier, line });
        }
        if Some(word.as_str()) != own {
            used.push(word.clone());
        }
        previous = Some(word);
    }
}
