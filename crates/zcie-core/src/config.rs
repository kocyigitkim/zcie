//! Engine configuration (`docs/15-configuration.md`).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// Sections that environment variables `ZCIE_<SECTION>_<KEY>` can change.
const SECTIONS: [&str; 10] = ["server", "storage", "ingest", "index", "query", "cache", "security", "capture", "vector", "context"];

/// `[server]`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ServerConfig {
    /// HTTP listen address.
    pub http_addr: String,
    /// Permit a non-loopback address.
    pub allow_remote: bool,
    /// Environment variable with the development admin key.
    pub api_key_env: String,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self { http_addr: "127.0.0.1:47831".into(), allow_remote: false, api_key_env: "ZCIE_API_KEY".into() }
    }
}

/// `[storage]`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct StorageConfig {
    /// Instance folder.
    pub data_dir: String,
    /// Sync write for capture batches.
    pub sync_writes: bool,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self { data_dir: "~/.zcie/instances/local".into(), sync_writes: true }
    }
}

/// `[ingest]`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct IngestSection {
    /// Largest accepted payload in bytes.
    pub max_payload_bytes: usize,
    /// In-memory queue capacity.
    pub queue_capacity: usize,
    /// Number of workers.
    pub worker_count: usize,
    /// File watcher debounce in milliseconds.
    pub debounce_ms: u64,
}

impl Default for IngestSection {
    fn default() -> Self {
        Self { max_payload_bytes: 1_048_576, queue_capacity: 10_000, worker_count: 4, debounce_ms: 200 }
    }
}

/// `[index]`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct IndexSection {
    /// Larger files get metadata only.
    pub max_file_bytes: u64,
    /// Languages for symbol extraction.
    pub languages: Vec<String>,
}

impl Default for IndexSection {
    fn default() -> Self {
        Self { max_file_bytes: 1_048_576, languages: vec!["rust".into(), "typescript".into(), "python".into()] }
    }
}

/// `[query]`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct QuerySection {
    /// Default token budget.
    pub default_token_budget: u32,
    /// Maximum token budget.
    pub max_token_budget: u32,
    /// Maximum results.
    pub max_results: usize,
    /// Maximum traversal depth.
    pub graph_max_depth: usize,
    /// Maximum edges per node.
    pub graph_max_fanout: usize,
}

impl Default for QuerySection {
    fn default() -> Self {
        Self { default_token_budget: 250, max_token_budget: 4000, max_results: 8, graph_max_depth: 3, graph_max_fanout: 50 }
    }
}

/// `[cache]`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct CacheSection {
    /// Capacity in entries.
    pub l0_capacity_entries: usize,
    /// Lifetime of a "not found" result in milliseconds.
    pub negative_ttl_ms: u64,
}

impl Default for CacheSection {
    fn default() -> Self {
        Self { l0_capacity_entries: 10_000, negative_ttl_ms: 2000 }
    }
}

/// `[security]`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SecuritySection {
    /// Redact secrets in payloads.
    pub redact_secrets: bool,
    /// Extra exclusion globs.
    pub exclude_globs: Vec<String>,
}

impl Default for SecuritySection {
    fn default() -> Self {
        let globs = [".env", ".env.*", "*.pem", "*.key", "id_rsa*"];
        Self { redact_secrets: true, exclude_globs: globs.iter().map(|g| g.to_string()).collect() }
    }
}

/// `[capture]`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct CaptureSection {
    /// Store assistant messages.
    pub assistant_messages: bool,
    /// Maximum bytes of command output.
    pub command_output_max_bytes: usize,
}

impl Default for CaptureSection {
    fn default() -> Self {
        Self { assistant_messages: false, command_output_max_bytes: 65_536 }
    }
}

/// `[vector]` (ADR-011).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct VectorSection {
    /// Use local vector search if the model folder exists.
    pub enabled: bool,
    /// Model folder name under `<home>/models/`.
    pub model: String,
    /// Lowest cosine similarity of a vector hit. Default: the value of the model
    /// (`zcie_vector::default_min_similarity`).
    pub min_similarity: Option<f32>,
}

impl Default for VectorSection {
    fn default() -> Self {
        Self { enabled: true, model: zcie_vector::DEFAULT_MODEL.into(), min_similarity: None }
    }
}

/// The full `engine.toml`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct EngineConfig {
    /// `[server]`.
    pub server: ServerConfig,
    /// `[storage]`.
    pub storage: StorageConfig,
    /// `[ingest]`.
    pub ingest: IngestSection,
    /// `[index]`.
    pub index: IndexSection,
    /// `[query]`.
    pub query: QuerySection,
    /// `[cache]`.
    pub cache: CacheSection,
    /// `[security]`.
    pub security: SecuritySection,
    /// `[capture]`.
    pub capture: CaptureSection,
    /// `[vector]`.
    pub vector: VectorSection,
    /// `[context]`.
    pub context: ContextSection,
}

/// `[context]` (ADR-017): the context-efficiency behavior of the query engine. Each switch turns
/// one change of phase P4 off, so that a benchmark can compare it on and off.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ContextSection {
    /// No duplicates, no capsule for short generic prompts, budgets per class (D-056).
    pub capsule_rules: bool,
    /// Token budget of the repository map; 0 turns the map off (D-058).
    pub repo_map_budget: u32,
    /// Personalized PageRank over the graph for multi-hop questions (D-060).
    pub graph_rank: bool,
    /// Extractive compression of snippets (D-061).
    pub compress: bool,
}

impl Default for ContextSection {
    fn default() -> Self {
        Self { capsule_rules: true, repo_map_budget: 500, graph_rank: true, compress: true }
    }
}

impl EngineConfig {
    /// Loads defaults, then `config_file` (if it exists), then `ZCIE_<SECTION>_<KEY>` variables.
    pub fn load(config_file: &Path) -> Result<EngineConfig> {
        let mut table = toml::Table::try_from(EngineConfig::default()).map_err(|e| Error::Config(e.to_string()))?;
        if config_file.exists() {
            let text = std::fs::read_to_string(config_file)?;
            let file: toml::Table = text.parse().map_err(|e: toml::de::Error| Error::Config(e.to_string()))?;
            merge(&mut table, file);
        }
        for (name, value) in std::env::vars() {
            apply_env(&mut table, &name, &value);
        }
        toml::Value::Table(table).try_into().map_err(|e: toml::de::Error| Error::Config(e.to_string()))
    }

    /// Returns the instance folder. `~` expands to `HOME`. The default follows `home`.
    pub fn data_dir(&self, home: &Path) -> PathBuf {
        if self.storage.data_dir == StorageConfig::default().data_dir {
            return home.join("instances").join("local");
        }
        expand_home(&self.storage.data_dir)
    }
}

fn merge(base: &mut toml::Table, overlay: toml::Table) {
    for (key, value) in overlay {
        match (base.get_mut(&key), value) {
            (Some(toml::Value::Table(base_table)), toml::Value::Table(overlay_table)) => merge(base_table, overlay_table),
            (_, value) => {
                base.insert(key, value);
            }
        }
    }
}

fn apply_env(table: &mut toml::Table, name: &str, value: &str) {
    let Some(rest) = name.strip_prefix("ZCIE_") else { return };
    let lower = rest.to_lowercase();
    let Some(section) = SECTIONS.iter().find(|s| lower.starts_with(&format!("{s}_"))) else { return };
    let key = &lower[section.len() + 1..];
    let parsed = format!("v = {value}")
        .parse::<toml::Table>()
        .ok()
        .and_then(|t| t.get("v").cloned())
        .unwrap_or_else(|| toml::Value::String(value.to_string()));
    if let Some(toml::Value::Table(section_table)) = table.get_mut(*section) {
        section_table.insert(key.to_string(), parsed);
    }
}

/// Expands a leading `~` with the `HOME` environment variable.
pub fn expand_home(path: &str) -> PathBuf {
    match (path.strip_prefix("~/"), std::env::var("HOME")) {
        (Some(rest), Ok(home)) => PathBuf::from(home).join(rest),
        _ => PathBuf::from(path),
    }
}

/// Returns the default ZCIE home folder `~/.zcie`.
pub fn default_home() -> PathBuf {
    expand_home("~/.zcie")
}
