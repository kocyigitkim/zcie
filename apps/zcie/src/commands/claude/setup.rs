//! The setup, status and remove procedures.

use std::path::{Path, PathBuf};

use anyhow::Context;
use zcie_sdk::Client;
use zcie_security::{ClientEntry, ClientsFile, Permission, generate_key, hash_key};

use super::settings::{self, Manifest};
use super::{Scope, SetupArgs, hooks};

/// The files of one setup.
struct Target {
    project: PathBuf,
    settings_file: PathBuf,
    manifest_file: PathBuf,
}

fn target(scope: Scope, dir: Option<PathBuf>, home: &Path) -> anyhow::Result<Target> {
    let project = match dir {
        Some(dir) => dir,
        None => std::env::current_dir()?,
    };
    let project = project.canonicalize().with_context(|| format!("{} does not exist", project.display()))?;
    let (settings_file, manifest_file) = match scope {
        Scope::Project => (
            project.join(".claude").join("settings.local.json"),
            project.join(".claude").join("zcie-setup.json"),
        ),
        Scope::User => {
            let user_home = crate::commands::user_home().context("HOME is not set")?;
            (PathBuf::from(user_home).join(".claude").join("settings.json"), home.join("claude-user-setup.json"))
        }
    };
    Ok(Target { project, settings_file, manifest_file })
}

fn load_manifest(path: &Path) -> Manifest {
    std::fs::read_to_string(path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

/// Runs `zcie claude setup`.
pub fn setup(args: &SetupArgs, home: &Path, url: &str) -> anyhow::Result<()> {
    let target = target(args.scope, args.dir.clone(), home)?;
    let workspace = args.for_workspace.clone().unwrap_or_else(|| folder_name(&target.project));
    let hook_dir = home.join("hooks");
    let key_file = home.join("config").join("claude-code.key");
    let mut config = settings::read(&target.settings_file)?;
    let mut manifest = load_manifest(&target.manifest_file);
    manifest.settings_file = target.settings_file.display().to_string();
    apply(&mut config, &mut manifest, args, &hook_dir, &key_file, url);
    if args.dry_run {
        println!("{}", serde_json::to_string_pretty(&config)?);
        println!("(dry run: nothing was written)");
        return Ok(());
    }
    hooks::install(home)?;
    ensure_key(home, &key_file, &workspace)?;
    settings::write(&target.settings_file, &config)?;
    std::fs::write(&target.manifest_file, serde_json::to_string_pretty(&manifest)?)?;
    let registered = if args.no_register || args.scope == Scope::User { None } else { register(url, &key_file, &workspace, &target.project) };
    report(args, &target, &workspace, &manifest, registered);
    Ok(())
}

fn apply(config: &mut serde_json::Value, manifest: &mut Manifest, args: &SetupArgs, hook_dir: &Path, key_file: &Path, url: &str) {
    let env = env_values(args, &manifest.settings_file, key_file, url);
    settings::remove(config, &Manifest { env_keys: manifest.env_keys.clone(), ..Manifest::default() });
    manifest.env_keys.clear();
    settings::add_env(config, &env, manifest);
    if !args.no_lean {
        let allow: Vec<String> = args.allow.split(',').map(str::trim).filter(|s| !s.is_empty()).map(str::to_string).collect();
        let tools: &[&str] = match args.profile {
            super::Profile::Auto => &settings::AUTO_DENY,
            super::Profile::Balanced => &settings::BALANCED_DENY,
            super::Profile::Lean => &settings::LEAN_DENY,
        };
        settings::drop_old_deny(config, tools, manifest);
        settings::add_deny(config, tools, &allow, manifest);
    }
    settings::remove(config, &Manifest { hooks_added: manifest.hooks_added.clone(), ..Manifest::default() });
    manifest.hooks_added.clear();
    for spec in hooks::profile(!args.no_capture, !args.no_inject, !args.no_guards, !args.no_lean, !args.no_check) {
        let command = hooks::command(&hook_dir.join(spec.script), "");
        settings::add_hook(config, (spec.event, spec.matcher), &command, spec.timeout, manifest);
    }
}

/// The env values of the setup.
fn env_values(args: &SetupArgs, settings_file: &str, key_file: &Path, url: &str) -> Vec<(&'static str, String)> {
    let mut env = vec![
        ("ZCIE_API_KEY_FILE", key_file.display().to_string()),
        ("ZCIE_SETTINGS_FILE", settings_file.to_string()),
    ];
    if url != zcie_sdk::DEFAULT_ENDPOINT {
        env.push(("ZCIE_URL", url.to_string()));
    }
    if let Some(budget) = args.budget {
        env.push(("ZCIE_INJECT_BUDGET", budget.to_string()));
    }
    if args.no_cross_project {
        env.push(("ZCIE_CROSS_PROJECT", "false".to_string()));
    }
    if args.cache_5m {
        env.push(("FORCE_PROMPT_CACHING_5M", "1".to_string()));
    }
    if args.profile == super::Profile::Auto && !args.no_lean {
        env.push(("ENABLE_TOOL_SEARCH", "true".to_string()));
    }
    env
}

fn folder_name(path: &Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "default".into())
}

/// Makes the Claude Code API key file if it does not exist (read and write, all workspaces).
/// An existing key gets access to `workspace` if it does not have it.
pub(crate) fn ensure_key(home: &Path, key_file: &Path, workspace: &str) -> anyhow::Result<()> {
    if let Ok(existing) = std::fs::read_to_string(key_file)
        && !existing.trim().is_empty()
    {
        return grant_workspace(home, existing.trim(), workspace);
    }
    let clients_path = home.join("config").join("clients.toml");
    let mut clients = if clients_path.exists() { ClientsFile::load(&clients_path)? } else { ClientsFile::default() };
    let mut name = "claude-code".to_string();
    let mut index = 2;
    while clients.clients.iter().any(|c| c.name == name) {
        name = format!("claude-code-{index}");
        index += 1;
    }
    let key = generate_key()?;
    clients.add(ClientEntry {
        name,
        key_hash: hash_key(&key),
        principal_id: "user-001".into(),
        workspaces: vec!["*".into()],
        permissions: vec![Permission::Read, Permission::Write],
    });
    clients.save(&clients_path)?;
    std::fs::write(key_file, format!("{key}\n"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(key_file, std::fs::Permissions::from_mode(0o600))?;
    }
    reload_daemon(home);
    Ok(())
}

/// Adds `workspace` to the client entry of `key` if the entry does not allow it.
fn grant_workspace(home: &Path, key: &str, workspace: &str) -> anyhow::Result<()> {
    let clients_path = home.join("config").join("clients.toml");
    let mut clients = ClientsFile::load(&clients_path)?;
    let hash = hash_key(key);
    let Some(entry) = clients.clients.iter_mut().find(|c| c.key_hash == hash) else { return Ok(()) };
    if entry.workspaces.iter().any(|w| w == "*" || w == workspace) {
        return Ok(());
    }
    entry.workspaces.push(workspace.to_string());
    clients.save(&clients_path)?;
    reload_daemon(home);
    Ok(())
}

/// Sends SIGHUP to a running daemon so it loads the new key. On Windows the daemon sees the
/// change of `clients.toml` by itself (ADR-027); this waits for that.
#[cfg(not(unix))]
fn reload_daemon(_home: &Path) {
    std::thread::sleep(std::time::Duration::from_millis(1200));
}

/// Sends SIGHUP to a running daemon so it loads the new key.
#[cfg(unix)]
fn reload_daemon(home: &Path) {
    let Ok(pid) = std::fs::read_to_string(home.join("run").join("zcied.pid")) else { return };
    let pid = pid.trim();
    if !pid.is_empty() && pid.chars().all(|c| c.is_ascii_digit()) {
        let _ = std::process::Command::new("kill").arg("-HUP").arg(pid).status();
        std::thread::sleep(std::time::Duration::from_millis(300));
    }
}

/// Makes or connects the project of the folder and writes `.claude/zcie_id` (ADR-009).
/// Returns a short result text.
pub(crate) fn register(url: &str, key_file: &Path, name: &str, project: &Path) -> Option<String> {
    let key = std::fs::read_to_string(key_file).ok()?.trim().to_string();
    let client = Client::new(url, &key, name);
    let existing = crate::commands::project::find_project_id(project).map(|(_, id)| id);
    let request = zcie_types::api::EnsureProjectRequest {
        project_id: existing,
        name: Some(name.to_string()),
        root: Some(project.display().to_string()),
        index: false,
    };
    match client.ensure_project(&request) {
        Ok(info) => match crate::commands::project::write_project_id(project, &info.project_id) {
            Ok(()) => Some(format!("project {} ({}), id file .claude/zcie_id", info.name, info.project_id)),
            Err(error) => Some(format!("project {} made, but the id file was not written: {error}", info.project_id)),
        },
        Err(error) => Some(format!("not registered: {error} (the first Claude session makes it; or run: zcie daemon start)")),
    }
}

fn report(args: &SetupArgs, target: &Target, workspace: &str, manifest: &Manifest, registered: Option<String>) {
    println!("ZCIE setup for Claude Code");
    println!("  settings file : {}", target.settings_file.display());
    println!("  project name  : {workspace}");
    println!("  project       : {} {}", target.project.display(), registered.unwrap_or_default());
    let profile = match args.profile {
        super::Profile::Auto => format!("auto: tools load on demand, {} heavy tools denied (about 27k -> 7k tokens per turn)", settings::AUTO_DENY.len()),
        super::Profile::Balanced => format!("balanced: {} heavy unused tools denied (about 27k -> 14k tokens per turn)", settings::BALANCED_DENY.len()),
        super::Profile::Lean => format!("lean: {} tools denied (about 27k -> 6k tokens per turn)", settings::LEAN_DENY.len()),
    };
    println!("  tool profile  : {}", if args.no_lean { "off".to_string() } else { profile });
    println!("  hooks         : {} entries", manifest.hooks_added.len());
    let scope = if args.scope == Scope::User { "user" } else { "project" };
    println!("  undo          : zcie claude remove --scope {scope}");
    println!("Start a new Claude Code session to load the settings.");
}

/// Runs `zcie claude status`.
pub fn status(scope: Scope, dir: Option<PathBuf>, home: &Path) -> anyhow::Result<()> {
    let target = target(scope, dir, home)?;
    if !target.manifest_file.exists() {
        println!("ZCIE is not set up for this scope ({}). Run: zcie claude setup", target.settings_file.display());
        return Ok(());
    }
    let manifest = load_manifest(&target.manifest_file);
    println!("settings file : {}", manifest.settings_file);
    println!("env           : {}", manifest.env_keys.join(", "));
    println!("denied tools  : {}", manifest.deny_added.len());
    for hook in &manifest.hooks_added {
        println!("hook          : {}", hook.replace('|', "  "));
    }
    Ok(())
}

/// Runs `zcie claude tools enable|disable`: removes tools from, or adds them to, the deny list.
pub fn tools(scope: Scope, dir: Option<PathBuf>, home: &Path, enable: bool, names: &[String]) -> anyhow::Result<()> {
    let target = target(scope, dir, home)?;
    let mut config = settings::read(&target.settings_file)?;
    let mut manifest = load_manifest(&target.manifest_file);
    let list = settings::deny_list(&mut config);
    for name in names {
        list.retain(|v| v != name.as_str());
        if !enable {
            list.push(serde_json::Value::String(name.clone()));
        }
    }
    if !enable {
        manifest.deny_added.extend(names.iter().filter(|n| !manifest.deny_added.contains(n)).cloned().collect::<Vec<_>>());
    }
    settings::write(&target.settings_file, &config)?;
    if target.manifest_file.exists() {
        std::fs::write(&target.manifest_file, serde_json::to_string_pretty(&manifest)?)?;
    }
    println!("{} {} in {}", if enable { "Enabled" } else { "Disabled" }, names.join(", "), target.settings_file.display());
    Ok(())
}

/// Runs `zcie claude remove`.
pub fn remove(scope: Scope, dir: Option<PathBuf>, home: &Path) -> anyhow::Result<()> {
    let target = target(scope, dir, home)?;
    if !target.manifest_file.exists() {
        println!("Nothing to remove: no ZCIE setup for {}", target.settings_file.display());
        return Ok(());
    }
    let manifest = load_manifest(&target.manifest_file);
    let mut config = settings::read(&target.settings_file)?;
    settings::remove(&mut config, &manifest);
    settings::write(&target.settings_file, &config)?;
    std::fs::remove_file(&target.manifest_file)?;
    println!("Removed the ZCIE entries from {}", target.settings_file.display());
    Ok(())
}
