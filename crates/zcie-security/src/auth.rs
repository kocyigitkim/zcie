//! API keys and the auth context (`docs/11-security.md` section 2).

use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;

use serde::{Deserialize, Serialize};
use zcie_types::LOCAL_TENANT;

use crate::error::{Error, Result};

/// Prefix of a ZCIE API key.
pub const KEY_PREFIX: &str = "zcie_";
/// Number of random bytes in an API key.
pub const KEY_BYTES: usize = 32;
/// Prefix of a stored key hash.
pub const HASH_PREFIX: &str = "b3:";

/// A permission of a client.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Permission {
    /// Query operations.
    Read,
    /// Capture and write operations.
    Write,
    /// Registration, forget, rebuild and audit.
    Admin,
}

/// One client entry in `clients.toml`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClientEntry {
    /// Client name.
    pub name: String,
    /// BLAKE3 hash of the key: `b3:<hex>`.
    pub key_hash: String,
    /// Principal of the client.
    pub principal_id: String,
    /// Permitted workspace names, or `*`.
    pub workspaces: Vec<String>,
    /// Permissions of the client.
    pub permissions: Vec<Permission>,
}

/// The content of `clients.toml`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ClientsFile {
    /// Client entries.
    #[serde(rename = "client", default)]
    pub clients: Vec<ClientEntry>,
}

impl ClientsFile {
    /// Loads `clients.toml`. Returns `NotFound` if the file does not exist.
    pub fn load(path: &Path) -> Result<ClientsFile> {
        if !path.exists() {
            return Err(Error::NotFound(path.display().to_string()));
        }
        let text = std::fs::read_to_string(path)?;
        toml::from_str(&text).map_err(|e| Error::Toml(e.to_string()))
    }

    /// Writes the file with mode 0600. Creates the parent folder with mode 0700.
    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            create_private_dir(parent)?;
        }
        let text = toml::to_string(self).map_err(|e| Error::Toml(e.to_string()))?;
        let mut options = OpenOptions::new();
        options.write(true).create(true).truncate(true);
        set_private_mode(&mut options);
        let mut file = options.open(path)?;
        file.write_all(text.as_bytes())?;
        file.sync_all()?;
        Ok(())
    }

    /// Adds a client entry.
    pub fn add(&mut self, entry: ClientEntry) {
        self.clients.push(entry);
    }
}

#[cfg(unix)]
fn set_private_mode(options: &mut OpenOptions) {
    use std::os::unix::fs::OpenOptionsExt;
    options.mode(0o600);
}

#[cfg(not(unix))]
fn set_private_mode(_options: &mut OpenOptions) {}

fn create_private_dir(dir: &Path) -> Result<()> {
    std::fs::create_dir_all(dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

/// Returns the stored hash of a key: `b3:<hex>`.
pub fn hash_key(key: &str) -> String {
    format!("{HASH_PREFIX}{}", blake3::hash(key.as_bytes()).to_hex())
}

/// Makes a new API key: `zcie_` and 64 hex characters.
pub fn generate_key() -> Result<String> {
    let mut bytes = [0u8; KEY_BYTES];
    getrandom::fill(&mut bytes).map_err(|e| Error::Random(e.to_string()))?;
    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    Ok(format!("{KEY_PREFIX}{hex}"))
}

/// The identity and permissions of one request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthContext {
    /// Tenant ID. Always `local`.
    pub tenant_id: String,
    /// Principal ID.
    pub principal_id: String,
    /// Client name.
    pub client_name: String,
    /// Permitted workspace names or IDs, or `*`.
    pub workspaces: Vec<String>,
    /// Permissions.
    pub permissions: Vec<Permission>,
    /// Version of the policy that applied.
    pub policy_version: u64,
}

impl AuthContext {
    /// Returns the auth context of the development admin key.
    pub fn local_admin() -> AuthContext {
        AuthContext {
            tenant_id: LOCAL_TENANT.to_string(),
            principal_id: "local-admin".to_string(),
            client_name: "dev-key".to_string(),
            workspaces: vec!["*".to_string()],
            permissions: vec![Permission::Read, Permission::Write, Permission::Admin],
            policy_version: 1,
        }
    }

    /// Returns true if the context has the permission. `Admin` implies `Read` and `Write`.
    pub fn can(&self, permission: Permission) -> bool {
        self.permissions.contains(&permission) || self.is_admin()
    }

    /// Returns true if the context has `Admin`.
    pub fn is_admin(&self) -> bool {
        self.permissions.contains(&Permission::Admin)
    }

    /// Returns true if the workspace name or ID is permitted.
    pub fn allows_workspace(&self, name_or_id: &str) -> bool {
        self.workspaces.iter().any(|w| w == "*" || w == name_or_id)
    }

    fn from_entry(entry: &ClientEntry) -> AuthContext {
        AuthContext {
            tenant_id: LOCAL_TENANT.to_string(),
            principal_id: entry.principal_id.clone(),
            client_name: entry.name.clone(),
            workspaces: entry.workspaces.clone(),
            permissions: entry.permissions.clone(),
            policy_version: 1,
        }
    }
}

/// Checks an `Authorization: Bearer <key>` header against the clients and the dev key.
pub fn authenticate(
    header: Option<&str>,
    clients: &ClientsFile,
    dev_key: Option<&str>,
) -> Result<AuthContext> {
    let key = header
        .and_then(|h| h.strip_prefix("Bearer "))
        .map(str::trim)
        .filter(|k| !k.is_empty())
        .ok_or(Error::Unauthorized)?;
    let presented = blake3::hash(key.as_bytes());
    if let Some(dev) = dev_key.filter(|d| !d.is_empty())
        && blake3::hash(dev.as_bytes()) == presented
    {
        return Ok(AuthContext::local_admin());
    }
    for entry in &clients.clients {
        if stored_hash(&entry.key_hash) == Some(presented) {
            return Ok(AuthContext::from_entry(entry));
        }
    }
    Err(Error::Unauthorized)
}

fn stored_hash(value: &str) -> Option<blake3::Hash> {
    let hex = value.strip_prefix(HASH_PREFIX).unwrap_or(value);
    blake3::Hash::from_hex(hex).ok()
}
