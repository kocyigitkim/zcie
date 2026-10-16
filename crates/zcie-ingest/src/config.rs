//! Configuration values of the write path.

use std::path::PathBuf;

use zcie_security::ExcludeSet;

/// Settings of `Ingest`. `zcie-core` fills them from `engine.toml`.
#[derive(Debug, Clone)]
pub struct IngestConfig {
    /// Tenant ID.
    pub tenant: String,
    /// Largest accepted payload in bytes.
    pub max_payload_bytes: usize,
    /// Sync write for capture batches.
    pub sync_writes: bool,
    /// Store `assistant.message` events.
    pub assistant_messages: bool,
    /// Maximum bytes of command and tool output.
    pub command_output_max_bytes: usize,
    /// Apply secret redaction to payloads.
    pub redact_secrets: bool,
    /// Larger files get metadata only.
    pub max_file_bytes: u64,
    /// Languages for symbol extraction.
    pub languages: Vec<String>,
    /// Folder for dead-letter JSON files.
    pub dead_letter_dir: PathBuf,
    /// Exclusion globs.
    pub exclude: ExcludeSet,
}
