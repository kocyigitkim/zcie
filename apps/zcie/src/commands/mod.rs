//! Command groups of the CLI.

pub mod capture;
pub mod claude;
pub mod daemon;
pub mod data;
pub mod doctor;
pub mod import;
pub mod key;
pub mod opencode;
pub mod project;
pub mod project_add;
pub mod service;
pub mod setup;
pub mod usage;
pub mod vector;

use std::path::PathBuf;

use anyhow::Context;
use serde::Serialize;
use zcie_sdk::Client;

use crate::{Cli, Command, ImportSource};

/// Runs one parsed command.
pub fn run(cli: Cli) -> anyhow::Result<()> {
    let home = home_dir(&cli)?;
    let mut cli = cli;
    cli.url = daemon_url(&cli, &home);
    let key = api_key(&cli, &home);
    let project = cli.project.clone().or_else(|| project::find_project_id(&std::env::current_dir().unwrap_or_default()).map(|(_, id)| id)).unwrap_or_else(|| "default".into());
    let client = Client::new(&cli.url, &key, &project);
    match cli.command {
        Command::Daemon { action } => daemon::run(action, &home, &client),
        Command::Repo { action } => data::run_repo(action, &client),
        Command::Query(args) => data::run_query(args, &client),
        Command::Expand(args) => data::run_expand(args, &client),
        Command::Capture(args) => capture::run(args, &client),
        Command::Link(args) => data::run_link(args, &client),
        Command::Forget(args) => data::run_forget(args, &client),
        Command::Health => data::run_health(&client),
        Command::Doctor { fix } => doctor::run(&home, &client, fix),
        Command::RebuildSearch => print_json(&client.rebuild_search()?),
        Command::Key { action } => key::run(action, &home),
        Command::Project { action } => project::run(action, &client),
        Command::Claude { action } => claude::run(action, &home, &cli.url),
        Command::Opencode { action } => opencode::run(action, &home, &cli.url),
        Command::Vector { action } => vector::run(action, &home),
        Command::Import { source: ImportSource::Claude(args) } => import::run(args, &client),
        Command::Setup(args) => setup::run(&args, &home),
        Command::Config { action } => setup::config::run(action, &home),
        Command::Verify { live } => verify(&home, live),
        // A short timeout: the menu bar app runs `zcie usage` every few seconds.
        Command::Usage(args) => usage::run(&args, &home, &Client::with_timeout(&cli.url, &key, &project, std::time::Duration::from_secs(3))),
        Command::Uninstall { purge } => setup::config::uninstall(&home, purge),
    }
}

fn verify(home: &std::path::Path, live: bool) -> anyhow::Result<()> {
    match setup::settings::load(home)? {
        Some(settings) => setup::verify::run(home, &settings, live),
        None => anyhow::bail!("ZCIE is not set up in {}. Run: zcie setup", home.display()),
    }
}

/// Returns the daemon address: `--url` / `ZCIE_URL`, else the port of `zcie setup`, else the default.
fn daemon_url(cli: &Cli, home: &std::path::Path) -> String {
    if cli.url != zcie_sdk::DEFAULT_ENDPOINT || std::env::var_os("ZCIE_URL").is_some() {
        return cli.url.clone();
    }
    match setup::settings::load(home) {
        Ok(Some(settings)) => settings.url(),
        _ => cli.url.clone(),
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
