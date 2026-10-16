//! TS-09 — Crash recovery (`docs/13-test-plan.md` section 5). The daemon runs as a child process.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::process::Command;
use std::time::{Duration, Instant};

use zcie_tests::engine::capture_request;
use zcie_tests::process::binary;
use zcie_tests::{DaemonProcess, fixture_repo, free_port};

const WORKSPACE: &str = "atheron";

fn poll(mut check: impl FnMut() -> bool, what: &str) {
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(30) {
        if check() {
            return;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    panic!("timeout: {what}");
}

fn symbol_count(daemon: &DaemonProcess, name: &str) -> usize {
    let response = daemon.client(WORKSPACE).query(&format!("where is {name} defined"), 2000).unwrap();
    response.results.iter().filter(|r| r.kind == "symbol" && r.title.ends_with(name)).count()
}

#[test]
fn captured_events_survive_repeated_kill_and_restart() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("home");
    let mut daemon = DaemonProcess::spawn(&home, free_port());
    let mut ids = Vec::new();
    for cycle in 0..10 {
        for index in 0..10 {
            let n = cycle * 10 + index;
            let mut request = capture_request("user.message", "s-crash", Some(&n.to_string()), &format!("message {n}"));
            request.idempotency_key = format!("crash:{n}");
            let receipt = daemon.client(WORKSPACE).capture(request).unwrap();
            assert!(receipt.durable);
            ids.push(receipt.event_id);
        }
        daemon.kill9();
        daemon = DaemonProcess::spawn(&home, free_port());
    }
    let client = daemon.client(WORKSPACE);
    for id in &ids {
        client.get_entity(&format!("ev:{id}")).unwrap();
    }
    let watermarks = client.health().unwrap().watermarks.unwrap();
    assert_eq!(watermarks.canonical_seq, 100);
}

#[test]
fn killed_jobs_run_again_without_duplicates() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("home");
    let repo = fixture_repo(dir.path(), true);
    let mut daemon = DaemonProcess::spawn(&home, free_port());
    daemon.client(WORKSPACE).register_repository(&repo.to_string_lossy(), None).unwrap();
    daemon.kill9();
    let daemon = DaemonProcess::spawn(&home, free_port());
    poll(|| symbol_count(&daemon, "attach_to_bone") >= 1, "symbol indexed after restart");
    daemon.wait_idle();
    assert_eq!(symbol_count(&daemon, "attach_to_bone"), 1);
    assert_eq!(daemon.client(WORKSPACE).health().unwrap().queue.unwrap().dead_letter, 0);
}

#[test]
fn search_watermark_catches_up_after_restart() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("home");
    let mut daemon = DaemonProcess::spawn(&home, free_port());
    for n in 0..20 {
        let request = capture_request("user.message", "s-search", Some(&n.to_string()), &format!("quaternion note {n}"));
        daemon.client(WORKSPACE).capture(request).unwrap();
    }
    daemon.kill9();
    let daemon = DaemonProcess::spawn(&home, free_port());
    let health = daemon.wait_idle();
    let wm = health.watermarks.unwrap();
    assert_eq!(wm.canonical_seq, 20);
    assert!(wm.search_seq >= wm.canonical_seq);
    let response = daemon.client(WORKSPACE).query("quaternion", 2000).unwrap();
    assert!(!response.results.is_empty());
}

#[test]
fn corrupted_search_index_is_rebuilt_at_startup() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("home");
    let mut daemon = DaemonProcess::spawn(&home, free_port());
    let request = capture_request("user.message", "s-corrupt", Some("1"), "the gyroscope drifts during calibration");
    daemon.client(WORKSPACE).capture(request).unwrap();
    daemon.wait_idle();
    daemon.terminate();
    let search = home.join("instances/local/state/search");
    for index in ["code", "documents", "conversations"] {
        std::fs::write(search.join(index).join("meta.json"), b"{ not json").unwrap();
    }
    let daemon = DaemonProcess::spawn(&home, free_port());
    poll(
        || !daemon.client(WORKSPACE).query("gyroscope", 2000).unwrap().results.is_empty(),
        "query works after rebuild",
    );
}

#[test]
fn replayed_idempotency_keys_after_restart_are_duplicates() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("home");
    let mut daemon = DaemonProcess::spawn(&home, free_port());
    let mut requests = Vec::new();
    for n in 0..5 {
        let mut request = capture_request("user.message", "s-replay", Some(&n.to_string()), &format!("replay {n}"));
        request.idempotency_key = format!("replay:{n}");
        daemon.client(WORKSPACE).capture(request.clone()).unwrap();
        requests.push(request);
    }
    daemon.kill9();
    let daemon = DaemonProcess::spawn(&home, free_port());
    for request in requests {
        let receipt = daemon.client(WORKSPACE).capture(request).unwrap();
        assert!(receipt.duplicate);
    }
    assert_eq!(daemon.client(WORKSPACE).health().unwrap().watermarks.unwrap().canonical_seq, 5);
}

#[test]
fn second_daemon_on_same_home_stops_with_lock_error() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("home");
    let _daemon = DaemonProcess::spawn(&home, free_port());
    let output = Command::new(binary("zcied"))
        .arg("--home")
        .arg(&home)
        .env("ZCIE_SERVER_HTTP_ADDR", format!("\"127.0.0.1:{}\"", free_port()))
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("daemon.lock"), "stderr: {stderr}");
}
