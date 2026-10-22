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
    /// Check the daemon, index, key, hooks, project and autostart. `--fix` repairs hooks and starts the daemon.
    Doctor {
        /// Reinstall old hooks and start a stopped daemon.
        #[arg(long)]
        fix: bool,
    },
    /// Rebuild the search index.
    RebuildSearch,
    /// Configure Claude Code for ZCIE and low token use.
    Claude {
        /// The Claude Code action.
        #[command(subcommand)]
        action: commands::claude::ClaudeAction,
    },
    /// Connect OpenCode to ZCIE (plugin and MCP server).
    Opencode {
        /// The OpenCode action.
        #[command(subcommand)]
        action: commands::opencode::OpenCodeAction,
    },
    /// Connect Cursor to ZCIE (hooks and MCP server).
    Cursor {
        /// The Cursor action.
        #[command(subcommand)]
        action: commands::cursor::CursorAction,
    },
    /// Install or check the local embedding model of the vector search.
    Vector {
        /// The vector action.
        #[command(subcommand)]
        action: commands::vector::VectorAction,
    },
    /// Import earlier conversations into the memory of their projects.
    Import {
        /// The source.
        #[command(subcommand)]
        source: ImportSource,
    },
    /// Manage projects (central project memory).
    Project {
        /// The project action.
        #[command(subcommand)]
        action: commands::project::ProjectAction,
    },
    /// Install and configure ZCIE on this computer (wizard). Run it again to change the settings.
    Setup(Box<commands::setup::SetupArgs>),
    /// Show or change the settings of `zcie setup`.
    Config {
        /// The config action.
        #[command(subcommand)]
        action: commands::setup::config::ConfigAction,
    },
    /// Check that the installation works end to end (a new session must remember an earlier one).
    Verify {
        /// Also run two real Claude Code sessions (uses your Claude account, about $0.05).
        #[arg(long)]
        live: bool,
    },
    /// Show the token savings of ZCIE for each project (from the Claude Code transcripts).
    Usage(commands::usage::UsageArgs),
    /// Remove the Claude Code, OpenCode and Cursor entries and the autostart agent, and stop the daemon.
    Uninstall {
        /// Also delete all memory and indexes (the data folder).
        #[arg(long)]
        purge: bool,
    },
    /// Manage API keys.
    Key {
        /// The key action.
        #[command(subcommand)]
        action: commands::key::KeyAction,
    },
}

/// Sources of `zcie import`.
#[derive(Debug, Subcommand)]
pub enum ImportSource {
    /// Claude Code transcripts: the messages that the user wrote (~/.claude/projects).
    Claude(commands::import::ImportArgs),
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    commands::run(cli)
}
