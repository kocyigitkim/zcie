//! `zcie claude setup | status | remove`: configure Claude Code for low token use with ZCIE.
//!
//! The setup installs the hook scripts, makes an API key file, registers the project in ZCIE,
//! and merges a lean tool profile plus the ZCIE hooks into a Claude Code settings file.

pub mod hooks;
pub mod settings;
pub mod setup;

use std::path::PathBuf;

use clap::{Args, Subcommand, ValueEnum};

/// Where the settings go.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Scope {
    /// `<project>/.claude/settings.local.json` (personal, not shared).
    Project,
    /// `~/.claude/settings.json` (all projects).
    User,
}

/// Which built-in tools the setup denies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Profile {
    /// Deferred tool loading (`ENABLE_TOOL_SEARCH=true`) and 6 non-deferrable heavy tools denied.
    /// The model loads the other tools itself when it needs them. Measured: −26 % to −46 % cost
    /// for short tasks and edits, the same cost as the default in long sessions.
    Auto,
    /// Deny 15 heavy, rarely used tools; keep web, subagent, skill, LSP and notebook tools (about −48 % context).
    Balanced,
    /// Deny 21 tools; keep only Bash, Read, Edit, Write (about −77 % context).
    Lean,
}

/// Options of `zcie claude setup`.
#[derive(Debug, Args)]
pub struct SetupArgs {
    /// Settings scope.
    #[arg(long, value_enum, default_value = "project")]
    pub scope: Scope,
    /// Project folder. Default: the current folder.
    #[arg(long)]
    pub dir: Option<PathBuf>,
    /// Project name. Default: the project folder name.
    #[arg(long = "name", visible_alias = "for-workspace")]
    pub for_workspace: Option<String>,
    /// Tool profile. `auto` (default) keeps automatic tool use; `lean` is the cheapest; `balanced`
    /// keeps web, subagent and skill tools loaded but cost more in the benchmark (cache rewrites).
    #[arg(long, value_enum, default_value = "auto")]
    pub profile: Profile,
    /// Keep these tools available (comma separated), for example `WebFetch,Task`.
    #[arg(long, default_value = "")]
    pub allow: String,
    /// Do not deny unused built-in tools.
    #[arg(long)]
    pub no_lean: bool,
    /// Do not inject ZCIE context into prompts.
    #[arg(long)]
    pub no_inject: bool,
    /// Do not install the read guard, session guard and session rules.
    #[arg(long)]
    pub no_guards: bool,
    /// Do not capture prompts and file changes into ZCIE memory.
    #[arg(long)]
    pub no_capture: bool,
    /// Use 5-minute prompt-cache writes (cheaper for continuous work; a pause over 5 minutes
    /// makes the next turn write the whole context again).
    #[arg(long)]
    pub cache_5m: bool,
    /// Do not register the project folder in ZCIE.
    #[arg(long)]
    pub no_register: bool,
    /// Token budget of the context capsule of each prompt (default of the hook: 1200).
    #[arg(long)]
    pub budget: Option<u32>,
    /// Do not check the project when a turn changed code (no verification gate, ADR-020).
    #[arg(long)]
    pub no_check: bool,
    /// The context hook reads the current project only (no projects that a prompt names).
    #[arg(long)]
    pub no_cross_project: bool,
    /// Show the result without writing files.
    #[arg(long)]
    pub dry_run: bool,
}

/// Claude Code actions.
#[derive(Debug, Subcommand)]
pub enum ClaudeAction {
    /// Configure Claude Code for ZCIE and low token use.
    Setup(SetupArgs),
    /// Show the ZCIE configuration of Claude Code.
    Status {
        /// Settings scope.
        #[arg(long, value_enum, default_value = "project")]
        scope: Scope,
        /// Project folder. Default: the current folder.
        #[arg(long)]
        dir: Option<PathBuf>,
    },
    /// Enable or disable built-in Claude Code tools (edits the deny list; applies at once).
    Tools {
        /// `enable` or `disable`.
        #[arg(value_parser = ["enable", "disable"])]
        action: String,
        /// Tool names, for example `WebFetch WebSearch Task`.
        #[arg(required = true)]
        names: Vec<String>,
        /// Settings scope.
        #[arg(long, value_enum, default_value = "project")]
        scope: Scope,
        /// Project folder. Default: the current folder.
        #[arg(long)]
        dir: Option<PathBuf>,
    },
    /// Remove exactly the entries that `setup` added.
    Remove {
        /// Settings scope.
        #[arg(long, value_enum, default_value = "project")]
        scope: Scope,
        /// Project folder. Default: the current folder.
        #[arg(long)]
        dir: Option<PathBuf>,
    },
}

/// Runs one Claude Code action.
pub fn run(action: ClaudeAction, home: &std::path::Path, url: &str) -> anyhow::Result<()> {
    match action {
        ClaudeAction::Setup(args) => setup::setup(&args, home, url),
        ClaudeAction::Status { scope, dir } => setup::status(scope, dir, home),
        ClaudeAction::Remove { scope, dir } => setup::remove(scope, dir, home),
        ClaudeAction::Tools { action, names, scope, dir } => setup::tools(scope, dir, home, action == "enable", &names),
    }
}
