//! `zcie config show|set|path` and `zcie uninstall` (ADR-014).

use std::path::Path;

use anyhow::bail;
use clap::Subcommand;

use super::settings::{self, KEYS};
use super::{apply, t};

/// Config actions.
#[derive(Debug, Subcommand)]
pub enum ConfigAction {
    /// Show the settings and their allowed values.
    Show,
    /// Change settings and apply them at once, for example `zcie config set budget 1500 vector english`.
    Set {
        /// Pairs of key and value.
        #[arg(required = true, num_args = 2..)]
        pairs: Vec<String>,
        /// Do not run `zcie verify` after the change.
        #[arg(long)]
        no_verify: bool,
    },
    /// Print the path of the settings file.
    Path,
}

/// Runs one config action.
pub fn run(action: ConfigAction, home: &Path) -> anyhow::Result<()> {
    let Some(current) = settings::load(home)? else {
        bail!("ZCIE is not set up in {}. Run: zcie setup", home.display());
    };
    match action {
        ConfigAction::Show => {
            for (key, allowed) in KEYS {
                println!("{key:<11} = {:<28} ({allowed})", current.get(key));
            }
            println!("\n{}: {}", t(&current.language, "file", "dosya"), settings::file(home).display());
            Ok(())
        }
        ConfigAction::Path => {
            println!("{}", settings::file(home).display());
            Ok(())
        }
        ConfigAction::Set { pairs, no_verify } => set(home, &current, &pairs, no_verify),
    }
}

fn set(home: &Path, current: &settings::Settings, pairs: &[String], no_verify: bool) -> anyhow::Result<()> {
    if !pairs.len().is_multiple_of(2) {
        bail!("give pairs of key and value, for example: zcie config set budget 1500");
    }
    let mut new = current.clone();
    for pair in pairs.chunks(2) {
        new.set(&pair[0], &pair[1])?;
    }
    new.validate()?;
    if new == *current {
        println!("{}", t(&new.language, "Nothing changed.", "Değişiklik yok."));
        return Ok(());
    }
    for (key, _) in KEYS {
        if new.get(key) != current.get(key) {
            println!("{key}: {} → {}", current.get(key), new.get(key));
        }
    }
    apply::apply(home, Some(current), &new)?;
    if no_verify {
        return Ok(());
    }
    super::verify::run(home, &new, false)
}

/// Removes the Claude Code and OpenCode entries and the autostart agent, and stops the daemon.
/// `purge` also deletes the data folder.
pub fn uninstall(home: &Path, purge: bool) -> anyhow::Result<()> {
    if let Some(current) = settings::load(home)? {
        apply::remove_all(home, &current)?;
    } else if crate::commands::daemon::is_running(home) {
        crate::commands::daemon::stop(home)?;
    }
    if purge {
        std::fs::remove_dir_all(home)?;
        println!("removed {}", home.display());
    } else {
        println!("The memory stays in {} (delete it with: zcie uninstall --purge).", home.display());
    }
    println!("Remove the program with: npm uninstall -g zcie");
    Ok(())
}
