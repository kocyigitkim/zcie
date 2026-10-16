//! TS-12 — Weapon socket scenario, end-to-end (`docs/13-test-plan.md` section 5).
//!
//! The daemon runs as a real child process. No step calls a language model: the engine has no
//! model client (TS-12-06 holds by construction; see AC-10).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use serde_json::{Value, json};
use zcie_tests::engine::capture_request;
use zcie_tests::{DaemonProcess, commit_all, fixture_repo, free_port, mcp_session};
use zcie_types::{ExpandLevel, QueryResponse};

const WORKSPACE: &str = "atheron";
const QUESTION: &str = "what did we do for weapon attachment";

struct Scenario {
    message_event: String,
    test_event: String,
    task_ref: String,
}

fn run_steps(daemon: &DaemonProcess, repo: &std::path::Path) -> Scenario {
    let client = daemon.client(WORKSPACE);
    client.register_repository(&repo.to_string_lossy(), None).unwrap();
    daemon.wait_fixture_indexed(WORKSPACE);
    let message = capture_request("user.message", "s1", Some("1"), "Fix #89: weapon attachment moves during animation");
    let message_event = client.capture(message).unwrap().event_id;
    let file = repo.join("src/animation/weapon_socket.rs");
    let text = std::fs::read_to_string(&file).unwrap().replace("self.bone = bone.to_string();", "self.bone = bone.trim().to_string();");
    std::fs::write(&file, text).unwrap();
    let change = capture_request("file.changed", "s1", Some("1"), r#"{"path":"src/animation/weapon_socket.rs"}"#);
    client.capture(change).unwrap();
    let sha = commit_all(repo, "Fix weapon socket alignment\n\nZCIE-Task: #89");
    client.capture(capture_request("git.commit", "git", None, &json!({ "sha": sha }).to_string())).unwrap();
    daemon.wait_idle();
    let task_ref = client.query("#89", 2000).unwrap().results.into_iter().find(|r| r.kind == "task").expect("task #89").entity_ref;
    let payload = json!({ "name": "weapon-socket.integration", "status": "passed", "refs": [task_ref] }).to_string();
    let test_event = client.capture(capture_request("test.result", "s1", Some("2"), &payload)).unwrap().event_id;
    daemon.wait_idle();
    Scenario { message_event, test_event, task_ref }
}

fn kinds(response: &QueryResponse) -> Vec<String> {
    response.results.iter().map(|r| r.kind.clone()).collect()
}

#[test]
fn weapon_socket_history_is_found_after_restart_from_sdk_and_mcp() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("home");
    let repo = fixture_repo(dir.path(), true);
    let mut daemon = DaemonProcess::spawn(&home, free_port());
    let scenario = run_steps(&daemon, &repo);
    daemon.terminate();
    let daemon = DaemonProcess::spawn(&home, free_port());
    daemon.wait_idle();
    let client = daemon.client(WORKSPACE);
    let response = client.query(QUESTION, 2000).unwrap();
    check_contents(&response, &scenario);
    check_links(&client, &scenario);
    check_provenance(&client, &response);
    check_freshness(&response);
    check_reverse_path(&client, &response, &scenario);
    let mcp = mcp_session(&daemon, WORKSPACE, &[json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"zcie_query","arguments":{"question": QUESTION, "token_budget": 2000}}})]);
    let text = mcp[1]["result"]["content"][0]["text"].as_str().unwrap();
    let via_mcp: Value = serde_json::from_str(text).unwrap();
    let mcp_refs: Vec<&str> = via_mcp["results"].as_array().unwrap().iter().map(|r| r["ref"].as_str().unwrap()).collect();
    let sdk_refs: Vec<&str> = response.results.iter().map(|r| r.entity_ref.as_str()).collect();
    assert_eq!(mcp_refs, sdk_refs);
}

/// TS-12-01: task, file, symbol, commit and test are in the response.
fn check_contents(response: &QueryResponse, scenario: &Scenario) {
    let kinds = kinds(response);
    for kind in ["task", "file", "symbol", "commit"] {
        assert!(kinds.iter().any(|k| k == kind), "missing {kind}: {kinds:?}");
    }
    let test_ref = format!("ev:{}", scenario.test_event);
    let has_test = response.results.iter().any(|r| r.entity_ref == test_ref || (r.kind == "memory" && r.title.contains("weapon-socket.integration")));
    assert!(has_test, "test result missing: {kinds:?}");
    assert!(response.results.iter().any(|r| r.entity_ref == scenario.task_ref));
}

/// TS-12-02: each link has a status and evidence; the task-to-commit link is verified.
fn check_links(client: &zcie_sdk::Client, scenario: &Scenario) {
    let expanded = client.expand(std::slice::from_ref(&scenario.task_ref), ExpandLevel::Evidence, Some(2000)).unwrap();
    let content = expanded.items[0].content.clone().unwrap();
    let implements = content.lines().find(|l| l.contains("-IMPLEMENTS-> commit:")).expect("IMPLEMENTS edge");
    assert!(implements.contains("[verified commit_trailer]"), "{implements}");
    assert!(content.lines().filter(|l| l.contains("->") || l.contains("<-")).all(|l| l.contains('[') && l.contains(']')));
    assert!(content.contains("state: resolved"), "{content}");
}

/// TS-12-03: each memory item has source events.
fn check_provenance(client: &zcie_sdk::Client, response: &QueryResponse) {
    let memories: Vec<&str> = response.results.iter().filter(|r| r.kind == "memory").map(|r| r.entity_ref.as_str()).collect();
    assert!(!memories.is_empty());
    for memory in memories {
        let record = client.get_entity(memory).unwrap();
        assert!(!record["source_event_ids"].as_array().unwrap().is_empty(), "{memory}");
        assert!(record["scope"]["workspace_id"].is_string());
    }
}

/// TS-12-04: revision and freshness.
fn check_freshness(response: &QueryResponse) {
    assert_eq!(serde_json::to_value(response.freshness).unwrap(), "current");
    for item in response.results.iter().filter(|r| matches!(r.kind.as_str(), "symbol" | "file" | "commit")) {
        assert!(item.revision.as_deref().is_some_and(|r| !r.is_empty()), "{}", item.entity_ref);
    }
}

/// TS-12-05: the path from the commit back to the user message.
fn check_reverse_path(client: &zcie_sdk::Client, response: &QueryResponse, scenario: &Scenario) {
    let commit = response.results.iter().find(|r| r.kind == "commit" && r.title.contains("weapon socket")).expect("commit");
    let timeline = client.get_timeline(&commit.entity_ref).unwrap();
    assert!(timeline.items.iter().any(|i| i.event_id == scenario.message_event), "message not reached from commit");
}
