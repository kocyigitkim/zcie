//! Command groups of the CLI.

pub mod capture;
pub mod claude;
pub mod daemon;
pub mod data;
pub mod key;

use std::path::PathBuf;

use anyhow::Context;
use serde::Serialize;
use zcie_sdk::Client;

use crate::{Cli, Command};

/// Runs one parsed command.
pub fn run(cli: Cli) -> anyhow::Result<()> {
    let home = home_dir(&cli)?;
    let key = api_key(&cli, &home);
    let client = Client::new(&cli.url, &key, &cli.workspace);
    match cli.command {
        Command::Daemon { action } => daemon::run(action, &home, &client),
        Command::Repo { action } => data::run_repo(action, &client),
        Command::Query(args) => data::run_query(args, &client),
        Command::Expand(args) => data::run_expand(args, &client),
        Command::Capture(args) => capture::run(args, &client),
        Command::Link(args) => data::run_link(args, &client),
        Command::Forget(args) => data::run_forget(args, &client),
        Command::Health => data::run_health(&client),
        Command::RebuildSearch => print_json(&client.rebuild_search()?),
        Command::Key { action } => key::run(action, &home),
        Command::Claude { action } => claude::run(action, &home, &cli.url),
    }
}

/// Returns the API key: `--key` / `ZCIE_API_KEY`, or the local admin key in `<home>/config/admin.key`.
fn api_key(cli: &Cli, home: &std::path::Path) -> String {
    if !cli.key.is_empty() {
        return cli.key.clone();
    }
    std::fs::read_to_string(home.join("config").join("admin.key")).map(|k| k.trim().to_string()).unwrap_or_default()
}

/// Returns the ZCIE home folder: `--home`, or `~/.zcie`.
fn home_dir(cli: &Cli) -> anyhow::Result<PathBuf> {
    if let Some(home) = &cli.home {
        return Ok(home.clone());
    }
    let user_home = std::env::var("HOME").context("HOME is not set; use --home")?;
    Ok(PathBuf::from(user_home).join(".zcie"))
}

/// Prints a value as pretty JSON on stdout.
pub fn print_json<T: Serialize>(value: &T) -> anyhow::Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}
