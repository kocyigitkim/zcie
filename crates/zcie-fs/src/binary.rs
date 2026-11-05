//! Language and binary detection.

/// Number of bytes that binary detection examines.
pub const BINARY_PROBE_BYTES: usize = 8 * 1024;

/// Returns the language of a path from its extension.
pub fn language_for_path(path: &str) -> Option<&'static str> {
    match extension(path).as_deref() {
        Some("rs") => Some("rust"),
        Some("ts" | "tsx" | "mts" | "cts") => Some("typescript"),
        Some("js" | "jsx" | "mjs" | "cjs") => Some("javascript"),
        Some("py") => Some("python"),
        Some("hfl") => Some("hfl"),
        // The languages of the line extractor (ADR-030).
        Some("go") => Some("go"),
        Some("java") => Some("java"),
        Some("kt" | "kts") => Some("kotlin"),
        Some("scala" | "sc") => Some("scala"),
        Some("swift") => Some("swift"),
        Some("c" | "h") => Some("c"),
        Some("cc" | "cpp" | "cxx" | "c++" | "hpp" | "hh" | "hxx" | "ipp") => Some("cpp"),
        Some("cs") => Some("csharp"),
        Some("m" | "mm") => Some("objc"),
        Some("php") => Some("php"),
        Some("rb" | "rake") => Some("ruby"),
        Some("lua") => Some("lua"),
        Some("dart") => Some("dart"),
        Some("sh" | "bash" | "zsh") => Some("shell"),
        Some("ps1" | "psm1") => Some("powershell"),
        Some("sql") => Some("sql"),
        Some("r") => Some("r"),
        Some("jl") => Some("julia"),
        Some("ex" | "exs") => Some("elixir"),
        Some("pl" | "pm") => Some("perl"),
        Some("zig") => Some("zig"),
        Some("nim") => Some("nim"),
        Some("gd") => Some("gdscript"),
        Some("pas" | "pp") => Some("pascal"),
        Some("groovy" | "gradle") => Some("groovy"),
        _ => None,
    }
}

/// Returns true if the path is a text document (Markdown or plain text).
pub fn is_document(path: &str) -> bool {
    matches!(extension(path).as_deref(), Some("md" | "markdown" | "txt" | "rst"))
}

/// Returns true if the first 8 KiB contain a 0x00 byte.
pub fn is_binary(bytes: &[u8]) -> bool {
    let end = bytes.len().min(BINARY_PROBE_BYTES);
    bytes[..end].contains(&0)
}

fn extension(path: &str) -> Option<String> {
    let name = path.rsplit('/').next().unwrap_or(path);
    let (_, ext) = name.rsplit_once('.')?;
    Some(ext.to_ascii_lowercase())
}
