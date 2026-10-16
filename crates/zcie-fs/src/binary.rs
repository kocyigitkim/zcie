//! Language and binary detection.

/// Number of bytes that binary detection examines.
pub const BINARY_PROBE_BYTES: usize = 8 * 1024;

/// Returns the language of a path from its extension.
pub fn language_for_path(path: &str) -> Option<&'static str> {
    match extension(path).as_deref() {
        Some("rs") => Some("rust"),
        Some("ts" | "tsx" | "mts" | "cts") => Some("typescript"),
        Some("py") => Some("python"),
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
