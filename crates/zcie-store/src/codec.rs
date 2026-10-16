//! Value codec. Values are JSON (`docs/adr/ADR-002-json-values.md`).
//!
//! This is the only module that encodes and decodes stored values.

use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::error::Result;

/// Encodes a value as JSON bytes.
pub fn encode<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    Ok(serde_json::to_vec(value)?)
}

/// Decodes JSON bytes into a value.
pub fn decode<T: DeserializeOwned>(bytes: &[u8]) -> Result<T> {
    Ok(serde_json::from_slice(bytes)?)
}

/// A small value with one number, for counters and watermarks.
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub struct NumberValue {
    /// Value schema version.
    pub v: u32,
    /// The number.
    pub value: u64,
}

impl NumberValue {
    /// Makes a version 1 number value.
    pub fn new(value: u64) -> Self {
        Self { v: 1, value }
    }
}
