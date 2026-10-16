//! Runtime tree, permissions and manifest (`docs/04-folder-structure.md` section 6).

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use zcie_types::now_utc;

use crate::error::{Error, Result};

/// Schema version that this daemon supports.
pub const SCHEMA_VERSION: u32 = 1;

/// Paths of one instance.
#[derive(Debug, Clone)]
pub struct InstancePaths {
    /// ZCIE home (`~/.zcie`).
    pub home: PathBuf,
    /// Instance folder (`storage.data_dir`).
    pub data: PathBuf,
}

impl InstancePaths {
    /// `config/engine.toml`.
    pub fn engine_toml(&self) -> PathBuf {
        self.home.join("config").join("engine.toml")
    }
    /// `config/clients.toml`.
    pub fn clients_toml(&self) -> PathBuf {
        self.home.join("config").join("clients.toml")
    }
    /// `run/` folder.
    pub fn run_dir(&self) -> PathBuf {
        self.home.join("run")
    }
    /// `state/rocksdb`.
    pub fn rocksdb(&self) -> PathBuf {
        self.data.join("state").join("rocksdb")
    }
    /// `state/search`.
    pub fn search(&self) -> PathBuf {
        self.data.join("state").join("search")
    }
    /// `state/objects`.
    pub fn objects(&self) -> PathBuf {
        self.data.join("state").join("objects")
    }
    /// `queue/dead-letter`.
    pub fn dead_letter(&self) -> PathBuf {
        self.data.join("queue").join("dead-letter")
    }
    /// `telemetry/logs`.
    pub fn logs(&self) -> PathBuf {
        self.data.join("telemetry").join("logs")
    }
}

/// Creates the runtime tree with mode 0700 folders.
pub fn create_tree(paths: &InstancePaths) -> Result<()> {
    let folders = [
        paths.home.clone(),
        paths.home.join("config"),
        paths.home.join("config").join("policies"),
        paths.run_dir(),
        paths.home.join("exports"),
        paths.data.clone(),
        paths.data.join("state"),
        paths.rocksdb(),
        paths.search(),
        paths.objects(),
        paths.data.join("state").join("checkpoints"),
        paths.dead_letter(),
        paths.data.join("cache"),
        paths.data.join("backups"),
        paths.logs(),
    ];
    for folder in folders {
        fs::create_dir_all(&folder)?;
        set_mode(&folder, 0o700)?;
    }
    Ok(())
}

/// Sets the Unix mode of a path.
pub fn set_mode(path: &Path, mode: u32) -> Result<()> {
    let current = fs::metadata(path)?.permissions().mode() & 0o777;
    if current != mode {
        fs::set_permissions(path, fs::Permissions::from_mode(mode))?;
    }
    Ok(())
}

/// Writes a file with mode 0600.
pub fn write_private(path: &Path, bytes: &[u8]) -> Result<()> {
    fs::write(path, bytes)?;
    set_mode(path, 0o600)
}

/// `manifest.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    /// Value schema version.
    pub v: u32,
    /// Data schema version.
    pub schema_version: u32,
    /// Instance name.
    pub instance: String,
    /// RFC 3339 creation time.
    pub created_utc: String,
}

/// Reads the manifest or makes a new one. Stops if the schema version is too high.
pub fn check_manifest(paths: &InstancePaths) -> Result<Manifest> {
    let path = paths.data.join("manifest.json");
    if path.exists() {
        let manifest: Manifest = serde_json::from_slice(&fs::read(&path)?).map_err(|e| Error::Manifest(e.to_string()))?;
        if manifest.schema_version > SCHEMA_VERSION {
            return Err(Error::Manifest(format!("schema version {} is not supported", manifest.schema_version)));
        }
        return Ok(manifest);
    }
    let manifest = Manifest { v: 1, schema_version: SCHEMA_VERSION, instance: "local".into(), created_utc: now_utc() };
    let bytes = serde_json::to_vec_pretty(&manifest).map_err(|e| Error::Manifest(e.to_string()))?;
    write_private(&path, &bytes)?;
    Ok(manifest)
}
