//! `zcie`: management CLI. All commands except `daemon start` and `key create` call the HTTP API.

mod commands;

use std::path::PathBuf;

use clap::{Parser, Subcommand};

/// Global options of all commands.
#[derive(Debug, Parser)]
#[command(name = "zcie", version, about = "ZeroContext Intelligence Engine CLI")]
pub struct Cli {
    /// Daemon address.
    #[arg(long, global = true, env = "ZCIE_URL", default_value = zcie_sdk::DEFAULT_ENDPOINT)]
    pub url: String,
    /// Project ID or name. Default: the project of the current folder (`.claude/zcie_id`).
    #[arg(long, visible_alias = "workspace", global = true, env = "ZCIE_WORKSPACE")]
    pub project: Option<String>,
    /// API key.
    #[arg(long, global = true, env = "ZCIE_API_KEY", default_value = "", hide_env_values = true)]
    pub key: String,
    /// ZCIE home folder. Default `~/.zcie`.
    #[arg(long, global = true, env = "ZCIE_HOME")]
    pub home: Option<PathBuf>,
    /// The command.
    #[command(subcommand)]
    pub command: Command,
}

/// Top-level commands.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Start, stop or check the daemon.
    Daemon {
        /// The daemon action.
        #[command(subcommand)]
        action: commands::daemon::DaemonAction,
    },
    /// Manage repositories.
    Repo {
        /// The repository action.
        #[command(subcommand)]
        action: commands::data::RepoAction,
    },
    /// Run a query.
    Query(commands::data::QueryArgs),
    /// Expand refs into evidence or source.
    Expand(commands::data::ExpandArgs),
    /// Capture one event.
    Capture(commands::capture::CaptureArgs),
    /// Add a user-confirmed link.
    Link(commands::data::LinkArgs),
    /// Forget events and derived records.
    Forget(commands::data::ForgetArgs),
    /// Show the daemon health.
    Health,
    /// Rebuild the search index.
    RebuildSearch,
    /// Configure Claude Code for ZCIE and low token use.
    Claude {
        /// The Claude Code action.
        #[command(subcommand)]
        action: commands::claude::ClaudeAction,
    },
    /// Manage projects (central project memory).
    Project {
        /// The project action.
        #[command(subcommand)]
        action: commands::project::ProjectAction,
    },
    /// Manage API keys.
    Key {
        /// The key action.
        #[command(subcommand)]
        action: commands::key::KeyAction,
    },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    commands::run(cli)
}
