//! `zcie opencode setup|remove`: connect OpenCode to ZCIE. It installs the plugin
//! (`adapters/opencode/zcie.js`) and adds the `zcie-mcp` server to `opencode.json`.

use std::path::{Path, PathBuf};

use anyhow::Context;
use clap::Subcommand;
use serde_json::{Value, json};

use super::claude::setup::{ensure_key, register};

/// The plugin, embedded in the binary.
const PLUGIN: &str = include_str!("../../../../adapters/opencode/zcie.js");

/// OpenCode actions.
#[derive(Debug, Subcommand)]
pub enum OpenCodeAction {
    /// Install the plugin and the MCP server for a project (default) or for all projects (`--global`).
    Setup {
        /// Project folder. Default: the current folder.
        #[arg(long)]
        dir: Option<PathBuf>,
        /// Install in ~/.config/opencode for all projects.
        #[arg(long)]
        global: bool,
        /// Project name. Default: the folder name.
        #[arg(long)]
        name: Option<String>,
    },
    /// Remove the plugin and the MCP server entry.
    Remove {
        /// Project folder. Default: the current folder.
        #[arg(long)]
        dir: Option<PathBuf>,
        /// Remove from ~/.config/opencode.
        #[arg(long)]
        global: bool,
    },
}

/// Runs one OpenCode action.
pub fn run(action: OpenCodeAction, home: &Path, url: &str) -> anyhow::Result<()> {
    match action {
        OpenCodeAction::Setup { dir, global, name } => setup(home, url, dir, global, name),
        OpenCodeAction::Remove { dir, global } => remove(dir, global),
    }
}

/// Returns the project folder and the OpenCode config folder (`<project>/.opencode` or `~/.config/opencode`).
fn folders(dir: Option<PathBuf>, global: bool) -> anyhow::Result<(PathBuf, PathBuf)> {
    let project = match dir {
        Some(dir) => dir,
        None => std::env::current_dir()?,
    };
    let project = project.canonicalize().with_context(|| format!("{} does not exist", project.display()))?;
    let config = if global {
        PathBuf::from(std::env::var("HOME").context("HOME is not set")?).join(".config/opencode")
    } else {
        project.join(".opencode")
    };
    Ok((project, config))
}

/// `opencode.json` of a project is in the project folder; the global one is in the config folder.
fn config_file(project: &Path, config: &Path, global: bool) -> PathBuf {
    if global { config.join("opencode.json") } else { project.join("opencode.json") }
}

fn setup(home: &Path, url: &str, dir: Option<PathBuf>, global: bool, name: Option<String>) -> anyhow::Result<()> {
    let (project, config) = folders(dir, global)?;
    let key_file = home.join("config").join("claude-code.key");
    let name = name.unwrap_or_else(|| project.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "default".into()));
    ensure_key(home, &key_file, &name)?;
    let plugin = config.join("plugins").join("zcie.js");
    std::fs::create_dir_all(plugin.parent().context("no plugin folder")?)?;
    std::fs::write(&plugin, PLUGIN)?;
    let file = config_file(&project, &config, global);
    let mcp = mcp_entry(&key_file, url)?;
    let written = edit_config(&file, |root| {
        root.as_object_mut().map(|o| o.entry("mcp").or_insert_with(|| json!({})).as_object_mut().map(|m| m.insert("zcie".into(), mcp.clone())));
    })?;
    println!("ZCIE setup for OpenCode");
    println!("  plugin        : {}", plugin.display());
    match written {
        true => println!("  MCP server    : {} (mcp.zcie)", file.display()),
        false => println!("  MCP server    : {} is not plain JSON; add this under \"mcp\": \"zcie\": {mcp}", file.display()),
    }
    if !global {
        let result = register(url, &key_file, &name, &project).unwrap_or_default();
        println!("  project       : {} {result}", project.display());
    }
    println!("Start a new OpenCode session to load the plugin.");
    Ok(())
}

fn mcp_entry(key_file: &Path, url: &str) -> anyhow::Result<Value> {
    let exe = std::env::current_exe()?;
    let mcp = exe.parent().context("no binary folder")?.join("zcie-mcp");
    Ok(json!({
        "type": "local",
        "command": [mcp.display().to_string()],
        "environment": { "ZCIE_URL": url, "ZCIE_API_KEY_FILE": key_file.display().to_string() },
        "enabled": true
    }))
}

/// Reads a JSON config (or `{}`), changes it and writes it back. Returns false and changes
/// nothing if the file is not plain JSON (for example JSON with comments).
fn edit_config(file: &Path, change: impl FnOnce(&mut Value)) -> anyhow::Result<bool> {
    let mut root = match std::fs::read_to_string(file) {
        Ok(text) => match serde_json::from_str::<Value>(&text) {
            Ok(value) if value.is_object() => value,
            _ => return Ok(false),
        },
        Err(_) => json!({ "$schema": "https://opencode.ai/config.json" }),
    };
    change(&mut root);
    std::fs::write(file, serde_json::to_string_pretty(&root)? + "\n")?;
    Ok(true)
}

fn remove(dir: Option<PathBuf>, global: bool) -> anyhow::Result<()> {
    let (project, config) = folders(dir, global)?;
    let plugin = config.join("plugins").join("zcie.js");
    if plugin.exists() {
        std::fs::remove_file(&plugin)?;
        println!("removed {}", plugin.display());
    }
    let file = config_file(&project, &config, global);
    if file.exists() && edit_config(&file, |root| {
        root.get_mut("mcp").and_then(Value::as_object_mut).map(|m| m.remove("zcie"));
    })? {
        println!("removed mcp.zcie from {}", file.display());
    }
    Ok(())
}
