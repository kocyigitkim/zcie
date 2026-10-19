//! `zcie doctor`: checks the parts that must work together for a complete memory: the daemon,
//! the index queues, the key, the installed hooks, the project of the current folder and the
//! autostart agent. `--fix` repairs what it can (hooks, daemon start).

use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use anyhow::bail;
use zcie_sdk::Client;

use super::claude::hooks;
use super::project::{ID_FILE, find_project_id};

/// Queued jobs above this count mean that the index is behind.
const QUEUE_WARN: u64 = 1_000;

/// The result of one check.
#[derive(Debug, PartialEq, Eq)]
enum Level {
    Ok,
    Info,
    Warn,
    Fail,
}

struct Check {
    level: Level,
    text: String,
}

fn check(level: Level, text: impl Into<String>) -> Check {
    Check { level, text: text.into() }
}

/// Runs all checks and prints one line per check. Fails if a check fails.
pub fn run(home: &Path, client: &Client, fix: bool) -> anyhow::Result<()> {
    if fix {
        repair(home, client)?;
    }
    let cwd = std::env::current_dir()?;
    let mut checks = daemon_checks(client);
    checks.push(key_check(home));
    checks.push(hooks_check(home));
    checks.push(project_check(client, &cwd));
    checks.push(settings_check(home, &cwd));
    checks.push(autostart_check(home));
    checks.push(vector_check(home));
    for c in &checks {
        let tag = match c.level {
            Level::Ok => "ok  ",
            Level::Info => "info",
            Level::Warn => "WARN",
            Level::Fail => "FAIL",
        };
        println!("{tag}  {}", c.text);
    }
    if checks.iter().any(|c| c.level == Level::Fail) {
        bail!("some checks failed");
    }
    Ok(())
}

fn repair(home: &Path, client: &Client) -> anyhow::Result<()> {
    if !hooks::outdated(home).is_empty() {
        hooks::install(home)?;
        println!("fixed  hooks reinstalled in {}", home.join("hooks").display());
    }
    if client.health().is_err() {
        super::daemon::run(super::daemon::DaemonAction::Start { foreground: false }, home, client)?;
    }
    Ok(())
}

fn daemon_checks(client: &Client) -> Vec<Check> {
    let health = match client.health() {
        Ok(health) => health,
        Err(_) => return vec![check(Level::Fail, "daemon is not running: hooks record nothing. Fix: zcie daemon start (or zcie daemon install)")],
    };
    let mut out = vec![check(Level::Ok, format!("daemon {} running for {} s", health.version, health.uptime_s))];
    if let Some(queue) = &health.queue {
        if queue.dead_letter > 0 {
            out.push(check(Level::Warn, format!("{} failed jobs (dead letter); see the daemon log", queue.dead_letter)));
        }
        let pending = queue.interactive + queue.background;
        let level = if pending > QUEUE_WARN { Level::Warn } else { Level::Ok };
        out.push(check(level, format!("index queue: {pending} jobs pending")));
    }
    if let Some(wm) = &health.watermarks {
        let behind = wm.canonical_seq.saturating_sub(wm.search_seq.min(wm.graph_seq));
        out.push(check(if behind == 0 { Level::Ok } else { Level::Info }, format!("index is {behind} events behind the store")));
    }
    out
}

fn key_check(home: &Path) -> Check {
    let path = std::env::var("ZCIE_API_KEY_FILE").map(Into::into).unwrap_or_else(|_| home.join("config/claude-code.key"));
    match std::fs::metadata(&path) {
        Ok(meta) if meta.permissions().mode() & 0o077 != 0 => {
            check(Level::Warn, format!("key file {} is readable by others. Fix: chmod 600 {}", path.display(), path.display()))
        }
        Ok(_) => check(Level::Ok, format!("key file {}", path.display())),
        Err(_) => check(Level::Warn, format!("no key file {}. Fix: zcie claude setup", path.display())),
    }
}

fn hooks_check(home: &Path) -> Check {
    match hooks::outdated(home).as_slice() {
        [] => check(Level::Ok, format!("hooks in {} match this zcie version", home.join("hooks").display())),
        names => check(Level::Warn, format!("hooks are missing or old: {}. Fix: zcie doctor --fix", names.join(", "))),
    }
}

fn project_check(client: &Client, cwd: &Path) -> Check {
    let Some((root, id)) = find_project_id(cwd) else {
        return check(Level::Info, format!("no {ID_FILE} in this folder or a parent; the first Claude Code session makes it (or: zcie project init)"));
    };
    match client.project(&id) {
        Ok(info) => check(Level::Ok, format!("project {} ({id}) at {}", info.name, root.display())),
        Err(_) => check(Level::Warn, format!("{} has {id}, but the daemon does not know it. Fix: zcie project init", root.join(ID_FILE).display())),
    }
}

fn settings_check(home: &Path, cwd: &Path) -> Check {
    let hook_dir = home.join("hooks").display().to_string();
    let candidates = [cwd.join(".claude/settings.local.json"), cwd.join(".claude/settings.json")];
    let user = std::env::var("HOME").map(|h| Path::new(&h).join(".claude/settings.json")).ok();
    let found = candidates.iter().chain(user.iter()).find(|p| std::fs::read_to_string(p).is_ok_and(|t| t.contains(&hook_dir)));
    match found {
        Some(path) => check(Level::Ok, format!("Claude Code hooks are set in {}", path.display())),
        None => check(Level::Info, "Claude Code hooks are not set for this folder. Fix: zcie claude setup"),
    }
}

fn vector_check(home: &Path) -> Check {
    let models = home.join("models");
    match zcie_vector::model_folder(&models, zcie_vector::DEFAULT_MODEL) {
        Some(folder) if folder.ends_with(zcie_vector::DEFAULT_MODEL) => check(Level::Ok, format!("vector search model in {} (zcie vector status checks the files)", folder.display())),
        Some(folder) => check(Level::Info, format!("vector search uses {}; the multilingual model is better for Turkish. Fix: zcie vector install", folder.display())),
        None => check(Level::Info, "vector search is off: no model. Fix: zcie vector install"),
    }
}

fn autostart_check(home: &Path) -> Check {
    match super::service::agent_file(home) {
        Ok(file) if file.exists() => check(Level::Ok, format!("autostart agent {}", file.display())),
        _ => check(Level::Info, "the daemon does not start at login. Fix: zcie daemon install"),
    }
}
