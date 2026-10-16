//! Non-destructive merge of ZCIE entries into a Claude Code settings file, and their removal.

use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

/// Tools that the auto profile denies: the heavy tools that Claude Code cannot defer (Skill and
/// Agent also add long listings to every request). All other tools stay available by name, and
/// the model loads them itself through ToolSearch.
pub const AUTO_DENY: [&str; 6] = ["Skill", "Agent", "Workflow", "ScheduleWakeup", "ReportFindings", "ListAgents"];

/// Built-in Claude Code tools that the balanced profile denies: heavy definitions that ordinary
/// coding work does not use. Web, subagent, skill, LSP and notebook tools stay loaded.
pub const BALANCED_DENY: [&str; 15] = [
    "CronCreate", "CronDelete", "CronList", "DesignSync", "EnterWorktree", "ExitWorktree", "ListAgents", "Monitor",
    "PushNotification", "RemoteTrigger", "ReportFindings", "ScheduleWakeup", "SendMessage", "TaskStop", "Workflow",
];

/// Built-in Claude Code tools that the lean profile denies (measured: about 60 % of the
/// context of each turn, see benchmarks/results/claude-code-ab-2026-10-16.md).
pub const LEAN_DENY: [&str; 21] = [
    "CronCreate", "CronDelete", "CronList", "DesignSync", "EnterWorktree", "ExitWorktree", "LSP", "ListAgents",
    "Monitor", "NotebookEdit", "PushNotification", "RemoteTrigger", "ReportFindings", "ScheduleWakeup", "SendMessage",
    "Task", "TaskStop", "WebFetch", "WebSearch", "Workflow", "Skill",
];

/// What ZCIE added to a settings file. `remove` takes away exactly these entries.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Manifest {
    /// The settings file.
    pub settings_file: String,
    /// Environment keys that ZCIE set.
    pub env_keys: Vec<String>,
    /// Deny rules that ZCIE added.
    pub deny_added: Vec<String>,
    /// Hook commands that ZCIE added, as `event|command`.
    pub hooks_added: Vec<String>,
}

/// Reads a settings file. A missing file is an empty object.
pub fn read(path: &Path) -> anyhow::Result<Value> {
    if !path.exists() {
        return Ok(json!({}));
    }
    let text = std::fs::read_to_string(path)?;
    if text.trim().is_empty() {
        return Ok(json!({}));
    }
    let value: Value = serde_json::from_str(&text)?;
    if !value.is_object() {
        anyhow::bail!("{} is not a JSON object", path.display());
    }
    Ok(value)
}

/// Writes a settings file and keeps one backup of the earlier content.
pub fn write(path: &Path, value: &Value) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if path.exists() {
        std::fs::copy(path, path.with_extension("json.zcie-backup"))?;
    }
    std::fs::write(path, serde_json::to_string_pretty(value)? + "\n")?;
    Ok(())
}

fn object<'a>(value: &'a mut Value, key: &str) -> Option<&'a mut Map<String, Value>> {
    let root = value.as_object_mut()?;
    if !root.get(key).is_some_and(Value::is_object) {
        root.insert(key.to_string(), json!({}));
    }
    root.get_mut(key).and_then(Value::as_object_mut)
}

/// Sets environment variables. Records the keys.
pub fn add_env(settings: &mut Value, values: &[(&str, String)], manifest: &mut Manifest) {
    let Some(env) = object(settings, "env") else { return };
    for (key, value) in values {
        env.insert((*key).to_string(), Value::String(value.clone()));
        if !manifest.env_keys.iter().any(|k| k == key) {
            manifest.env_keys.push((*key).to_string());
        }
    }
}

/// Removes deny rules that ZCIE added earlier and that `keep` does not contain (profile change).
pub fn drop_old_deny(settings: &mut Value, keep: &[&str], manifest: &mut Manifest) {
    let old: Vec<String> = manifest.deny_added.iter().filter(|t| !keep.contains(&t.as_str())).cloned().collect();
    if let Some(deny) = settings.pointer_mut("/permissions/deny").and_then(Value::as_array_mut) {
        deny.retain(|v| !old.iter().any(|o| v == o));
    }
    manifest.deny_added.retain(|t| keep.contains(&t.as_str()));
}

/// Adds deny rules of `tools` that are not present. `allow` removes tools from the list.
pub fn add_deny(settings: &mut Value, tools: &[&str], allow: &[String], manifest: &mut Manifest) {
    let Some(permissions) = object(settings, "permissions") else { return };
    let deny = permissions.entry("deny").or_insert_with(|| json!([]));
    let Some(list) = deny.as_array_mut() else { return };
    for tool in tools.iter().filter(|t| !allow.iter().any(|a| a == *t)) {
        if !list.iter().any(|v| v == *tool) {
            list.push(Value::String((*tool).to_string()));
            manifest.deny_added.push((*tool).to_string());
        }
    }
}

/// Returns the deny list of the settings, made if missing.
pub fn deny_list(settings: &mut Value) -> &mut Vec<Value> {
    if !settings.pointer("/permissions/deny").is_some_and(Value::is_array) {
        if !settings.get("permissions").is_some_and(Value::is_object) {
            settings["permissions"] = json!({});
        }
        settings["permissions"]["deny"] = json!([]);
    }
    match settings.pointer_mut("/permissions/deny").and_then(Value::as_array_mut) {
        Some(list) => list,
        None => unreachable!("the deny list was just set"),
    }
}

/// Adds one hook command to an event if the same command is not present.
pub fn add_hook(settings: &mut Value, event: &str, matcher: Option<&str>, command: &str, manifest: &mut Manifest) {
    let Some(hooks) = object(settings, "hooks") else { return };
    let groups = hooks.entry(event.to_string()).or_insert_with(|| json!([]));
    let Some(groups) = groups.as_array_mut() else { return };
    let present = groups.iter().any(|g| {
        g["hooks"].as_array().is_some_and(|hs| hs.iter().any(|h| h["command"] == command))
    });
    if present {
        return;
    }
    let mut group = json!({ "hooks": [ { "type": "command", "command": command, "timeout": 5 } ] });
    if let Some(matcher) = matcher {
        group["matcher"] = Value::String(matcher.to_string());
    }
    groups.push(group);
    manifest.hooks_added.push(format!("{event}|{command}"));
}

/// Removes the entries that the manifest lists.
pub fn remove(settings: &mut Value, manifest: &Manifest) {
    if let Some(env) = settings.get_mut("env").and_then(Value::as_object_mut) {
        for key in &manifest.env_keys {
            env.remove(key);
        }
    }
    if let Some(deny) = settings.pointer_mut("/permissions/deny").and_then(Value::as_array_mut) {
        deny.retain(|v| !manifest.deny_added.iter().any(|d| v == d));
    }
    for entry in &manifest.hooks_added {
        let Some((event, command)) = entry.split_once('|') else { continue };
        if let Some(groups) = settings.pointer_mut(&format!("/hooks/{event}")).and_then(Value::as_array_mut) {
            groups.retain(|g| !g["hooks"].as_array().is_some_and(|hs| hs.iter().any(|h| h["command"] == command)));
        }
    }
}
