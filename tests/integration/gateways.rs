//! TS-11 — Gateways and clients, end-to-end (`docs/13-test-plan.md` section 5).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::io::Write;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use zcie_tests::engine::capture_request;
use zcie_tests::process::binary;
use zcie_tests::{DaemonProcess, fixture_repo, free_port, mcp_session, workspace_root};

const WORKSPACE: &str = "atheron";

struct Http {
    base: String,
    key: String,
    client: reqwest::blocking::Client,
}

impl Http {
    fn new(daemon: &DaemonProcess) -> Http {
        Http { base: daemon.url(), key: daemon.admin_key(), client: reqwest::blocking::Client::new() }
    }

    fn call(&self, method: &str, path: &str, body: Option<Value>, key: Option<&str>, origin: Option<&str>) -> (u16, Value) {
        let url = format!("{}{path}", self.base);
        let mut builder = if method == "GET" { self.client.get(url) } else { self.client.post(url) };
        if let Some(key) = key {
            builder = builder.bearer_auth(key);
        }
        if let Some(origin) = origin {
            builder = builder.header("Origin", origin);
        }
        if let Some(body) = body {
            builder = builder.json(&body);
        }
        let response = builder.send().unwrap();
        let status = response.status().as_u16();
        (status, response.json().unwrap_or(Value::Null))
    }

    fn ok(&self, method: &str, path: &str, body: Option<Value>) -> Value {
        let (status, value) = self.call(method, path, body, Some(&self.key.clone()), None);
        assert_eq!(status, 200, "{method} {path}: {value}");
        value
    }
}

fn enc(value: &str) -> String {
    zcie_sdk::encode_path_part(value)
}

fn started() -> (tempfile::TempDir, DaemonProcess) {
    let dir = tempfile::tempdir().unwrap();
    let daemon = DaemonProcess::spawn(&dir.path().join("home"), free_port());
    (dir, daemon)
}

#[test]
fn request_without_key_gets_401_with_error_body() {
    let (_dir, daemon) = started();
    let http = Http::new(&daemon);
    let (status, body) = http.call("POST", "/v1/query", Some(json!({"workspace": WORKSPACE, "question": "x"})), None, None);
    assert_eq!(status, 401);
    assert_eq!(body["error"]["code"], "unauthorized");
    assert!(body["error"]["message"].is_string());
}

#[test]
fn request_with_foreign_origin_gets_403() {
    let (_dir, daemon) = started();
    let http = Http::new(&daemon);
    let key = http.key.clone();
    let (status, body) = http.call("GET", "/v1/health", None, Some(&key), Some("https://evil.example.com"));
    assert_eq!(status, 403);
    assert_eq!(body["error"]["code"], "forbidden");
    let (status, _) = http.call("GET", "/v1/health", None, Some(&key), Some(&format!("http://127.0.0.1:{}", daemon.port)));
    assert_eq!(status, 200);
}

fn capture(http: &Http, event_type: &str, payload: &str) -> String {
    let request = capture_request(event_type, "s-http", Some("1"), payload);
    let value = http.ok("POST", "/v1/events", Some(serde_json::to_value(request).unwrap()));
    assert_eq!(value["durable"], true);
    value["event_id"].as_str().unwrap().to_string()
}

#[test]
fn each_route_answers_a_valid_request() {
    let (dir, daemon) = started();
    let http = Http::new(&daemon);
    let repo = fixture_repo(dir.path(), true);
    let registered = http.ok("POST", "/v1/repositories", Some(json!({"workspace": WORKSPACE, "path": repo})));
    assert!(registered["repository_id"].as_str().unwrap().starts_with("repo_"));
    daemon.wait_fixture_indexed(WORKSPACE);
    let event_id = capture(&http, "user.message", "Fix #12: socket jitter in attach_to_bone");
    daemon.wait_idle();
    let query = http.ok("POST", "/v1/query", Some(json!({"workspace": WORKSPACE, "question": "where is attach_to_bone defined"})));
    let symbol = query["results"][0]["ref"].as_str().unwrap().to_string();
    let expand = http.ok("POST", "/v1/expand", Some(json!({"workspace": WORKSPACE, "refs": [symbol], "level": "source"})));
    assert!(expand["items"][0]["content"].as_str().unwrap().contains("attach_to_bone"));
    let entity = http.ok("GET", &format!("/v1/entities/{}", enc(&format!("ev:{event_id}"))), None);
    assert_eq!(entity["event"]["event_id"], event_id.as_str());
    let task = http.ok("POST", "/v1/tasks", Some(json!({"workspace": WORKSPACE, "title": "Polish socket", "source_event_id": event_id})));
    let task_ref = format!("task:{}", task["task_id"].as_str().unwrap());
    let timeline = http.ok("GET", &format!("/v1/timeline/{}", enc(&task_ref)), None);
    assert!(timeline["items"].as_array().unwrap().iter().any(|i| i["event_id"] == event_id.as_str()));
    let link = json!({"workspace": WORKSPACE, "from": task_ref, "relation": "AFFECTS_FILE", "to": symbol, "evidence": "user_confirmation", "source_event_id": event_id});
    assert!(http.ok("POST", "/v1/links", Some(link))["edge_id"].as_str().unwrap().starts_with("edge_"));
    check_memory_forget_admin(&http, &event_id);
}

fn check_memory_forget_admin(http: &Http, event_id: &str) {
    let entity = http.ok("GET", &format!("/v1/timeline/{}", enc(&format!("ev:{event_id}"))), None);
    assert!(entity["items"].is_array());
    let query = http.ok("POST", "/v1/query", Some(json!({"workspace": WORKSPACE, "question": "jitter", "mode": "memory", "token_budget": 2000})));
    let memory = query["results"].as_array().unwrap().iter().find(|r| r["kind"] == "memory").expect("memory result");
    let memory_id = memory["ref"].as_str().unwrap().trim_start_matches("mem:").to_string();
    let corrected = http.ok("POST", &format!("/v1/memory/{memory_id}/correct"), Some(json!({"text": "Fix socket jitter only", "source_event_id": event_id})));
    assert_eq!(corrected["text"], "Fix socket jitter only");
    let health = http.ok("GET", "/v1/health", None);
    assert!(health["watermarks"]["canonical_seq"].as_u64().unwrap() >= 1);
    assert!(http.ok("POST", "/v1/admin/rebuild-search", None)["job_id"].as_str().unwrap().starts_with("job_"));
    let forget = http.ok("POST", "/v1/forget", Some(json!({"workspace": WORKSPACE, "selector": {"ref": format!("ev:{event_id}")}, "reason": "test"})));
    assert_eq!(forget["events_removed"], 1);
}

#[test]
fn sdk_query_returns_the_same_results_as_http() {
    let (dir, daemon) = started();
    let repo = fixture_repo(dir.path(), true);
    let client = daemon.client(WORKSPACE);
    client.register_repository(&repo.to_string_lossy(), None).unwrap();
    daemon.wait_fixture_indexed(WORKSPACE);
    let http = Http::new(&daemon);
    let sdk = client.query("where is build_report defined", 250).unwrap();
    let raw = http.ok("POST", "/v1/query", Some(json!({"workspace": WORKSPACE, "question": "where is build_report defined", "token_budget": 250})));
    let sdk_refs: Vec<String> = sdk.results.iter().map(|r| r.entity_ref.clone()).collect();
    let raw_refs: Vec<String> = raw["results"].as_array().unwrap().iter().map(|r| r["ref"].as_str().unwrap().to_string()).collect();
    assert!(!sdk_refs.is_empty());
    assert_eq!(sdk_refs, raw_refs);
    assert_eq!(sdk.answer.as_deref(), raw["answer"].as_str());
}

#[test]
fn mcp_gateway_lists_two_tools_and_answers_like_http() {
    let (dir, daemon) = started();
    let repo = fixture_repo(dir.path(), true);
    daemon.client(WORKSPACE).register_repository(&repo.to_string_lossy(), None).unwrap();
    daemon.wait_fixture_indexed(WORKSPACE);
    let calls = [
        json!({"jsonrpc":"2.0","id":1,"method":"tools/list"}),
        json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"zcie_query","arguments":{"question":"where is attach_to_bone defined"}}}),
    ];
    let responses = mcp_session(&daemon, WORKSPACE, &calls);
    let mut tools: Vec<&str> = responses[1]["result"]["tools"].as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap()).collect();
    tools.sort();
    assert_eq!(tools, ["zcie_expand", "zcie_query"]);
    let text = responses[2]["result"]["content"][0]["text"].as_str().unwrap();
    let mcp: Value = serde_json::from_str(text).unwrap();
    let http = daemon.client(WORKSPACE).query("where is attach_to_bone defined", 250).unwrap();
    assert_eq!(mcp["answer"].as_str(), http.answer.as_deref());
    assert_eq!(mcp["results"][0]["ref"].as_str(), Some(http.results[0].entity_ref.as_str()));
}

#[test]
fn claude_code_hooks_exit_zero_when_daemon_is_down() {
    let hooks = workspace_root().join("adapters/claude-code/hooks");
    let inputs = [
        ("session-start.sh", json!({"session_id":"s","cwd":"/tmp","hook_event_name":"SessionStart"})),
        ("user-prompt-submit.sh", json!({"session_id":"s","prompt":"hello","cwd":"/tmp"})),
        ("post-tool-use.sh", json!({"session_id":"s","tool_name":"Edit","tool_input":{"file_path":"/tmp/a.rs"},"cwd":"/tmp"})),
        ("stop.sh", json!({"session_id":"s","cwd":"/tmp"})),
    ];
    let closed = format!("http://127.0.0.1:{}", free_port());
    for (script, input) in inputs {
        let started = Instant::now();
        let mut child = Command::new("sh")
            .arg(hooks.join(script))
            .env("ZCIE_URL", &closed)
            .env("TMPDIR", std::env::temp_dir())
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        child.stdin.take().unwrap().write_all(input.to_string().as_bytes()).unwrap();
        let status = child.wait().unwrap();
        assert!(status.success(), "{script} failed");
        assert!(started.elapsed() < Duration::from_secs(3), "{script} took too long");
    }
}

#[test]
fn cli_query_prints_json() {
    let (_dir, daemon) = started();
    let request = capture_request("user.message", "s-cli", Some("1"), "the pendulum sways");
    daemon.client(WORKSPACE).capture(request).unwrap();
    daemon.wait_idle();
    let output = Command::new(binary("zcie"))
        .args(["--url", &daemon.url(), "--key", &daemon.admin_key(), "--workspace", WORKSPACE, "--home"])
        .arg(&daemon.home)
        .args(["query", "pendulum"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(value["results"].as_array().is_some_and(|r| !r.is_empty()));
}
