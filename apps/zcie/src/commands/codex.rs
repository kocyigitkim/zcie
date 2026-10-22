//! `zcie codex setup|remove`: connect Codex CLI to ZCIE (ADR-026). Codex hooks take Claude Code input,
//! so `~/.codex/hooks.json` runs the Claude Code hook scripts through `adapters/codex/codex-hook.sh`.
//! The `zcie-mcp` server is added with `codex mcp add`.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::Context;
use clap::Subcommand;
use serde_json::{Value, json};

use super::claude::hooks;
use super::claude::settings;
use super::claude::setup::ensure_key;

/// The Codex hooks: event and Claude Code script.
const HOOKS: [(&str, &str); 6] = [
    ("SessionStart", "session-start.sh"),
    ("SessionStart", "session-map.sh"),
    ("UserPromptSubmit", "user-prompt-submit.sh"),
    ("UserPromptSubmit", "inject-context.sh"),
    ("Stop", "assistant-message.sh"),
    ("SessionEnd", "stop.sh"),
];

/// Codex actions.
#[derive(Debug, Subcommand)]
pub enum CodexAction {
    /// Install the hooks in ~/.codex/hooks.json and the MCP server for all projects.
    Setup,
    /// Remove the hooks and the MCP server entry.
    Remove,
}

/// Runs one Codex action.
pub fn run(action: CodexAction, home: &Path, url: &str) -> anyhow::Result<()> {
    let hooks_file = PathBuf::from(std::env::var("HOME").context("HOME is not set")?).join(".codex/hooks.json");
    let wrapper = home.join("hooks").join("codex-hook.sh").display().to_string();
    let mut root = settings::read(&hooks_file)?;
    remove_hooks(&mut root, &wrapper);
    match action {
        CodexAction::Setup => {
            let key_file = home.join("config").join("claude-code.key");
            ensure_key(home, &key_file, "codex")?;
            hooks::install(home)?;
            add_hooks(&mut root, &wrapper);
            settings::write(&hooks_file, &root)?;
            let mcp = std::env::current_exe()?.parent().context("no binary folder")?.join("zcie-mcp");
            // A second add replaces the entry, so setup can run again.
            let added = Command::new("codex")
                .args(["mcp", "add", "zcie", "--env", &format!("ZCIE_URL={url}")])
                .args(["--env", &format!("ZCIE_API_KEY_FILE={}", key_file.display()), "--"])
                .arg(&mcp)
                .output();
            println!("ZCIE setup for Codex");
            println!("  hooks         : {}", hooks_file.display());
            match added {
                Ok(out) if out.status.success() => println!("  MCP server    : zcie (codex mcp add)"),
                _ => println!("  MCP server    : not added; run: codex mcp add zcie -- {}", mcp.display()),
            }
            println!("Start Codex and trust the new hooks when it asks (/hooks).");
        }
        CodexAction::Remove => {
            if hooks_file.exists() {
                settings::write(&hooks_file, &root)?;
                println!("removed the ZCIE hooks from {}", hooks_file.display());
            }
            if Command::new("codex").args(["mcp", "remove", "zcie"]).output().is_ok_and(|o| o.status.success()) {
                println!("removed the zcie MCP server from Codex");
            }
        }
    }
    Ok(())
}

/// The hook command of one script.
fn command(wrapper: &str, script: &str) -> String {
    format!("{wrapper} {script}")
}

/// Adds one matcher group per ZCIE hook. Call `remove_hooks` first, so a second setup adds nothing.
fn add_hooks(config: &mut Value, wrapper: &str) {
    if !config["hooks"].is_object() {
        config["hooks"] = json!({});
    }
    for (event, script) in HOOKS {
        let timeout = if script == "inject-context.sh" || script == "session-map.sh" { 10 } else { 5 };
        let group = json!({"hooks": [{"type": "command", "command": command(wrapper, script), "timeout": timeout}]});
        match config["hooks"][event].as_array_mut() {
            Some(list) => list.push(group),
            None => config["hooks"][event] = json!([group]),
        }
    }
}

/// Removes the matcher groups that run the wrapper. Other hooks stay.
fn remove_hooks(config: &mut Value, wrapper: &str) {
    let ours = |group: &Value| {
        group["hooks"].as_array().is_some_and(|hooks| hooks.iter().any(|h| h["command"].as_str().is_some_and(|c| c.starts_with(wrapper))))
    };
    if let Some(events) = config.get_mut("hooks").and_then(Value::as_object_mut) {
        for list in events.values_mut().filter_map(Value::as_array_mut) {
            list.retain(|group| !ours(group));
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn ts_26_01_hooks_are_added_once_and_removed_alone() {
        let foreign = json!({"hooks": [{"type": "command", "command": "/usr/bin/other-hook"}]});
        let mut config = json!({"hooks": {"Stop": [foreign.clone()]}});
        for _ in 0..2 {
            remove_hooks(&mut config, "/z/hooks/codex-hook.sh");
            add_hooks(&mut config, "/z/hooks/codex-hook.sh");
        }
        let all = config.to_string();
        for (_, script) in HOOKS {
            assert_eq!(all.matches(&format!("codex-hook.sh {script}")).count(), 1, "{script}: {config}");
        }
        assert!(config["hooks"]["Stop"].as_array().unwrap().contains(&foreign), "{config}");
        remove_hooks(&mut config, "/z/hooks/codex-hook.sh");
        assert_eq!(config["hooks"]["Stop"], json!([foreign]), "{config}");
        assert_eq!(config["hooks"]["SessionStart"], json!([]), "{config}");
    }
}
