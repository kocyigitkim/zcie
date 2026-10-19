//! The settings of an installation (`<home>/config/setup.toml`, ADR-014). `zcie setup` asks for
//! them, `zcie config set` changes them, and `apply` makes the system match them.

use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use serde::{Deserialize, Serialize};

/// Default port of the daemon.
pub const DEFAULT_PORT: u16 = 47831;
/// Default token budget of the context capsule.
pub const DEFAULT_BUDGET: u32 = 1200;
/// Smallest and largest capsule budget.
const BUDGET_RANGE: (u32, u32) = (200, 4000);
/// Allowed numbers of index workers.
pub const WORKER_CHOICES: [usize; 3] = [1, 2, 4];

/// The settings. Each field has a key for `zcie config set`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    /// Language of the wizard and the reports: `tr` or `en`.
    pub language: String,
    /// HTTP port of the daemon (address 127.0.0.1).
    pub port: u16,
    /// Vector search: `multilingual`, `english` or `off`.
    pub vector: String,
    /// Index workers: 1, 2 or 4 (CPU use while a project is indexed).
    pub workers: usize,
    /// Start the daemon at login (macOS launchd).
    pub autostart: bool,
    /// Claude Code integration: `user` (all projects), `project` (one folder) or `none`.
    pub claude: String,
    /// Project folder of `claude = "project"`.
    pub claude_dir: String,
    /// Tool profile of Claude Code: `auto`, `balanced`, `lean` or `off`.
    pub profile: String,
    /// Token budget of the context capsule.
    pub budget: u32,
    /// OpenCode integration for all projects.
    pub opencode: bool,
    /// Store the answers of the assistant too (`capture.assistant_messages`, ADR-016).
    pub answers: bool,
    /// Run the check of the project when a turn changed code (`auto`) or not (`off`) (ADR-020).
    pub check: String,
    /// The context hook also reads projects that a prompt names (ADR-009, ADR-019 D-074).
    pub cross_project: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            language: if std::env::var("LANG").unwrap_or_default().starts_with("tr") { "tr".into() } else { "en".into() },
            port: DEFAULT_PORT,
            vector: "multilingual".into(),
            workers: 2,
            autostart: cfg!(target_os = "macos"),
            claude: "user".into(),
            claude_dir: String::new(),
            profile: "auto".into(),
            budget: DEFAULT_BUDGET,
            opencode: false,
            answers: true,
            check: "auto".into(),
            cross_project: true,
        }
    }
}

/// The keys of `zcie config set`, with the allowed values.
pub const KEYS: [(&str, &str); 13] = [
    ("language", "tr | en"),
    ("port", "1024..65535"),
    ("vector", "multilingual | english | off"),
    ("workers", "1 | 2 | 4"),
    ("autostart", "true | false"),
    ("claude", "user | project | none"),
    ("claude_dir", "folder of claude = project"),
    ("profile", "auto | balanced | lean | off"),
    ("budget", "200..4000"),
    ("opencode", "true | false"),
    ("answers", "true | false"),
    ("check", "auto | off"),
    ("cross_project", "true | false"),
];

/// Returns the file of the settings.
pub fn file(home: &Path) -> PathBuf {
    home.join("config").join("setup.toml")
}

/// Loads the settings, or `None` if `zcie setup` has not run.
pub fn load(home: &Path) -> anyhow::Result<Option<Settings>> {
    let path = file(home);
    if !path.exists() {
        return Ok(None);
    }
    let text = std::fs::read_to_string(&path)?;
    let settings: Settings = toml::from_str(&text).with_context(|| format!("{} is not valid", path.display()))?;
    settings.validate()?;
    Ok(Some(settings))
}

/// Saves the settings.
pub fn save(home: &Path, settings: &Settings) -> anyhow::Result<()> {
    let path = file(home);
    std::fs::create_dir_all(path.parent().context("no config folder")?)?;
    let text = format!("# ZCIE settings. Change them with `zcie config set <key> <value>` or `zcie setup`.\n{}", toml::to_string(settings)?);
    std::fs::write(&path, text)?;
    Ok(())
}

/// Parses a yes/no value (true/false, yes/no, evet/hayır, on/off, 1/0).
pub fn parse_bool(value: &str) -> anyhow::Result<bool> {
    match value.to_lowercase().as_str() {
        "true" | "yes" | "on" | "1" | "evet" | "e" => Ok(true),
        "false" | "no" | "off" | "0" | "hayir" | "hayır" | "h" => Ok(false),
        _ => bail!("{value} is not true or false"),
    }
}

fn one_of(value: &str, allowed: &[&str]) -> anyhow::Result<String> {
    let lower = value.to_lowercase();
    if !allowed.contains(&lower.as_str()) {
        bail!("{value} is not one of: {}", allowed.join(", "));
    }
    Ok(lower)
}

impl Settings {
    /// Changes one setting. The value is checked.
    pub fn set(&mut self, key: &str, value: &str) -> anyhow::Result<()> {
        match key.replace('-', "_").as_str() {
            "language" => self.language = one_of(value, &["tr", "en"])?,
            "port" => self.port = value.parse().ok().filter(|p| *p >= 1024).with_context(|| format!("{value} is not a port from 1024 to 65535"))?,
            "vector" => self.vector = one_of(value, &["multilingual", "english", "off"])?,
            "workers" => self.workers = value.parse().ok().filter(|w| WORKER_CHOICES.contains(w)).with_context(|| format!("{value} is not 1, 2 or 4"))?,
            "autostart" => self.autostart = parse_bool(value)?,
            "claude" => self.claude = one_of(value, &["user", "project", "none"])?,
            "claude_dir" => self.claude_dir = absolute(value)?,
            "profile" => self.profile = one_of(value, &["auto", "balanced", "lean", "off"])?,
            "budget" => self.budget = value.parse().ok().filter(|b| (BUDGET_RANGE.0..=BUDGET_RANGE.1).contains(b)).with_context(|| format!("{value} is not from 200 to 4000"))?,
            "opencode" => self.opencode = parse_bool(value)?,
            "answers" => self.answers = parse_bool(value)?,
            "check" => self.check = one_of(value, &["auto", "off"])?,
            "cross_project" => self.cross_project = parse_bool(value)?,
            other => bail!("unknown setting {other}; known: {}", KEYS.map(|(k, _)| k).join(", ")),
        }
        Ok(())
    }

    /// Returns the value of one setting as text.
    pub fn get(&self, key: &str) -> String {
        match key {
            "language" => self.language.clone(),
            "port" => self.port.to_string(),
            "vector" => self.vector.clone(),
            "workers" => self.workers.to_string(),
            "autostart" => self.autostart.to_string(),
            "claude" => self.claude.clone(),
            "claude_dir" => self.claude_dir.clone(),
            "profile" => self.profile.clone(),
            "budget" => self.budget.to_string(),
            "opencode" => self.opencode.to_string(),
            "answers" => self.answers.to_string(),
            "check" => self.check.clone(),
            "cross_project" => self.cross_project.to_string(),
            _ => String::new(),
        }
    }

    /// Checks the values of a loaded file.
    pub fn validate(&self) -> anyhow::Result<()> {
        let mut copy = self.clone();
        for (key, _) in KEYS {
            let value = self.get(key);
            if key == "claude_dir" && value.is_empty() {
                continue;
            }
            copy.set(key, &value)?;
        }
        if self.claude == "project" && self.claude_dir.is_empty() {
            bail!("claude = \"project\" needs claude_dir");
        }
        Ok(())
    }

    /// Returns the daemon address.
    pub fn url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    /// Returns the model folder name of the vector setting, or `None` if it is off.
    pub fn model(&self) -> Option<&'static str> {
        match self.vector.as_str() {
            "multilingual" => Some("potion-multilingual-128M"),
            "english" => Some("potion-base-8M"),
            _ => None,
        }
    }

    /// True if the daemon must restart for the change from `old` (the engine settings changed).
    pub fn engine_differs(&self, old: &Settings) -> bool {
        self.port != old.port || self.vector != old.vector || self.workers != old.workers || self.answers != old.answers
    }
}

/// Returns an existing folder as an absolute path.
fn absolute(value: &str) -> anyhow::Result<String> {
    let expanded = match value.strip_prefix("~/") {
        Some(rest) => PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(rest),
        None => PathBuf::from(value),
    };
    let path = expanded.canonicalize().with_context(|| format!("{value} is not an existing folder"))?;
    if !path.is_dir() {
        bail!("{value} is not a folder");
    }
    Ok(path.display().to_string())
}
