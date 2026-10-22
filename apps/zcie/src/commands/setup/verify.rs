//! `zcie verify`: checks that the installation really works, end to end (ADR-014). The memory
//! check runs the installed Claude Code hooks like Claude Code does: a session tells a fact, a
//! NEW session asks for it, and the capsule of the new session must have it.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::bail;
use serde_json::json;
use zcie_sdk::Client;

use super::settings::Settings;
use super::t;

/// Time for a captured message to become searchable.
const RECALL_WAIT: Duration = Duration::from_secs(15);
/// Time for the MCP server to answer.
const MCP_WAIT: Duration = Duration::from_secs(10);

/// The result of one check.
pub struct Check {
    /// True if the check passed.
    pub ok: bool,
    /// What was checked and the result.
    pub text: String,
}

fn pass(text: impl Into<String>) -> Check {
    Check { ok: true, text: text.into() }
}

fn fail(text: impl Into<String>) -> Check {
    Check { ok: false, text: text.into() }
}

/// Returns a unique code for one verification ("ZV-3FA9C1").
pub fn new_code() -> String {
    let seed = format!("{:?}{}", Instant::now(), std::process::id());
    let hash = blake3::hash(format!("{seed}{:?}", std::time::SystemTime::now()).as_bytes()).to_hex();
    format!("ZV-{}", hash[..6].to_uppercase())
}

/// Runs all checks and prints them. Fails if a check fails.
pub fn run(home: &Path, s: &Settings, live: bool) -> anyhow::Result<()> {
    let l = s.language.as_str();
    println!("\n{}", t(l, "Checking the installation:", "Kurulum doğrulanıyor:"));
    let mut checks = vec![binaries(), tool("curl"), tool("jq"), daemon(s)];
    if s.autostart && cfg!(target_os = "macos") {
        let loaded = crate::commands::service::is_loaded(home);
        checks.push(if loaded { pass("autostart: launchd agent is loaded") } else { fail("autostart: launchd agent is not loaded (fix: zcie setup)") });
    }
    checks.push(engine_config(home, s));
    if s.claude != "none" {
        checks.push(claude_settings(home, s));
    }
    checks.extend(memory(home, s));
    checks.push(mcp(home, s));
    if s.opencode {
        let plugin = PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(".config/opencode/plugins/zcie.js");
        checks.push(if plugin.exists() { pass("OpenCode plugin installed") } else { fail("OpenCode plugin is missing (fix: zcie setup)") });
    }
    if s.cursor {
        let hooks = PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(".cursor/hooks.json");
        let installed = std::fs::read_to_string(&hooks).is_ok_and(|text| text.contains("cursor-hook.sh"));
        checks.push(if installed { pass("Cursor hooks installed") } else { fail("Cursor hooks are missing (fix: zcie setup)") });
    }
    if live {
        println!("{}", t(l, "  live test with Claude Code (two new sessions, about a minute)…", "  Claude Code ile canlı test (iki yeni oturum, yaklaşık bir dakika)…"));
        checks.push(super::live::run(home, s));
    }
    report(l, &checks)
}

fn report(l: &str, checks: &[Check]) -> anyhow::Result<()> {
    for check in checks {
        println!("  {}  {}", if check.ok { "✓" } else { "✗" }, check.text);
    }
    let passed = checks.iter().filter(|c| c.ok).count();
    println!("\n{passed}/{} {}", checks.len(), t(l, "checks passed.", "kontrol geçti."));
    if passed < checks.len() {
        bail!("{}", t(l, "ZCIE does not work completely; see the ✗ lines.", "ZCIE tam çalışmıyor; ✗ satırlarına bakın."));
    }
    println!("{}", t(l, "ZCIE works. Start a new Claude Code session; it remembers your earlier sessions.", "ZCIE çalışıyor. Yeni bir Claude Code oturumu açın; önceki oturumlarınızı hatırlar."));
    Ok(())
}

fn sibling(name: &str) -> Option<PathBuf> {
    let path = std::env::current_exe().ok()?.parent()?.join(name);
    path.exists().then_some(path)
}

fn binaries() -> Check {
    match (sibling("zcied"), sibling("zcie-mcp")) {
        (Some(_), Some(_)) => pass(format!("programs zcie, zcied, zcie-mcp {}", env!("CARGO_PKG_VERSION"))),
        _ => fail("zcied or zcie-mcp is missing next to zcie (reinstall the package)"),
    }
}

fn tool(name: &str) -> Check {
    let found = Command::new(name).arg("--version").stdout(Stdio::null()).stderr(Stdio::null()).status().is_ok_and(|s| s.success());
    match (found, name) {
        (true, _) => pass(format!("{name} found (the hooks need it)")),
        (false, "jq") => fail("jq is missing: the hooks do nothing without it (fix: brew install jq)"),
        (false, _) => fail(format!("{name} is missing: the hooks need it")),
    }
}

fn daemon(s: &Settings) -> Check {
    match Client::new(&s.url(), "", "verify").health() {
        Ok(h) if h.version == env!("CARGO_PKG_VERSION") => pass(format!("daemon {} answers at {}", h.version, s.url())),
        Ok(h) => fail(format!("daemon {} answers, but zcie is {} (fix: zcie daemon restart)", h.version, env!("CARGO_PKG_VERSION"))),
        Err(_) => fail(format!("daemon does not answer at {} (fix: zcie daemon start)", s.url())),
    }
}

fn engine_config(home: &Path, s: &Settings) -> Check {
    let text = std::fs::read_to_string(home.join("config/engine.toml")).unwrap_or_default();
    let port_ok = text.contains(&format!("127.0.0.1:{}", s.port));
    let model_ok = s.model().is_none_or(|m| crate::commands::vector::model_present(home, m));
    match (port_ok, model_ok) {
        (true, true) => pass(format!("engine.toml matches the settings (vector search: {})", s.vector)),
        (false, _) => fail("engine.toml does not match setup.toml (fix: zcie setup --yes)"),
        (_, false) => fail("the vector model is missing (fix: zcie vector install)"),
    }
}

fn claude_settings(home: &Path, s: &Settings) -> Check {
    let file = match s.claude.as_str() {
        "project" => PathBuf::from(&s.claude_dir).join(".claude/settings.local.json"),
        _ => PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(".claude/settings.json"),
    };
    let text = std::fs::read_to_string(&file).unwrap_or_default();
    let hooks = home.join("hooks").display().to_string();
    let parsed = serde_json::from_str::<serde_json::Value>(&text).is_ok();
    match (parsed, text.contains(&format!("{hooks}/inject-context.sh")) && text.contains(&format!("{hooks}/user-prompt-submit.sh"))) {
        (true, true) => pass(format!("Claude Code hooks in {}", file.display())),
        _ => fail(format!("Claude Code hooks are missing in {} (fix: zcie setup --yes)", file.display())),
    }
}

/// The key file that the hooks use.
pub fn key_file(home: &Path) -> PathBuf {
    let claude = home.join("config/claude-code.key");
    if claude.exists() { claude } else { home.join("config/admin.key") }
}

/// Runs one installed hook script with a JSON input, like Claude Code does. Returns stdout.
pub fn run_hook(home: &Path, s: &Settings, script: &str, input: &serde_json::Value) -> String {
    let child = Command::new("sh")
        .arg(home.join("hooks").join(script))
        .env("ZCIE_URL", s.url())
        .env("ZCIE_API_KEY_FILE", key_file(home))
        .env_remove("ZCIE_WORKSPACE")
        .env_remove("ZCIE_API_KEY")
        .env_remove("ZCIE_SETTINGS_FILE")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn();
    let Ok(mut child) = child else { return String::new() };
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(input.to_string().as_bytes());
    }
    child.wait_with_output().map(|o| String::from_utf8_lossy(&o.stdout).to_string()).unwrap_or_default()
}

/// Session 1 tells a fact through the hooks; session 2 (new) gets it in its capsule; an English
/// question finds the Turkish fact. The temporary project is deleted after the check.
fn memory(home: &Path, s: &Settings) -> Vec<Check> {
    let code = new_code();
    let dir = std::env::temp_dir().join(format!("zcie-verify-{code}"));
    if std::fs::create_dir_all(&dir).is_err() {
        return vec![fail(format!("cannot make {}", dir.display()))];
    }
    let cwd = dir.display().to_string();
    run_hook(home, s, "session-start.sh", &json!({"session_id": format!("{code}-1"), "cwd": cwd, "source": "startup"}));
    let project = std::fs::read_to_string(dir.join(".claude/zcie_id")).map(|p| p.trim().to_string()).unwrap_or_default();
    let mut checks = vec![if project.is_empty() { fail("SessionStart hook did not make the project ID file") } else { pass(format!("new folder gets a project ID automatically ({project})")) }];
    if !project.is_empty() {
        checks.extend(recall(home, s, &code, &cwd, &project));
        let admin = std::fs::read_to_string(home.join("config/admin.key")).unwrap_or_default();
        let _ = Client::new(&s.url(), admin.trim(), &project).delete_project(&project);
    }
    let _ = std::fs::remove_dir_all(&dir);
    checks
}

fn recall(home: &Path, s: &Settings, code: &str, cwd: &str, project: &str) -> Vec<Check> {
    let fact = format!("ZCIE doğrulama notu: bu projede dağıtım için {code} isimli sunucuyu kullanıyoruz.");
    run_hook(home, s, "user-prompt-submit.sh", &json!({"session_id": format!("{code}-1"), "cwd": cwd, "prompt": fact}));
    let key = std::fs::read_to_string(key_file(home)).unwrap_or_default();
    let client = Client::new(&s.url(), key.trim(), project);
    let started = Instant::now();
    let found = |q: &str| client.query(q, 1200).map(|r| serde_json::to_string(&r).unwrap_or_default().contains(code)).unwrap_or(false);
    while started.elapsed() < RECALL_WAIT && !found("dağıtım sunucusu") {
        std::thread::sleep(Duration::from_millis(200));
    }
    let stored = started.elapsed();
    let capsule = run_hook(home, s, "inject-context.sh", &json!({"session_id": format!("{code}-2"), "cwd": cwd, "prompt": "Bu projede dağıtım için hangi sunucuyu kullanıyoruz?"}));
    let mut out = vec![
        if capsule.contains(code) {
            pass(format!("a NEW session gets the fact of an earlier session (searchable after {} ms)", stored.as_millis()))
        } else {
            fail("a new session did not get the fact of the earlier session (see: zcie doctor)")
        },
    ];
    if s.vector == "multilingual" {
        let english = found("Which server do we use for deployment in this project?");
        out.push(if english { pass("an English question finds the Turkish fact") } else { fail("an English question did not find the Turkish fact") });
    }
    out
}

/// Starts `zcie-mcp`, sends `initialize` and `tools/list`, and expects the `zcie_query` tool.
fn mcp(home: &Path, s: &Settings) -> Check {
    let Some(binary) = sibling("zcie-mcp") else { return fail("zcie-mcp is missing") };
    let child = Command::new(binary)
        .env("ZCIE_URL", s.url())
        .env("ZCIE_API_KEY_FILE", key_file(home))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn();
    let Ok(mut child) = child else { return fail("zcie-mcp does not start") };
    let requests = [
        json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "zcie-verify", "version": "1"}}}),
        json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
        json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}),
    ];
    // stdin stays open until the answer is read: the server stops at the end of its input.
    let mut stdin = child.stdin.take();
    for request in requests {
        if let Some(input) = stdin.as_mut() {
            let _ = writeln!(input, "{request}");
        }
    }
    let found = read_tools(child.stdout.take());
    drop(stdin);
    let _ = child.kill();
    let _ = child.wait();
    if found { pass("MCP server zcie-mcp lists zcie_query and zcie_expand") } else { fail("MCP server did not list its tools") }
}

fn read_tools(stdout: Option<std::process::ChildStdout>) -> bool {
    let Some(stdout) = stdout else { return false };
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if line.contains("\"id\":2") {
                let _ = sender.send(line.contains("zcie_query") && line.contains("zcie_expand"));
                return;
            }
        }
    });
    receiver.recv_timeout(MCP_WAIT).unwrap_or(false)
}
