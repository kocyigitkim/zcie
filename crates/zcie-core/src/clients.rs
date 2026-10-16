//! API key file setup (`docs/15-configuration.md` section 4).

use std::path::Path;

use zcie_security::{ClientEntry, ClientsFile, Permission, generate_key, hash_key};

use crate::error::Result;

/// Loads `clients.toml`. If it does not exist, makes one admin client and returns its key once.
pub fn ensure_clients(path: &Path) -> Result<(ClientsFile, Option<String>)> {
    if path.exists() {
        return Ok((ClientsFile::load(path)?, None));
    }
    let key = generate_key()?;
    let mut clients = ClientsFile::default();
    clients.add(ClientEntry {
        name: "admin".into(),
        key_hash: hash_key(&key),
        principal_id: "user-001".into(),
        workspaces: vec!["*".into()],
        permissions: vec![Permission::Read, Permission::Write, Permission::Admin],
    });
    clients.save(path)?;
    Ok((clients, Some(key)))
}
