//! Live test with real Claude Code sessions (ADR-014): session 1 tells a fact, session 2 is a
//! NEW `claude -p` session (no `--resume`) and must answer with the fact. It uses the Claude Code
//! account of the user (about $0.05) and runs in a temporary folder that is deleted afterwards.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use zcie_sdk::Client;

use super::settings::Settings;
use super::verify::{Check, new_code};
use crate::commands::claude::{self, Profile, Scope, SetupArgs};

/// Longest time of one Claude Code session.
const SESSION_WAIT: Duration = Duration::from_secs(240);
/// Tries to start `claude`, and the wait between two tries.
const SPAWN_TRIES: usize = 6;
const SPAWN_RETRY: Duration = Duration::from_secs(5);
/// Variables of a surrounding ZCIE setup (for example when `zcie setup` runs inside a Claude
/// Code session). The test session must use only the settings of the installation under test.
pub const OUTER_VARIABLES: [&str; 5] = ["ZCIE_WORKSPACE", "ZCIE_URL", "ZCIE_API_KEY", "ZCIE_API_KEY_FILE", "ZCIE_SETTINGS_FILE"];

/// True if a program is on the PATH.
pub fn on_path(name: &str) -> bool {
    std::env::var_os("PATH").is_some_and(|path| std::env::split_paths(&path).any(|dir| dir.join(name).is_file()))
}

/// True if the `claude` command is installed.
pub fn claude_available() -> bool {
    on_path("claude")
}

/// Tries to start `claude`; it is missing for some seconds while Claude Code updates itself.
fn spawn_claude(dir: &Path, prompt: &str) -> Result<std::process::Child, String> {
    let mut last = String::new();
    for _ in 0..SPAWN_TRIES {
        let mut command = Command::new("claude");
        for name in OUTER_VARIABLES {
            command.env_remove(name);
        }
        let spawned = command.args(["-p", prompt, "--output-format", "text"]).current_dir(dir).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn();
        match spawned {
            Ok(child) => return Ok(child),
            Err(error) => last = error.to_string(),
        }
        std::thread::sleep(SPAWN_RETRY);
    }
    Err(format!("claude does not start: {last}"))
}

/// Runs `claude -p <prompt>` in `dir` and returns its answer, or an error text.
fn ask_claude(dir: &Path, prompt: &str) -> Result<String, String> {
    let mut child = spawn_claude(dir, prompt)?;
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if started.elapsed() < SESSION_WAIT => std::thread::sleep(Duration::from_millis(500)),
            _ => {
                let _ = child.kill();
                return Err(format!("claude did not answer within {} s", SESSION_WAIT.as_secs()));
            }
        }
    }
    let output = child.wait_with_output().map_err(|e| e.to_string())?;
    if !output.status.success() {
        let error = String::from_utf8_lossy(&output.stderr);
        return Err(format!("claude failed: {}", error.trim().chars().take(200).collect::<String>()));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Sets up the Claude Code hooks for the test folder when the settings do not cover all projects.
fn prepare(home: &Path, s: &Settings, dir: &Path) -> anyhow::Result<()> {
    if s.claude == "user" {
        return Ok(());
    }
    let args = SetupArgs {
        scope: Scope::Project,
        dir: Some(dir.to_path_buf()),
        for_workspace: None,
        profile: Profile::Auto,
        allow: String::new(),
        no_lean: true,
        no_inject: false,
        no_guards: true,
        no_capture: false,
        cache_5m: false,
        no_register: true,
        budget: Some(s.budget),
        no_check: true,
        no_cross_project: false,
        dry_run: false,
    };
    claude::setup::setup(&args, home, &s.url())
}

/// Runs the live test and returns its check.
pub fn run(home: &Path, s: &Settings) -> Check {
    let code = new_code();
    let dir: PathBuf = std::env::temp_dir().join(format!("zcie-live-{code}"));
    let result = std::fs::create_dir_all(&dir).map_err(|e| e.to_string()).and_then(|()| prepare(home, s, &dir).map_err(|e| e.to_string())).and_then(|()| sessions(&dir, &code));
    if let Ok(project) = std::fs::read_to_string(dir.join(".claude/zcie_id")) {
        let admin = std::fs::read_to_string(home.join("config/admin.key")).unwrap_or_default();
        let _ = Client::new(&s.url(), admin.trim(), project.trim()).delete_project(project.trim());
    }
    let _ = std::fs::remove_dir_all(&dir);
    match result {
        Ok(answer) => Check { ok: true, text: format!("live: a NEW Claude Code session answered \"{answer}\" (expected {code})") },
        Err(error) => Check { ok: false, text: format!("live: {error}") },
    }
}

fn sessions(dir: &Path, code: &str) -> Result<String, String> {
    let fact = format!("Not: bu projede dağıtım için {code} isimli sunucuyu kullanıyoruz. Bunu bil. Yalnızca 'tamam' yaz, araç kullanma.");
    ask_claude(dir, &fact)?;
    let answer = ask_claude(dir, "Bu projede dağıtım için hangi sunucuyu kullanıyoruz? Yalnızca sunucu adını yaz, araç kullanma.")?;
    let short: String = answer.chars().take(80).collect();
    if answer.to_uppercase().contains(code) {
        Ok(short)
    } else {
        Err(format!("the new session did not know the fact; it answered \"{short}\""))
    }
}
