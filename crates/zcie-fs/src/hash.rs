//! BLAKE3 content hashes.

use std::path::Path;

use crate::error::Result;

/// Returns the content hash of bytes: `b3:<hex>`.
pub fn hash_bytes(bytes: &[u8]) -> String {
    format!("b3:{}", blake3::hash(bytes).to_hex())
}

/// Returns the content hash of a file: `b3:<hex>`.
pub fn hash_file(path: &Path) -> Result<String> {
    let bytes = std::fs::read(path)?;
    Ok(hash_bytes(&bytes))
}
