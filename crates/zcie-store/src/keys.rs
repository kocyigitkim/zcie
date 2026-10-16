//! Key helpers (`docs/06-storage.md` section 2).

/// The byte that separates key parts.
pub const SEPARATOR: u8 = 0x00;

/// Joins key parts with the byte 0x00.
pub fn key(parts: &[&str]) -> Vec<u8> {
    let mut out = Vec::with_capacity(parts.iter().map(|p| p.len() + 1).sum());
    for (index, part) in parts.iter().enumerate() {
        if index > 0 {
            out.push(SEPARATOR);
        }
        out.extend_from_slice(part.as_bytes());
    }
    out
}

/// Makes a prefix for a prefix scan. The prefix ends with 0x00.
pub fn prefix(parts: &[&str]) -> Vec<u8> {
    let mut out = key(parts);
    out.push(SEPARATOR);
    out
}

/// Encodes a sequence number as a 20-digit zero-padded string.
pub fn seq_part(seq: u64) -> String {
    format!("{seq:020}")
}

/// Splits a key into its parts. Invalid UTF-8 parts are replaced.
pub fn split_key(raw: &[u8]) -> Vec<String> {
    raw.split(|b| *b == SEPARATOR)
        .map(|part| String::from_utf8_lossy(part).into_owned())
        .collect()
}

/// Returns the last part of a key.
pub fn last_part(raw: &[u8]) -> String {
    split_key(raw).pop().unwrap_or_default()
}
