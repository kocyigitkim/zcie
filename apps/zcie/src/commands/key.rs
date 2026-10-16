//! `zcie key create`: make an API key locally. No daemon is needed.

use std::path::Path;

use anyhow::bail;
use clap::Subcommand;
use zcie_security::{ClientEntry, ClientsFile, Permission, generate_key, hash_key};

/// Key actions.
#[derive(Debug, Subcommand)]
pub enum KeyAction {
    /// Make a new API key and store its hash in clients.toml.
    Create {
        /// Client name.
        #[arg(long)]
        name: String,
        /// Principal of the client.
        #[arg(long, default_value = "user-001")]
        principal: String,
        /// Permitted workspaces, comma separated, or `*`.
        #[arg(long, default_value = "*")]
        workspaces: String,
        /// Permissions, comma separated: read, write, admin.
        #[arg(long, default_value = "read,write")]
        permissions: String,
    },
}

/// Runs one key action.
pub fn run(action: KeyAction, home: &Path) -> anyhow::Result<()> {
    match action {
        KeyAction::Create { name, principal, workspaces, permissions } => {
            create(home, name, principal, &workspaces, &permissions)
        }
    }
}

fn create(home: &Path, name: String, principal: String, workspaces: &str, permissions: &str) -> anyhow::Result<()> {
    let path = home.join("config/clients.toml");
    let mut file = if path.exists() { ClientsFile::load(&path)? } else { ClientsFile::default() };
    if file.clients.iter().any(|c| c.name == name) {
        bail!("a client with the name {name} exists in {}", path.display());
    }
    let key = generate_key()?;
    file.add(ClientEntry {
        name: name.clone(),
        key_hash: hash_key(&key),
        principal_id: principal,
        workspaces: split_list(workspaces),
        permissions: parse_permissions(permissions)?,
    });
    file.save(&path)?;
    eprintln!("Created client {name} in {}. The key is shown one time only:", path.display());
    println!("{key}");
    eprintln!("Restart the daemon (or send SIGHUP) to load the new key.");
    Ok(())
}

fn split_list(value: &str) -> Vec<String> {
    value.split(',').map(str::trim).filter(|v| !v.is_empty()).map(str::to_string).collect()
}

fn parse_permissions(value: &str) -> anyhow::Result<Vec<Permission>> {
    split_list(value)
        .iter()
        .map(|name| match name.as_str() {
            "read" => Ok(Permission::Read),
            "write" => Ok(Permission::Write),
            "admin" => Ok(Permission::Admin),
            other => bail!("unknown permission {other}"),
        })
        .collect()
}
