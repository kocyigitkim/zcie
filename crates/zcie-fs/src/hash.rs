//! BLAKE3 content hashes.

use std::io::Read;
use std::path::Path;

use crate::error::Result;

/// Returns the content hash of bytes: `b3:<hex>`.
pub fn hash_bytes(bytes: &[u8]) -> String {
    format!("b3:{}", blake3::hash(bytes).to_hex())
}

/// Bytes that one read of a streamed hash takes.
const HASH_CHUNK_BYTES: usize = 64 * 1024;

/// Returns the content hash of a file: `b3:<hex>`. The file is read in chunks, so a file of any
/// size uses a constant amount of memory.
pub fn hash_file(path: &Path) -> Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = blake3::Hasher::new();
    let mut buffer = vec![0u8; HASH_CHUNK_BYTES];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            return Ok(format!("b3:{}", hasher.finalize().to_hex()));
        }
        hasher.update(&buffer[..read]);
    }
}

/// Returns the first `count` bytes of a file (fewer for a short file).
pub fn read_head(path: &Path, count: usize) -> Result<Vec<u8>> {
    let mut head = Vec::with_capacity(count);
    std::fs::File::open(path)?.take(count as u64).read_to_end(&mut head)?;
    Ok(head)
}
