//! Makes the system match the settings: `engine.toml`, the vector model, the daemon and its
//! autostart agent, the Claude Code settings and the OpenCode plugin (ADR-014).

use std::path::{Path, PathBuf};

use anyhow::Context;
use zcie_sdk::Client;

use super::settings::Settings;
use super::t;
use crate::commands::claude::{self, Profile, Scope, SetupArgs};
use crate::commands::{daemon, opencode, service, vector};

/// Applies `new`. `old` is the previous state (`None` for a first setup).
pub fn apply(home: &Path, old: Option<&Settings>, new: &Settings) -> anyhow::Result<()> {
    let lang = new.language.as_str();
    std::fs::create_dir_all(home.join("config"))?;
    write_engine_toml(home, new)?;
    write_client_json(home, new)?;
    if let Some(model) = new.model().filter(|m| !vector::model_present(home, m)) {
        println!("{}", t(lang, "Installing the vector model (download once)…", "Vektör modeli kuruluyor (bir kez indirilir)…"));
        vector::install_model(home, model)?;
    }
    crate::commands::claude::hooks::install(home)?;
    let engine_changed = old.is_none_or(|o| new.engine_differs(o));
    run_daemon(home, new, engine_changed)?;
    claude_code(home, old, new)?;
    open_code(home, old, new)?;
    super::settings::save(home, new)?;
    Ok(())
}

/// Sets the engine values of the settings in `engine.toml`; other values in the file stay.
fn write_engine_toml(home: &Path, settings: &Settings) -> anyhow::Result<()> {
    let path = home.join("config").join("engine.toml");
    let mut root: toml::Table = match std::fs::read_to_string(&path) {
        Ok(text) => text.parse().with_context(|| format!("{} is not valid TOML", path.display()))?,
        Err(_) => toml::Table::new(),
    };
    let mut set = |section: &str, key: &str, value: toml::Value| {
        let table = root.entry(section).or_insert_with(|| toml::Value::Table(toml::Table::new()));
        if let toml::Value::Table(table) = table {
            table.insert(key.to_string(), value);
        }
    };
    set("server", "http_addr", toml::Value::String(format!("127.0.0.1:{}", settings.port)));
    set("ingest", "worker_count", toml::Value::Integer(settings.workers as i64));
    set("capture", "assistant_messages", toml::Value::Boolean(settings.answers));
    set("vector", "enabled", toml::Value::Boolean(settings.model().is_some()));
    if let Some(model) = settings.model() {
        set("vector", "model", toml::Value::String(model.to_string()));
    }
    std::fs::write(&path, format!("# Written by zcie setup / zcie config (setup.toml has the settings).\n{}", toml::to_string(&root)?))?;
    Ok(())
}

/// Writes the address and the capsule budget for clients that have no environment of their own
/// (the OpenCode plugin).
fn write_client_json(home: &Path, settings: &Settings) -> anyhow::Result<()> {
    let value = serde_json::json!({ "url": settings.url(), "budget": settings.budget });
    std::fs::write(home.join("config").join("client.json"), serde_json::to_string_pretty(&value)? + "\n")?;
    Ok(())
}

/// Starts or restarts the daemon so that it runs with the new settings, with or without the
/// launchd agent.
fn run_daemon(home: &Path, settings: &Settings, engine_changed: bool) -> anyhow::Result<()> {
    let lang = settings.language.as_str();
    let client = Client::new(&settings.url(), "", "setup");
    let binary = daemon::daemon_binary()?;
    let use_agent = settings.autostart && cfg!(target_os = "macos");
    let loaded = cfg!(target_os = "macos") && service::is_loaded(home);
    if use_agent && loaded {
        let (_, binary_changed) = service::sync_daemon_copy(&binary, home)?;
        if engine_changed || binary_changed || client.health().is_err() {
            service::kickstart(&binary, home)?;
        }
    } else if use_agent {
        if daemon::is_running(home) {
            daemon::stop(home)?;
        }
        service::install(&binary, home, false)?;
    } else {
        if loaded {
            service::uninstall(home)?;
        }
        if daemon::is_running(home) && (engine_changed || client.health().is_err()) {
            daemon::stop(home)?;
        }
        if !daemon::is_running(home) {
            daemon::start(home, &client, false)?;
        }
    }
    daemon::wait_healthy(&client, &daemon::log_file(home)?)?;
    println!("{} {}", t(lang, "Daemon is running at", "Daemon çalışıyor:"), settings.url());
    Ok(())
}

fn claude_args(settings: &Settings) -> SetupArgs {
    let (scope, dir) = match settings.claude.as_str() {
        "project" => (Scope::Project, Some(PathBuf::from(&settings.claude_dir))),
        _ => (Scope::User, None),
    };
    let profile = match settings.profile.as_str() {
        "balanced" => Profile::Balanced,
        "lean" => Profile::Lean,
        _ => Profile::Auto,
    };
    SetupArgs {
        scope,
        dir,
        for_workspace: None,
        profile,
        allow: String::new(),
        no_lean: settings.profile == "off",
        no_inject: false,
        no_guards: false,
        no_capture: false,
        cache_5m: false,
        no_register: false,
        budget: Some(settings.budget),
        no_check: settings.check == "off",
        no_cross_project: !settings.cross_project,
        dry_run: false,
    }
}

/// Sets up Claude Code for the chosen scope. A previous scope that is no longer chosen is removed.
fn claude_code(home: &Path, old: Option<&Settings>, new: &Settings) -> anyhow::Result<()> {
    if let Some(old) = old.filter(|o| o.claude != "none" && (o.claude != new.claude || o.claude_dir != new.claude_dir)) {
        let args = claude_args(old);
        claude::setup::remove(args.scope, args.dir, home)?;
    }
    if new.claude == "none" {
        return Ok(());
    }
    if new.claude == "user" {
        remove_folder_setups(home, new)?;
    }
    claude::setup::setup(&claude_args(new), home, &new.url())
}

/// Removes the ZCIE entries of folder setups (`zcie claude setup --scope project`) of the
/// registered projects. With the setup for all projects they would run each hook twice: two
/// captures of each prompt and two capsules. Only the entries that ZCIE added are removed.
fn remove_folder_setups(home: &Path, settings: &Settings) -> anyhow::Result<()> {
    let admin = std::fs::read_to_string(home.join("config/admin.key")).unwrap_or_default();
    let Ok(projects) = Client::new(&settings.url(), admin.trim(), "setup").list_projects() else { return Ok(()) };
    for root in projects.projects.iter().flat_map(|p| p.roots.iter()) {
        let root = PathBuf::from(root);
        if root.join(".claude/zcie-setup.json").exists() {
            println!("{} {}", t(&settings.language, "Folder setup replaced by the setup for all projects:", "Klasör kurulumu, tüm projeler kurulumuyla değiştirildi:"), root.display());
            claude::setup::remove(Scope::Project, Some(root), home)?;
        }
    }
    Ok(())
}

/// Installs or removes the global OpenCode plugin.
fn open_code(home: &Path, old: Option<&Settings>, new: &Settings) -> anyhow::Result<()> {
    if new.opencode {
        return opencode::run(opencode::OpenCodeAction::Setup { dir: None, global: true, name: None }, home, &new.url());
    }
    if old.is_some_and(|o| o.opencode) {
        return opencode::run(opencode::OpenCodeAction::Remove { dir: None, global: true }, home, &new.url());
    }
    Ok(())
}

/// Removes what the settings installed: the Claude Code and OpenCode entries and the autostart
/// agent. The daemon stops. The data stays unless the caller removes `home`.
pub fn remove_all(home: &Path, settings: &Settings) -> anyhow::Result<()> {
    if settings.claude != "none" {
        let args = claude_args(settings);
        claude::setup::remove(args.scope, args.dir, home)?;
    }
    if settings.opencode {
        opencode::run(opencode::OpenCodeAction::Remove { dir: None, global: true }, home, &settings.url())?;
    }
    if cfg!(target_os = "macos") {
        service::uninstall(home)?;
    }
    if daemon::is_running(home) {
        daemon::stop(home)?;
    }
    Ok(())
}
