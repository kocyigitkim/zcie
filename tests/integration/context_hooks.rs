//! TS-20 — Context efficiency (ADR-017): the Claude Code context hook. TS-20-02 runs the hook
//! against a real daemon; TS-20-03 runs it against a fixed response, so that the order is exact.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::process::{Command, Stdio};

use serde_json::{Value, json};
use zcie_tests::engine::capture_request;
use zcie_tests::{DaemonProcess, commit_all, free_port, ledger_repo, workspace_root};

const GENERIC: &str = include_str!("../../benchmarks/user-ideas/generic.json");

/// Runs `inject-context.sh` with a prompt. Returns stdout.
fn inject(url: &str, key: &str, tmp: &std::path::Path, prompt: &str) -> String {
    inject_in(url, key, tmp, &format!("s-{}", prompt.len()), prompt)
}

/// Runs `inject-context.sh` with a prompt in a given session. Returns stdout.
fn inject_in(url: &str, key: &str, tmp: &std::path::Path, session: &str, prompt: &str) -> String {
    let input = json!({"session_id": session, "cwd": "/tmp", "prompt": prompt});
    let mut child = Command::new("sh")
        .arg(workspace_root().join("adapters/claude-code/hooks/inject-context.sh"))
        .env("ZCIE_URL", url)
        .env("ZCIE_API_KEY", key)
        .env("ZCIE_WORKSPACE", "atheron")
        .env("TMPDIR", tmp)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(input.to_string().as_bytes()).unwrap();
    String::from_utf8(child.wait_with_output().unwrap().stdout).unwrap()
}

#[test]
fn ts_20_02_the_hook_prints_nothing_for_generic_prompts() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = DaemonProcess::spawn(&dir.path().join("home"), free_port());
    let client = daemon.client("atheron");
    for (turn, fact) in ["Plan: önce veritabanı şemasını oluştur, sonra API katmanını yaz.", "Kod incelemesini Elif yapıyor; tutarlar kuruş cinsinden i64.", "Karar: CI her commit için bütün testleri çalıştıracak."].iter().enumerate() {
        client.capture(capture_request("user.message", "s-facts", Some(&turn.to_string()), fact)).unwrap();
    }
    daemon.wait_idle();
    let prompts: Vec<String> = serde_json::from_str(GENERIC).unwrap();
    for prompt in &prompts {
        assert_eq!(inject(&daemon.url(), &daemon.admin_key(), dir.path(), prompt), "", "{prompt}");
    }
    let fact = inject(&daemon.url(), &daemon.admin_key(), dir.path(), "Kod incelemesini kim yapıyor?");
    assert!(fact.contains("Elif"), "a fact question still gets its capsule: {fact}");
}

/// Serves fixed responses: `expand` for `/v1/expand`, `query` for the other paths. Returns the URL.
fn fixed_server(query: Value, expand: Value) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut length = 0;
            let mut request_line = String::new();
            let _ = reader.read_line(&mut request_line);
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                    break;
                }
                if let Some(value) = line.to_lowercase().strip_prefix("content-length:") {
                    length = value.trim().parse().unwrap_or(0);
                }
            }
            let mut body = vec![0; length];
            let _ = reader.read_exact(&mut body);
            let text = if request_line.contains("/v1/expand") { expand.to_string() } else { query.to_string() };
            let mut stream = stream;
            let _ = write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}", text.len());
        }
    });
    url
}

#[test]
fn ts_20_03_the_best_item_is_first_and_the_second_best_is_last() {
    let names = ["alpha_one", "beta_two", "gamma_three", "delta_four", "epsilon_five"];
    let results: Vec<Value> = names.iter().enumerate().map(|(i, name)| json!({
        "ref": format!("sym:sym_{i}"), "kind": "symbol", "title": name, "path": format!("src/{name}.rs"),
        "range": [1, 3], "status": "verified", "score": 1.0 - i as f64 / 10.0, "snippet": format!("fn {name}()")
    })).collect();
    let response = json!({"class": "search", "answer_kind": "evidence", "answer": null, "results": results, "freshness": "current", "match_coverage": 0.9});
    let dir = tempfile::tempdir().unwrap();
    let expand = json!({"items": [{"ref": "sym:sym_2", "status": "ok", "content": "fn gamma_three() {\n    3\n}"}]});
    let out = inject(&fixed_server(response, expand), "key", dir.path(), "how does gamma_three connect the five parts");
    let order: Vec<&str> = out.lines().filter(|l| l.starts_with("- symbol ")).map(|l| l.trim_start_matches("- symbol ").split(' ').next().unwrap()).collect();
    assert_eq!(order, ["alpha_one", "gamma_three", "delta_four", "epsilon_five", "beta_two"], "{out}");
    // The prompt names gamma_three (not the first item): the capsule has its body.
    assert!(out.contains("source of gamma_three:\nfn gamma_three() {"), "{out}");
}

const EDIT_PROMPT: &str = "Add a cap of 50 to apply_late_fee in src/fees.rs";

/// A daemon with the ledger fixture indexed. Returns the folder, the daemon and the repository.
fn ledger_daemon() -> (tempfile::TempDir, DaemonProcess, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let daemon = DaemonProcess::spawn(&dir.path().join("home"), free_port());
    let repo = ledger_repo(dir.path());
    daemon.client("atheron").register_repository(&repo.to_string_lossy(), None).unwrap();
    daemon.wait_idle();
    (dir, daemon, repo)
}

fn change_fee(repo: &std::path::Path) {
    let text = std::fs::read_to_string(repo.join("src/fees.rs")).unwrap().replace("amount + amount *", "amount + 2 * amount *");
    std::fs::write(repo.join("src/fees.rs"), text).unwrap();
}

#[test]
fn ts_21_07_a_slice_is_sent_once_until_its_file_changes() {
    let (dir, daemon, repo) = ledger_daemon();
    let run = || inject_in(&daemon.url(), &daemon.admin_key(), dir.path(), "s-once", EDIT_PROMPT);
    let first = run();
    assert!(first.contains("source src/fees.rs:4-9") && first.contains(", current)") && first.contains("pub fn apply_late_fee"), "{first}");
    assert!(!first.contains("no Grep or Read check"), "the unconditional sentence of P4 is gone");
    assert!(!run().contains("source src/fees.rs:4-9"), "an unchanged slice is not sent again");
    change_fee(&repo);
    commit_all(&repo, "double fee");
    daemon.client("atheron").capture(capture_request("file.changed", "s-once", Some("2"), r#"{"path":"src/fees.rs"}"#)).unwrap();
    daemon.wait_idle();
    let third = run();
    assert!(third.contains("source src/fees.rs:4-9") && third.contains("2 * amount"), "a new revision is sent again: {third}");
}

#[test]
fn ts_21_08_a_pointing_prompt_gets_only_the_changed_working_set() {
    let (dir, daemon, repo) = ledger_daemon();
    let run = |prompt: &str| inject_in(&daemon.url(), &daemon.admin_key(), dir.path(), "s-fix", prompt);
    assert!(run(EDIT_PROMPT).contains("source src/fees.rs:4-9"));
    assert_eq!(run("bunu düzelt"), "", "nothing changed: no capsule");
    change_fee(&repo);
    let fix = run("bunu düzelt");
    assert!(fix.contains("changed since it was shown: src/fees.rs:4-9") && fix.contains("2 * amount"), "{fix}");
}

#[test]
fn ts_21_14_the_capsule_labels_unverified_and_assistant_facts() {
    let results = vec![
        json!({"ref": "mem:a", "kind": "memory", "title": "Karar: tutarlar f64", "status": "active", "evidence_status": "assistant_statement", "score": 0.9, "snippet": "Karar: tutarlar f64 olarak tutulacak."}),
        json!({"ref": "sym:b", "kind": "symbol", "title": "apply_charge", "path": "src/a.rs", "range": [1, 3], "status": "candidate", "score": 0.8, "snippet": "fn apply_charge()"}),
        json!({"ref": "mem:c", "kind": "memory", "title": "Rate limit 600", "status": "superseded", "score": 0.7, "snippet": "Rate limit dakikada 600 istek."}),
    ];
    let response = json!({"class": "search", "answer_kind": "evidence", "answer": null, "results": results, "freshness": "current", "match_coverage": 0.9});
    let dir = tempfile::tempdir().unwrap();
    let out = inject(&fixed_server(response, json!({"items": []})), "key", dir.path(), "tutarlar ve rate limit ne olacak");
    assert!(out.contains("[assistant statement, not confirmed by the user]"), "{out}");
    assert!(out.contains("[candidate: not verified]"), "{out}");
    assert!(out.contains("[superseded: a later correction"), "{out}");
}

/// Runs `verify-gate.sh` with a hook input. Returns stdout.
fn gate(tmp: &std::path::Path, input: Value) -> String {
    let mut child = Command::new("sh")
        .arg(workspace_root().join("adapters/claude-code/hooks/verify-gate.sh"))
        .env("TMPDIR", tmp)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(input.to_string().as_bytes()).unwrap();
    String::from_utf8(child.wait_with_output().unwrap().stdout).unwrap()
}

#[test]
fn ts_21_13_the_gate_blocks_a_turn_that_leaves_a_failing_check() {
    let dir = tempfile::tempdir().unwrap();
    // A monorepo: no Cargo.toml in the root, the crate is in a folder (the gate finds it).
    let krate = dir.path().join("mono").join("krate");
    std::fs::create_dir_all(krate.join("src")).unwrap();
    std::fs::write(krate.join("Cargo.toml"), "[package]\nname = \"krate\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[workspace]\n").unwrap();
    let lib = krate.join("src/lib.rs");
    std::fs::write(&lib, "pub fn add(a: i32, b: i32) -> i32 {\n    a + b\n").unwrap();
    let cwd = dir.path().join("mono").display().to_string();
    let stop = json!({"session_id": "s-gate", "hook_event_name": "Stop", "cwd": cwd});
    assert_eq!(gate(dir.path(), stop.clone()), "", "no edit in the turn: no check");
    gate(dir.path(), json!({"session_id": "s-gate", "hook_event_name": "PostToolUse", "cwd": cwd, "tool_name": "Edit", "tool_input": {"file_path": lib.display().to_string()}}));
    let blocked: Value = serde_json::from_str(&gate(dir.path(), stop.clone())).expect("a block decision");
    assert_eq!(blocked["decision"], "block");
    assert!(blocked["reason"].as_str().unwrap().contains("cargo check"), "{blocked}");
    std::fs::write(&lib, "pub fn add(a: i32, b: i32) -> i32 {\n    a + b\n}\n").unwrap();
    assert_eq!(gate(dir.path(), stop.clone()), "", "the fixed code passes: the turn ends");
    assert_eq!(gate(dir.path(), stop.clone()), "", "the list of changed files was cleared");
    // A Python file with a syntax error is compiled alone.
    let script = dir.path().join("mono").join("tool.py");
    std::fs::write(&script, "def run(:\n    pass\n").unwrap();
    gate(dir.path(), json!({"session_id": "s-gate", "hook_event_name": "PostToolUse", "cwd": cwd, "tool_name": "Write", "tool_input": {"file_path": script.display().to_string()}}));
    let python: Value = serde_json::from_str(&gate(dir.path(), stop.clone())).expect("a block decision");
    assert!(python["reason"].as_str().unwrap().contains("py_compile"), "{python}");
    std::fs::write(&script, "def run():\n    pass\n").unwrap();
    assert_eq!(gate(dir.path(), stop.clone()), "");
    // A change made by a Bash command (sed -i) is found through Git.
    zcie_tests::git(&dir.path().join("mono"), &["init", "-q", "-b", "main"]);
    zcie_tests::commit_all(&dir.path().join("mono"), "initial");
    std::fs::write(&lib, "pub fn add(a: i32, b: i32) -> i32 {\n    a + \n}\n").unwrap();
    gate(dir.path(), json!({"session_id": "s-gate", "hook_event_name": "PostToolUse", "cwd": cwd, "tool_name": "Bash", "tool_input": {"command": "sed -i '' 's/a + b/a + /' krate/src/lib.rs"}}));
    let bash: Value = serde_json::from_str(&gate(dir.path(), stop)).expect("a block decision after a Bash change");
    assert!(bash["reason"].as_str().unwrap().contains("cargo check"), "{bash}");
}

/// Runs `subagent-context.sh` with a hook input. Returns stdout.
fn subagent_hook(url: &str, key: &str, tmp: &std::path::Path, input: &Value) -> String {
    let mut child = Command::new("sh")
        .arg(workspace_root().join("adapters/claude-code/hooks/subagent-context.sh"))
        .env("ZCIE_URL", url)
        .env("ZCIE_API_KEY", key)
        .env("ZCIE_WORKSPACE", "atheron")
        .env("ZCIE_STATS_FILE", tmp.join("stats.jsonl"))
        .env("TMPDIR", tmp)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(input.to_string().as_bytes()).unwrap();
    String::from_utf8(child.wait_with_output().unwrap().stdout).unwrap()
}

#[test]
fn ts_24_02_a_subagent_task_gets_the_zcie_capsule() {
    let (dir, daemon, _repo) = ledger_daemon();
    let task = format!("Subagent task: {EDIT_PROMPT}");
    let agent = json!({"session_id": "s-main", "cwd": "/tmp", "tool_name": "Agent", "tool_input": {"description": "fee", "subagent_type": "general-purpose", "prompt": task}});
    let out: Value = serde_json::from_str(&subagent_hook(&daemon.url(), &daemon.admin_key(), dir.path(), &agent)).expect("an updated input");
    let prompt = out["hookSpecificOutput"]["updatedInput"]["prompt"].as_str().unwrap();
    assert!(prompt.starts_with(&task), "the task stays first");
    assert!(prompt.contains("Context from the ZCIE index") && prompt.contains("apply_late_fee"), "{prompt}");
    assert_eq!(out["hookSpecificOutput"]["updatedInput"]["subagent_type"], "general-purpose", "the other fields stay");
    assert!(std::fs::read_to_string(dir.path().join("stats.jsonl")).unwrap().contains("subagent_context"));
    let read = json!({"session_id": "s-main", "cwd": "/tmp", "tool_name": "Read", "tool_input": {"file_path": "/x"}});
    assert_eq!(subagent_hook(&daemon.url(), &daemon.admin_key(), dir.path(), &read), "");
    assert_eq!(subagent_hook("http://127.0.0.1:9", "k", dir.path(), &agent), "", "no daemon: the call runs unchanged");
}

/// A fixed answer with one good match (the capsule is injected for it).
fn capsule_response() -> Value {
    let results = vec![json!({"ref": "mem:rate", "kind": "memory", "title": "Rate limit 600", "status": "active", "score": 0.9, "snippet": "Rate limit dakikada 600 istek."})];
    json!({"class": "search", "answer_kind": "evidence", "answer": null, "results": results, "freshness": "current", "match_coverage": 0.9})
}

/// Runs `script` (a hook of `adapters/claude-code/hooks`, or the Cursor dispatcher) with a hook
/// input and extra environment. Returns stdout.
fn run_hook(script: std::path::PathBuf, input: &Value, url: &str, tmp: &std::path::Path, env: &[(&str, &str)]) -> String {
    let mut command = Command::new("sh");
    command
        .arg(script)
        .env("ZCIE_URL", url)
        .env("ZCIE_API_KEY", "key")
        .env("ZCIE_WORKSPACE", "atheron")
        .env("TMPDIR", tmp)
        .env_remove("ZCIE_APP")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    for (name, value) in env {
        command.env(name, value);
    }
    let mut child = command.spawn().unwrap();
    child.stdin.take().unwrap().write_all(input.to_string().as_bytes()).unwrap();
    String::from_utf8(child.wait_with_output().unwrap().stdout).unwrap()
}

#[test]
fn ts_25_03_cursor_prompt_gets_the_capsule_as_additional_context() {
    let dir = tempfile::tempdir().unwrap();
    // The dispatcher runs the sibling Claude Code scripts, so they are copied next to it.
    let hooks = dir.path().join("hooks");
    std::fs::create_dir_all(&hooks).unwrap();
    for entry in std::fs::read_dir(workspace_root().join("adapters/claude-code/hooks")).unwrap() {
        let path = entry.unwrap().path();
        std::fs::copy(&path, hooks.join(path.file_name().unwrap())).unwrap();
    }
    std::fs::copy(workspace_root().join("adapters/cursor/cursor-hook.sh"), hooks.join("cursor-hook.sh")).unwrap();
    let url = fixed_server(capsule_response(), json!({"items": []}));
    let prompt = |text: &str| json!({"hook_event_name": "beforeSubmitPrompt", "conversation_id": "c1", "cursor_version": "3.25.6", "workspace_roots": ["/tmp"], "prompt": text});
    let out = run_hook(hooks.join("cursor-hook.sh"), &prompt("tutarlar ve rate limit ne olacak"), &url, dir.path(), &[]);
    let value: Value = serde_json::from_str(&out).unwrap_or_else(|e| panic!("not JSON ({e}): {out}"));
    assert_eq!(value["continue"], json!(true), "{out}");
    assert!(value["additional_context"].as_str().unwrap_or_default().contains("ZCIE context"), "{out}");
    assert!(value["additional_context"].as_str().unwrap_or_default().starts_with("ZCIE: this project"), "the first prompt of a chat carries the opening: {out}");
    let generic = run_hook(hooks.join("cursor-hook.sh"), &prompt("Hello, how are you today?"), &url, dir.path(), &[]);
    assert_eq!(serde_json::from_str::<Value>(&generic).unwrap(), json!({"continue": true}), "a generic prompt gets no context: {generic}");
}

#[test]
fn ts_25_04_claude_hook_stays_silent_in_cursor_and_works_outside_it() {
    let url = fixed_server(capsule_response(), json!({"items": []}));
    let script = workspace_root().join("adapters/claude-code/hooks/inject-context.sh");
    let input = |cursor: bool| {
        let mut value = json!({"session_id": "s-guard", "cwd": "/tmp", "prompt": "tutarlar ve rate limit ne olacak"});
        if cursor {
            value["cursor_version"] = json!("3.25.6");
        }
        value
    };
    // Each run has its own TMPDIR: the first capsule of a session is not sent again in that session.
    let run = |cursor: bool, env: &[(&str, &str)]| {
        let tmp = tempfile::tempdir().unwrap();
        run_hook(script.clone(), &input(cursor), &url, tmp.path(), env)
    };
    assert_eq!(run(true, &[]), "", "Cursor also runs Claude hooks: they do nothing");
    assert!(run(false, &[]).contains("ZCIE context"), "a Claude Code session gets the capsule");
    assert!(run(true, &[("ZCIE_APP", "cursor")]).contains("ZCIE context"), "the Cursor dispatcher gets it");
}
