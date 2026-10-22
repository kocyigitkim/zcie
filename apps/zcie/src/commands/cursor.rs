//! `zcie cursor setup|remove`: connect Cursor to ZCIE. It adds the dispatcher hook
//! (`adapters/cursor/cursor-hook.sh`) to `hooks.json` and the `zcie-mcp` server to `mcp.json`.

use std::path::{Path, PathBuf};

use anyhow::Context;
use clap::Subcommand;
use serde_json::{Value, json};

use super::claude::hooks;
use super::claude::settings;
use super::claude::setup::{ensure_key, register};

/// The Cursor events that the dispatcher hook answers.
const EVENTS: [&str; 5] = [
    "sessionStart",
    "beforeSubmitPrompt",
    "afterFileEdit",
    "afterAgentResponse",
    "sessionEnd",
];

/// Cursor actions.
#[derive(Debug, Subcommand)]
pub enum CursorAction {
    /// Install the hook and the MCP server for a project (default) or for all projects (`--global`).
    Setup {
        /// Project folder. Default: the current folder.
        #[arg(long)]
        dir: Option<PathBuf>,
        /// Install in ~/.cursor for all projects.
        #[arg(long)]
        global: bool,
        /// Project name. Default: the folder name.
        #[arg(long)]
        name: Option<String>,
    },
    /// Remove the hook and the MCP server entry.
    Remove {
        /// Project folder. Default: the current folder.
        #[arg(long)]
        dir: Option<PathBuf>,
        /// Remove from ~/.cursor.
        #[arg(long)]
        global: bool,
    },
}

/// Runs one Cursor action.
pub fn run(action: CursorAction, home: &Path, url: &str) -> anyhow::Result<()> {
    match action {
        CursorAction::Setup { dir, global, name } => setup(home, url, dir, global, name),
        CursorAction::Remove { dir, global } => remove(home, dir, global),
    }
}

/// Returns the project folder and the Cursor folder (`<project>/.cursor` or `~/.cursor`).
fn folders(dir: Option<PathBuf>, global: bool) -> anyhow::Result<(PathBuf, PathBuf)> {
    let project = match dir {
        Some(dir) => dir,
        None => std::env::current_dir()?,
    };
    let project = project
        .canonicalize()
        .with_context(|| format!("{} does not exist", project.display()))?;
    let config = if global {
        PathBuf::from(crate::commands::user_home().context("HOME is not set")?).join(".cursor")
    } else {
        project.join(".cursor")
    };
    Ok((project, config))
}

fn setup(
    home: &Path,
    url: &str,
    dir: Option<PathBuf>,
    global: bool,
    name: Option<String>,
) -> anyhow::Result<()> {
    let (project, config) = folders(dir, global)?;
    let key_file = home.join("config").join("claude-code.key");
    let name = name.unwrap_or_else(|| {
        project
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "default".into())
    });
    ensure_key(home, &key_file, &name)?;
    hooks::install(home)?;
    let command = hooks::command(&home.join("hooks").join("cursor-hook.sh"), "");
    let hooks_file = config.join("hooks.json");
    let mut root = settings::read(&hooks_file)?;
    add_hooks(&mut root, &command);
    settings::write(&hooks_file, &root)?;
    // A global server has no project of its own: Cursor fills in the open workspace.
    let project_dir = if global {
        "${workspaceFolder}".to_string()
    } else {
        project.display().to_string()
    };
    let mcp_file = config.join("mcp.json");
    let mut root = settings::read(&mcp_file)?;
    let servers = root
        .as_object_mut()
        .context("mcp.json is not an object")?
        .entry("mcpServers")
        .or_insert_with(|| json!({}));
    servers
        .as_object_mut()
        .context("mcpServers is not an object")?
        .insert("zcie".into(), mcp_entry(&key_file, url, &project_dir)?);
    settings::write(&mcp_file, &root)?;
    println!("ZCIE setup for Cursor");
    println!("  hooks         : {}", hooks_file.display());
    println!("  MCP server    : {} (mcpServers.zcie)", mcp_file.display());
    if !global {
        let result = register(url, &key_file, &name, &project).unwrap_or_default();
        println!("  project       : {} {result}", project.display());
    }
    println!("Restart Cursor or open a new chat to load the hooks and the MCP server.");
    Ok(())
}

/// Adds the dispatcher hook to each event. Other hooks stay; a second call adds nothing.
fn add_hooks(config: &mut Value, command: &str) {
    if config.get("version").is_none() {
        config["version"] = json!(1);
    }
    if !config["hooks"].is_object() {
        config["hooks"] = json!({});
    }
    for event in EVENTS {
        let mut list = config["hooks"][event]
            .as_array()
            .cloned()
            .unwrap_or_default();
        if !list.iter().any(|h| h["command"] == command) {
            list.push(json!({ "command": command }));
        }
        config["hooks"][event] = Value::Array(list);
    }
}

/// Removes the dispatcher hook from every event. Other hooks stay.
fn remove_hooks(config: &mut Value, command: &str) {
    if let Some(events) = config.get_mut("hooks").and_then(Value::as_object_mut) {
        for list in events.values_mut().filter_map(Value::as_array_mut) {
            list.retain(|h| h["command"] != command);
        }
    }
}

fn mcp_entry(key_file: &Path, url: &str, project_dir: &str) -> anyhow::Result<Value> {
    let exe = std::env::current_exe()?;
    let mcp = exe.parent().context("no binary folder")?.join(format!("zcie-mcp{}", std::env::consts::EXE_SUFFIX));
    Ok(json!({
        "command": mcp.display().to_string(),
        "args": [],
        "env": {
            "ZCIE_URL": url,
            "ZCIE_API_KEY_FILE": key_file.display().to_string(),
            "ZCIE_PROJECT_DIR": project_dir
        }
    }))
}

fn remove(home: &Path, dir: Option<PathBuf>, global: bool) -> anyhow::Result<()> {
    let (_, config) = folders(dir, global)?;
    let command = hooks::command(&home.join("hooks").join("cursor-hook.sh"), "");
    let hooks_file = config.join("hooks.json");
    if hooks_file.exists() {
        let mut root = settings::read(&hooks_file)?;
        remove_hooks(&mut root, &command);
        settings::write(&hooks_file, &root)?;
        println!("removed the ZCIE hook from {}", hooks_file.display());
    }
    let mcp_file = config.join("mcp.json");
    if mcp_file.exists() {
        let mut root = settings::read(&mcp_file)?;
        if let Some(servers) = root.get_mut("mcpServers").and_then(Value::as_object_mut) {
            servers.remove("zcie");
        }
        settings::write(&mcp_file, &root)?;
        println!("removed mcpServers.zcie from {}", mcp_file.display());
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn ts_25_01_hooks_are_added_once_and_removed_alone() {
        let foreign = json!({"command": "/usr/bin/other-hook"});
        let mut config = json!({"hooks": {"beforeSubmitPrompt": [foreign.clone()]}});
        add_hooks(&mut config, "/home/z/hooks/cursor-hook.sh");
        add_hooks(&mut config, "/home/z/hooks/cursor-hook.sh");
        assert_eq!(config["version"], json!(1));
        for event in EVENTS {
            let list = config["hooks"][event].as_array().unwrap();
            let zcie = list.iter().filter(|h| h["command"] == "/home/z/hooks/cursor-hook.sh").count();
            assert_eq!(zcie, 1, "{event}: {config}");
        }
        assert!(config["hooks"]["beforeSubmitPrompt"].as_array().unwrap().contains(&foreign), "{config}");
        remove_hooks(&mut config, "/home/z/hooks/cursor-hook.sh");
        assert_eq!(config["hooks"]["beforeSubmitPrompt"], json!([foreign]), "{config}");
        assert_eq!(config["hooks"]["sessionEnd"], json!([]), "{config}");
    }

    #[test]
    fn ts_25_02_mcp_entry_starts_zcie_mcp_with_three_env_keys() {
        let entry = mcp_entry(Path::new("/k/claude-code.key"), "http://127.0.0.1:1", "${workspaceFolder}").unwrap();
        assert!(entry["command"].as_str().unwrap().trim_end_matches(".exe").ends_with("zcie-mcp"), "{entry}");
        assert_eq!(entry["args"], json!([]));
        let env = entry["env"].as_object().unwrap();
        assert_eq!(env.len(), 3, "{entry}");
        assert_eq!(env["ZCIE_URL"], "http://127.0.0.1:1");
        assert_eq!(env["ZCIE_API_KEY_FILE"], "/k/claude-code.key");
        assert_eq!(env["ZCIE_PROJECT_DIR"], "${workspaceFolder}");
    }
}
