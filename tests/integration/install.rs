//! TS-18 — Installation, settings and verification (ADR-014): `zcie setup`, `zcie config`,
//! `zcie verify`, `zcie uninstall` and the npm launcher. Each test uses its own home, HOME and
//! port; the autostart agent is off, so nothing outside the temporary folders changes.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use zcie_tests::free_port;
use zcie_tests::process::binary;

struct Install {
    _dir: tempfile::TempDir,
    root: PathBuf,
    port: u16,
}

impl Install {
    fn new() -> Install {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        std::fs::create_dir_all(root.join("user")).unwrap();
        std::fs::create_dir_all(root.join("project")).unwrap();
        Install { _dir: dir, root, port: free_port() }
    }

    fn home(&self) -> PathBuf {
        self.root.join("zcie-home")
    }

    fn project(&self) -> PathBuf {
        self.root.join("project")
    }

    /// Runs `zcie` with the test home, a test HOME and a PATH without claude and opencode.
    fn zcie(&self, args: &[&str], stdin: &str) -> Output {
        let mut child = Command::new(binary("zcie"))
            .arg("--home")
            .arg(self.home())
            .args(args)
            .current_dir(self.project())
            .env("HOME", self.root.join("user"))
            .env("PATH", "/usr/bin:/bin:/usr/sbin:/sbin")
            .env_remove("ZCIE_WORKSPACE")
            .env_remove("ZCIE_URL")
            .env_remove("ZCIE_API_KEY")
            .env_remove("ZCIE_API_KEY_FILE")
            .env_remove("ZCIE_SETTINGS_FILE")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child.stdin.take().unwrap().write_all(stdin.as_bytes()).unwrap();
        child.wait_with_output().unwrap()
    }

    fn setup_yes(&self) -> Output {
        let port = self.port.to_string();
        let dir = self.project().display().to_string();
        let args = ["setup", "--yes", "--language", "en", "--claude", "project", "--claude-dir", &dir, "--port", &port, "--vector", "off", "--autostart", "false", "--import", "false"];
        self.zcie(&args, "")
    }

    fn read(&self, path: &Path) -> String {
        std::fs::read_to_string(path).unwrap_or_default()
    }

    fn claude_settings(&self) -> String {
        self.read(&self.project().join(".claude/settings.local.json"))
    }
}

fn text(output: &Output) -> String {
    format!("{}{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr))
}

impl Drop for Install {
    fn drop(&mut self) {
        let _ = self.zcie(&["daemon", "stop"], "");
    }
}

#[test]
fn ts_18_01_setup_with_options_installs_and_verifies() {
    let install = Install::new();
    let output = install.setup_yes();
    let out = text(&output);
    assert!(output.status.success(), "{out}");
    assert!(out.contains("checks passed") && !out.contains('✗'), "{out}");
    assert!(out.contains("a NEW session gets the fact of an earlier session"), "{out}");
    let settings = install.read(&install.home().join("config/setup.toml"));
    assert!(settings.contains(&format!("port = {}", install.port)) && settings.contains("vector = \"off\""), "{settings}");
    let engine = install.read(&install.home().join("config/engine.toml"));
    assert!(engine.contains(&format!("127.0.0.1:{}", install.port)) && engine.contains("worker_count = 2"), "{engine}");
    assert!(install.claude_settings().contains("\"ZCIE_INJECT_BUDGET\": \"1200\""));
}

#[test]
fn ts_18_02_config_set_applies_and_rejects_wrong_values() {
    let install = Install::new();
    assert!(install.setup_yes().status.success());
    let changed = install.zcie(&["config", "set", "budget", "900", "workers", "1"], "");
    assert!(changed.status.success(), "{}", text(&changed));
    assert!(text(&changed).contains("budget: 1200 → 900"));
    assert!(install.claude_settings().contains("\"ZCIE_INJECT_BUDGET\": \"900\""));
    assert!(install.read(&install.home().join("config/engine.toml")).contains("worker_count = 1"));
    let before = install.read(&install.home().join("config/setup.toml"));
    for wrong in [["budget", "99999"], ["vector", "fast"], ["colour", "red"]] {
        let output = install.zcie(&["config", "set", wrong[0], wrong[1]], "");
        assert!(!output.status.success(), "{wrong:?} must be refused");
    }
    assert_eq!(install.read(&install.home().join("config/setup.toml")), before, "a refused value changes nothing");
    let shown = text(&install.zcie(&["config", "show"], ""));
    assert!(shown.contains("budget      = 900") && shown.contains("workers     = 1"), "{shown}");
}

#[test]
fn ts_18_03_wizard_takes_answers_and_keeps_the_others() {
    let install = Install::new();
    assert!(install.setup_yes().status.success());
    // language 1 (tr), claude (keep), folder (keep), profile 3 (lean), answers (keep), check
    // (keep), budget (keep), vector (keep), workers (keep), autostart n, port (keep), apply y.
    let output = install.zcie(&["setup", "--no-verify"], "1\n\n\n3\n\n\n\n\n\nn\n\ny\n");
    assert!(output.status.success(), "{}", text(&output));
    let settings = install.read(&install.home().join("config/setup.toml"));
    assert!(settings.contains("language = \"tr\"") && settings.contains("profile = \"lean\""), "{settings}");
    assert!(settings.contains("budget = 1200") && settings.contains(&format!("port = {}", install.port)), "{settings}");
    assert!(settings.contains("check = \"auto\"") && settings.contains("cross_project = true"), "{settings}");
    let declined = install.zcie(&["setup", "--no-verify"], "2\n\n\n1\n\n\n\n\n\nn\n\nn\n");
    assert!(text(&declined).contains("Nothing was changed"), "{}", text(&declined));
    assert!(install.read(&install.home().join("config/setup.toml")).contains("profile = \"lean\""));
}

#[test]
fn ts_18_04_verify_fails_when_the_daemon_is_stopped() {
    let install = Install::new();
    assert!(install.setup_yes().status.success());
    assert!(install.zcie(&["verify"], "").status.success());
    assert!(install.zcie(&["daemon", "stop"], "").status.success());
    let output = install.zcie(&["verify"], "");
    assert!(!output.status.success());
    assert!(text(&output).contains("✗  daemon does not answer"), "{}", text(&output));
}

#[test]
fn ts_18_05_uninstall_purge_removes_entries_and_data() {
    let install = Install::new();
    assert!(install.setup_yes().status.success());
    let output = install.zcie(&["uninstall", "--purge"], "");
    assert!(output.status.success(), "{}", text(&output));
    assert!(!install.home().exists(), "the data folder is deleted");
    assert!(!install.claude_settings().contains("zcie"), "the Claude Code entries are removed");
    assert!(!install.zcie(&["verify"], "").status.success(), "nothing is set up after uninstall");
}

#[test]
fn ts_18_06_npm_launcher_runs_the_native_program() {
    if Command::new("node").arg("--version").output().is_err() {
        println!("skipped: node is not installed");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let package = dir.path().join("zcie");
    let launcher = zcie_tests::workspace_root().join("npm/zcie/bin/zcie.js");
    std::fs::create_dir_all(package.join("bin")).unwrap();
    std::fs::copy(&launcher, package.join("bin/zcie.js")).unwrap();
    let node = |args: &[&str]| Command::new("node").arg(package.join("bin/zcie.js")).args(args).env("ZCIE_HOME", dir.path().join("home")).output().unwrap();
    let missing = node(&["--version"]);
    assert_eq!(missing.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&missing.stderr).contains("has no build for"));
    let platform = String::from_utf8(Command::new("node").args(["-p", "process.platform + '-' + process.arch"]).output().unwrap().stdout).unwrap();
    let native = package.join("native").join(platform.trim());
    std::fs::create_dir_all(&native).unwrap();
    for name in ["zcie", "zcied", "zcie-mcp"] {
        std::fs::copy(binary(name), native.join(name)).unwrap();
    }
    let version = node(&["--version"]);
    assert!(version.status.success() && String::from_utf8_lossy(&version.stdout).contains(env!("CARGO_PKG_VERSION")));
    let hint = node(&["health"]);
    assert!(String::from_utf8_lossy(&hint.stderr).contains("Run: zcie setup"), "a command before setup prints the hint");
}

#[test]
fn ts_18_07_project_add_indexes_the_folder_and_shows_the_state() {
    let install = Install::new();
    assert!(install.setup_yes().status.success());
    let code = install.root.join("billing");
    std::fs::create_dir_all(code.join("src")).unwrap();
    std::fs::write(code.join("src/fee.rs"), "/// Adds the late fee.\npub fn apply_late_fee(cents: i64) -> i64 {\n    cents + 250\n}\n").unwrap();
    std::fs::write(code.join("README.md"), "# Billing\nLate fees are 250 cents.\n").unwrap();
    let dir = code.display().to_string();
    let added = install.zcie(&["project", "add", &dir, "--name", "billing"], "");
    let out = text(&added);
    assert!(added.status.success(), "{out}");
    assert!(out.contains("index     complete") && out.contains("project   billing"), "{out}");
    assert!(code.join(".claude/zcie_id").exists(), "the ID file is written");
    let state = text(&install.zcie(&["project", "status", &dir], ""));
    // src/fee.rs and README.md; the ID file .claude/zcie_id is written after the scan starts, so
    // the scan or the watcher adds it a moment later.
    assert!((state.contains("files     2") || state.contains("files     3")) && state.contains("symbols   1"), "{state}");
    let query = text(&install.zcie(&["--project", "billing", "query", "where is apply_late_fee defined"], ""));
    assert!(query.contains("src/fee.rs"), "{query}");
    let missing = install.zcie(&["project", "status", install.root.join("user").to_str().unwrap()], "");
    assert!(!missing.status.success() && text(&missing).contains("zcie project add"), "a folder without a project gets the hint");
}

/// Makes `<root>/outer` (a Git repository that ignores `inner/` and `data/`) with the Git
/// repository `inner` inside it, like a setup folder that holds its application repositories.
fn nested_folders(root: &Path) -> (PathBuf, PathBuf) {
    let outer = root.join("outer");
    let inner = outer.join("inner");
    std::fs::create_dir_all(inner.join("src")).unwrap();
    std::fs::create_dir_all(outer.join("data")).unwrap();
    std::fs::write(outer.join(".gitignore"), "inner/\ndata/\n").unwrap();
    std::fs::write(outer.join("setup.sh"), "echo setup\n").unwrap();
    std::fs::write(outer.join("data/db.bin"), "data").unwrap();
    std::fs::write(inner.join("src/lib.rs"), "pub fn inner_fee() -> i64 {\n    1\n}\n").unwrap();
    for dir in [&outer, &inner] {
        assert!(Command::new("git").args(["init", "-q"]).current_dir(dir).status().unwrap().success());
    }
    (outer, inner)
}

#[test]
fn ts_18_08_project_add_reports_and_adds_nested_repositories() {
    let install = Install::new();
    assert!(install.setup_yes().status.success());
    let (outer, inner) = nested_folders(&install.root);
    let first = text(&install.zcie(&["project", "add", outer.to_str().unwrap()], ""));
    assert!(first.contains("skipped inner/ (a separate Git repository") && first.contains("skipped data/ (.gitignore)"), "{first}");
    assert!(!inner.join(".claude/zcie_id").exists(), "without --nested the inner repository is not added");
    let both = install.zcie(&["project", "add", outer.to_str().unwrap(), "--nested"], "");
    assert!(both.status.success(), "{}", text(&both));
    let outer_id = install.read(&outer.join(".claude/zcie_id"));
    let inner_id = install.read(&inner.join(".claude/zcie_id"));
    assert!(!inner_id.is_empty() && inner_id != outer_id, "the inner repository is its own project");
    let state = text(&install.zcie(&["project", "status", inner.to_str().unwrap()], ""));
    assert!(state.contains("symbols   1") && state.contains("index     complete"), "{state}");
}

#[test]
fn ts_18_09_hook_does_not_give_a_nested_repository_the_outer_project() {
    let install = Install::new();
    assert!(install.setup_yes().status.success());
    let (outer, inner) = nested_folders(&install.root);
    assert!(install.zcie(&["project", "add", outer.to_str().unwrap(), "--no-wait"], "").status.success());
    let input = format!("{{\"session_id\":\"s-nested\",\"cwd\":\"{}\",\"source\":\"startup\"}}", inner.display());
    let mut hook = Command::new("sh")
        .arg(install.home().join("hooks/session-start.sh"))
        .env("ZCIE_URL", format!("http://127.0.0.1:{}", install.port))
        .env("ZCIE_API_KEY_FILE", install.home().join("config/claude-code.key"))
        .env_remove("ZCIE_WORKSPACE")
        .stdin(Stdio::piped())
        .spawn()
        .unwrap();
    hook.stdin.take().unwrap().write_all(input.as_bytes()).unwrap();
    assert!(hook.wait().unwrap().success());
    let inner_id = install.read(&inner.join(".claude/zcie_id"));
    assert!(!inner_id.is_empty() && inner_id != install.read(&outer.join(".claude/zcie_id")), "a session in the inner repository gets its own project");
}

#[test]
fn ts_18_10_stop_hook_stores_the_answer_of_the_assistant() {
    let install = Install::new();
    assert!(install.setup_yes().status.success());
    assert!(install.read(&install.home().join("config/engine.toml")).contains("assistant_messages = true"), "answers are stored by default");
    assert!(install.claude_settings().contains("assistant-message.sh"), "the Stop hook is set");
    let answer = "Rapor servisi yavaş çünkü invoice_lines tablosunda customer_id indeksi yok; indeksi ekledim.";
    let input = serde_json::json!({"session_id": "s-stop", "cwd": install.project(), "hook_event_name": "Stop", "last_assistant_message": answer});
    let mut hook = Command::new("sh")
        .arg(install.home().join("hooks/assistant-message.sh"))
        .env("ZCIE_URL", format!("http://127.0.0.1:{}", install.port))
        .env("ZCIE_API_KEY_FILE", install.home().join("config/claude-code.key"))
        .env_remove("ZCIE_WORKSPACE")
        .stdin(Stdio::piped())
        .spawn()
        .unwrap();
    hook.stdin.take().unwrap().write_all(input.to_string().as_bytes()).unwrap();
    assert!(hook.wait().unwrap().success());
    std::thread::sleep(std::time::Duration::from_secs(2));
    let found = text(&install.zcie(&["query", "invoice_lines indeksi neden eklendi"], ""));
    assert!(found.contains("customer_id"), "a new query finds the earlier answer: {found}");
}

/// Runs the Cursor dispatcher of an install with a Cursor hook input. Returns stdout.
fn cursor_hook(install: &Install, input: serde_json::Value) -> String {
    let mut hook = Command::new("sh")
        .arg(install.home().join("hooks/cursor-hook.sh"))
        .env("ZCIE_URL", format!("http://127.0.0.1:{}", install.port))
        .env("ZCIE_API_KEY_FILE", install.home().join("config/claude-code.key"))
        .env_remove("ZCIE_WORKSPACE")
        .env_remove("ZCIE_APP")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    hook.stdin.take().unwrap().write_all(input.to_string().as_bytes()).unwrap();
    String::from_utf8(hook.wait_with_output().unwrap().stdout).unwrap()
}

#[test]
fn ts_25_05_cursor_hooks_store_edits_and_answers_with_app_cursor() {
    let install = Install::new();
    assert!(install.setup_yes().status.success());
    let repo = install.root.join("cursor-app");
    std::fs::create_dir_all(repo.join("src")).unwrap();
    zcie_tests::git(&repo, &["init", "-q"]);
    let folder = repo.display().to_string();
    let input = |event: &str, extra: serde_json::Value| {
        let mut value = serde_json::json!({"hook_event_name": event, "conversation_id": "c-cursor", "cursor_version": "3.25.6", "workspace_roots": [folder]});
        value.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
        value
    };
    let start = cursor_hook(&install, input("sessionStart", serde_json::json!({})));
    assert!(start.contains("ZCIE: this project"), "the new chat gets the rule: {start}");
    let id = install.read(&repo.join(".claude/zcie_id")).trim().to_string();
    assert!(!id.is_empty(), "the project ID file is written");
    std::fs::write(repo.join("src/rate.rs"), "pub const LIMIT: u32 = 600;\n").unwrap();
    let file = repo.join("src/rate.rs").display().to_string();
    cursor_hook(&install, input("afterFileEdit", serde_json::json!({"file_path": file})));
    cursor_hook(&install, input("afterAgentResponse", serde_json::json!({"text": "Karar: rate limit dakikada 600 istek olacak."})));
    std::thread::sleep(std::time::Duration::from_secs(2));
    let key = install.read(&install.home().join("config/claude-code.key")).trim().to_string();
    let client = zcie_sdk::Client::new(&format!("http://127.0.0.1:{}", install.port), &key, &id);
    let found = serde_json::to_value(client.query("rate limit dakikada kaç istek", 1000).unwrap()).unwrap();
    let refs: Vec<String> = found["results"].as_array().unwrap().iter().filter_map(|r| r["ref"].as_str().map(str::to_string)).collect();
    let expanded = serde_json::to_value(client.expand(&refs, zcie_types::ExpandLevel::Evidence, Some(4000)).unwrap()).unwrap().to_string();
    assert!(expanded.contains("assistant.message") && expanded.contains("cursor"), "the answer is stored with app cursor: {expanded}");
    // The file.changed event starts a file job; the activity log names its target.
    let log = install.read(&install.home().join("stats/activity.jsonl"));
    assert!(log.lines().any(|l| l.contains("\"job\"") && l.contains("rate.rs")), "the edit is indexed: {log}");
}

#[test]
fn ts_25_08_cursor_session_end_replays_the_transcript_once() {
    let install = Install::new();
    assert!(install.setup_yes().status.success());
    let repo = install.root.join("cursor-cli");
    std::fs::create_dir_all(&repo).unwrap();
    zcie_tests::git(&repo, &["init", "-q"]);
    let folder = repo.display().to_string();
    // The turn file is in the shared TMPDIR: the session ids must be new in each run.
    let unique = install.root.file_name().unwrap().to_string_lossy().to_string();
    let (cli, gui) = (format!("c-cli-{unique}"), format!("c-gui-{unique}"));
    let input = |event: &str, session: &str, extra: serde_json::Value| {
        let mut value = serde_json::json!({"hook_event_name": event, "conversation_id": session, "cursor_version": "2026.10.19", "workspace_roots": [folder]});
        value.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
        value
    };
    cursor_hook(&install, input("sessionStart", &cli, serde_json::json!({})));
    let id = install.read(&repo.join(".claude/zcie_id")).trim().to_string();
    assert!(!id.is_empty(), "the project ID file is written");
    let line = |role: &str, text: &str| serde_json::json!({"role": role, "message": {"content": [{"type": "text", "text": text}]}}).to_string();
    let first = install.root.join("cli.jsonl");
    std::fs::write(&first, [
        line("user", "<timestamp>Thursday, Oct 22, 2026, 9:03 PM (UTC+3)</timestamp>\n<user_query>\nÖnbellek süresi neden 90 saniye?\n</user_query>"),
        line("assistant", "**Analyzing** düşünce"),
        line("assistant", "Karar: önbellek süresi 90 saniye, çünkü fiyat verisi sık değişiyor."),
        serde_json::json!({"type": "turn_ended", "status": "success"}).to_string(),
    ].join("\n")).unwrap();
    let end = |session: &str, path: &Path| input("sessionEnd", session, serde_json::json!({"reason": "completed", "transcript_path": path}));
    cursor_hook(&install, end(&cli, &first));
    cursor_hook(&install, end(&cli, &first));
    // A GUI session: beforeSubmitPrompt ran, so sessionEnd must not replay the transcript.
    cursor_hook(&install, input("beforeSubmitPrompt", &gui, serde_json::json!({"prompt": "Merhaba"})));
    let second = install.root.join("gui.jsonl");
    std::fs::write(&second, [line("user", "<user_query>\nzebrafjord deposu nerede?\n</user_query>"), line("assistant", "zebrafjord deposu yok.")].join("\n")).unwrap();
    cursor_hook(&install, end(&gui, &second));
    std::thread::sleep(std::time::Duration::from_secs(2));
    let key = install.read(&install.home().join("config/claude-code.key")).trim().to_string();
    let client = zcie_sdk::Client::new(&format!("http://127.0.0.1:{}", install.port), &key, &id);
    let find = |q: &str| {
        let found = serde_json::to_value(client.query(q, 2000).unwrap()).unwrap();
        let refs: Vec<String> = found["results"].as_array().unwrap().iter().filter_map(|r| r["ref"].as_str().map(str::to_string)).collect();
        if refs.is_empty() { return found.to_string(); }
        // A memory expands to its text only; the raw events (kind and app) come from its provenance refs.
        let memories = serde_json::to_value(client.expand(&refs, zcie_types::ExpandLevel::Evidence, Some(4000)).unwrap()).unwrap();
        let events: Vec<String> = memories["items"].as_array().unwrap().iter().flat_map(|i| i["provenance"].as_array().cloned().unwrap_or_default()).filter_map(|p| p.as_str().map(str::to_string)).collect();
        let audit = serde_json::to_value(client.expand(&events, zcie_types::ExpandLevel::Evidence, Some(4000)).unwrap()).unwrap();
        format!("{memories}{audit}")
    };
    let replayed = find("önbellek süresi 90 saniye");
    assert!(replayed.contains("user.message") && replayed.contains("assistant.message") && replayed.contains("cursor"), "the prompt and the answer are replayed with app cursor: {replayed}");
    assert!(!replayed.contains("**Analyzing"), "only the last text of the turn is the answer: {replayed}");
    let skipped = find("zebrafjord deposu");
    assert!(!skipped.contains("zebrafjord"), "a session with beforeSubmitPrompt replays nothing: {skipped}");
}

#[test]
fn ts_25_07_cursor_setup_and_remove_touch_only_zcie_entries() {
    let install = Install::new();
    let folder = install.root.join("cursor-project");
    std::fs::create_dir_all(folder.join(".cursor")).unwrap();
    std::fs::write(folder.join(".cursor/mcp.json"), r#"{"mcpServers":{"other":{"command":"other-tool"}}}"#).unwrap();
    let dir = folder.display().to_string();
    let setup = install.zcie(&["cursor", "setup", "--dir", &dir], "");
    assert!(setup.status.success(), "{}", text(&setup));
    let events = ["sessionStart", "beforeSubmitPrompt", "afterFileEdit", "afterAgentResponse", "sessionEnd"];
    let count = |file: &str| {
        let value: serde_json::Value = serde_json::from_str(&install.read(&folder.join(file))).unwrap();
        events.map(|e| value["hooks"][e].as_array().map_or(0, |list| list.iter().filter(|h| h["command"].as_str().is_some_and(|c| c.ends_with("cursor-hook.sh"))).count()))
    };
    assert_eq!(count(".cursor/hooks.json"), [1; 5], "one entry per event");
    assert!(install.read(&folder.join(".cursor/mcp.json")).contains("\"zcie\"") && install.read(&folder.join(".cursor/mcp.json")).contains("\"other\""));
    assert!(install.zcie(&["cursor", "setup", "--dir", &dir], "").status.success());
    assert_eq!(count(".cursor/hooks.json"), [1; 5], "a second setup adds nothing");
    let removed = install.zcie(&["cursor", "remove", "--dir", &dir], "");
    assert!(removed.status.success(), "{}", text(&removed));
    assert_eq!(count(".cursor/hooks.json"), [0; 5], "remove takes the ZCIE hooks out");
    let mcp = install.read(&folder.join(".cursor/mcp.json"));
    assert!(!mcp.contains("\"zcie\"") && mcp.contains("\"other\""), "only the ZCIE server is removed: {mcp}");
    // Global: `zcie setup --cursor true` writes ~/.cursor, `verify` reports it, `uninstall` removes it.
    let port = install.port.to_string();
    let project = install.project().display().to_string();
    let args = ["setup", "--yes", "--language", "en", "--claude", "project", "--claude-dir", &project, "--cursor", "true", "--port", &port, "--vector", "off", "--autostart", "false", "--import", "false"];
    let output = install.zcie(&args, "");
    assert!(output.status.success(), "{}", text(&output));
    let hooks = install.read(&install.root.join("user/.cursor/hooks.json"));
    assert!(hooks.contains("cursor-hook.sh"), "{hooks}");
    let verify = text(&install.zcie(&["verify"], ""));
    assert!(verify.contains("Cursor hooks installed"), "{verify}");
    assert!(install.zcie(&["uninstall", "--purge"], "").status.success());
    assert!(!install.read(&install.root.join("user/.cursor/hooks.json")).contains("cursor-hook.sh"), "uninstall removes the Cursor hook");
}
