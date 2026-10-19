//! TS-15 — TypeScript and Python SDKs against a running daemon (`docs/13-test-plan.md` section 5).
//! A missing `node` or `python3` skips that SDK with a printed reason.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;
use std::process::Command;

use zcie_tests::{DaemonProcess, fixture_repo, free_port, workspace_root};

fn has(tool: &str) -> bool {
    Command::new(tool).arg("--version").output().is_ok_and(|o| o.status.success())
}

fn run_sdk_tests(daemon: &DaemonProcess, repo: &Path, folder: &str, command: &[&str]) {
    let output = Command::new(command[0])
        .args(&command[1..])
        .current_dir(workspace_root().join(folder))
        .env("ZCIE_URL", daemon.url())
        .env("ZCIE_API_KEY", daemon.admin_key())
        .env("ZCIE_TEST_REPO", repo)
        .env_remove("ZCIE_WORKSPACE")
        .output()
        .unwrap();
    let text = format!("{}\n{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
    assert!(output.status.success(), "{folder} tests failed:\n{text}");
    // A run that finds no test file also succeeds: check that tests ran.
    let expected = if folder.starts_with("adapters") { "# pass 3" } else { "# pass 4" };
    assert!(text.contains(expected) || text.contains("Ran 4 tests"), "{folder}: not all tests ran:\n{text}");
}

#[test]
fn typescript_sdk_passes_its_tests_against_the_daemon() {
    if !has("node") {
        println!("skipped: node is not installed");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let daemon = DaemonProcess::spawn(&dir.path().join("home"), free_port());
    let repo = fixture_repo(dir.path(), true);
    run_sdk_tests(&daemon, &repo, "sdk/typescript", &["node", "--test"]);
}

#[test]
fn python_sdk_passes_its_tests_against_the_daemon() {
    if !has("python3") {
        println!("skipped: python3 is not installed");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let daemon = DaemonProcess::spawn(&dir.path().join("home"), free_port());
    let repo = fixture_repo(dir.path(), true);
    run_sdk_tests(&daemon, &repo, "sdk/python", &["python3", "-m", "unittest", "discover", "-s", "tests"]);
}

#[test]
fn opencode_plugin_passes_its_tests_against_the_daemon() {
    if !has("node") {
        println!("skipped: node is not installed");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let daemon = DaemonProcess::spawn(&dir.path().join("home"), free_port());
    let repo = fixture_repo(dir.path(), true);
    run_sdk_tests(&daemon, &repo, "adapters/opencode", &["node", "--test"]);
}
