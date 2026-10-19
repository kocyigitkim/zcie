//! TS-22 — Token savings report (ADR-022). TS-22-04 runs `zcie usage` on fixture transcripts;
//! TS-22-05 checks the statistics lines of the read guard and the verification gate.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::io::Write;
use std::process::{Command, Stdio};

use serde_json::{Value, json};
use zcie_tests::process::binary;
use zcie_tests::workspace_root;

/// Writes one session of `turns` turns with `tokens` input tokens for each turn.
fn write_session(projects: &std::path::Path, id: &str, cwd: &str, zcie: bool, tokens: u64, turns: u64) {
    let folder = projects.join(cwd.replace('/', "-"));
    std::fs::create_dir_all(&folder).unwrap();
    let time = "2026-10-18T10:00:00Z";
    let mut lines = Vec::new();
    for turn in 0..turns {
        lines.push(json!({"type": "user", "cwd": cwd, "entrypoint": "cli", "timestamp": time, "message": {"content": format!("task {turn}")}}));
        // The capsule of the context hook comes right after the message that it answers.
        if zcie {
            lines.push(json!({"type": "attachment", "timestamp": time, "attachment": {"command": "/h/.zcie/hooks/inject-context.sh", "content": "ZCIE context (x)"}}));
        }
        let usage = json!({"input_tokens": tokens, "cache_creation_input_tokens": 0, "cache_read_input_tokens": 0, "output_tokens": 0});
        lines.push(json!({"type": "assistant", "cwd": cwd, "timestamp": time, "message": {"id": format!("{id}-{turn}"), "usage": usage, "content": []}}));
    }
    let text: String = lines.iter().map(|l| format!("{l}\n")).collect();
    std::fs::write(folder.join(format!("{id}.jsonl")), text).unwrap();
}

fn usage(home: &std::path::Path, projects: &std::path::Path, json: bool) -> String {
    let mut command = Command::new(binary("zcie"));
    command.args(["--home", &home.display().to_string(), "usage", "--days", "36500", "--dir", &projects.display().to_string()]);
    if json {
        command.arg("--json");
    }
    let out = command.output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8(out.stdout).unwrap()
}

#[test]
fn ts_22_04_zcie_usage_reports_the_saving_of_each_project() {
    let dir = tempfile::tempdir().unwrap();
    let (home, projects) = (dir.path().join("home"), dir.path().join("projects"));
    // No outlier: the largest cost per turn (2000) is below 3 × the median of all sessions (750).
    for (i, tokens) in [1000, 1500, 2000].into_iter().enumerate() {
        write_session(&projects, &format!("z{i}"), "/work/ledger", true, tokens / 4, 2);
        write_session(&projects, &format!("p{i}"), "/work/ledger", false, tokens, 1);
    }
    write_session(&projects, "q0", "/work/notes", false, 50, 1);
    std::fs::create_dir_all(home.join("stats")).unwrap();
    std::fs::write(home.join("stats/hooks.jsonl"), "{\"ts\":\"2026-10-19T08:00:00Z\",\"kind\":\"gate\",\"cwd\":\"/work/ledger\",\"result\":\"pass\"}\n").unwrap();
    let report: Value = serde_json::from_str(&usage(&home, &projects, true)).unwrap();
    let ledger = report["projects"].as_array().unwrap().iter().find(|p| p["name"] == "ledger").expect("ledger");
    assert_eq!(ledger["zcie"]["tokens_per_turn"], 375.0);
    assert_eq!(ledger["plain"]["tokens_per_turn"], 1500.0);
    assert_eq!(ledger["saving"]["percent"], 75.0);
    assert_eq!(ledger["saving"]["tokens"], 6750, "6 turns × (1500 − 375)");
    assert_eq!((ledger["saving"]["excluded"].as_u64(), ledger["saving"]["confidence"].as_str()), (Some(0), Some("medium")));
    assert_eq!(ledger["sessions"].as_array().unwrap().len(), 6);
    assert_eq!(ledger["gate"]["pass"], 1);
    assert_eq!(ledger["zcie"]["capsules"], 6, "one capsule for each of the 6 turns");
    assert!(home.join("run/usage-cache.json").exists(), "the cache is written");
    let text = usage(&home, &projects, false);
    assert!(text.contains("ledger") && text.contains("-75%") && text.contains("observed"), "{text}");
    assert!(text.contains("no sessions with ZCIE"), "notes has no ZCIE session: {text}");
}

/// Runs a hook script with an input and a statistics file. Returns stdout.
fn hook(script: &str, input: &Value, tmp: &std::path::Path, stats: Option<&std::path::Path>) -> String {
    let mut command = Command::new("sh");
    command.arg(workspace_root().join("adapters/claude-code/hooks").join(script)).env("TMPDIR", tmp).env("ZCIE_URL", "http://127.0.0.1:9");
    command.env_remove("ZCIE_STATS_FILE");
    if let Some(stats) = stats {
        command.env("ZCIE_STATS_FILE", stats);
    }
    let mut child = command.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().unwrap();
    child.stdin.take().unwrap().write_all(input.to_string().as_bytes()).unwrap();
    String::from_utf8(child.wait_with_output().unwrap().stdout).unwrap()
}

#[test]
fn ts_22_05_the_read_guard_and_the_gate_record_statistics() {
    let dir = tempfile::tempdir().unwrap();
    let stats = dir.path().join("stats/hooks.jsonl");
    let big = dir.path().join("big.txt");
    std::fs::write(&big, "secret line\n".repeat(900)).unwrap();
    let cwd = dir.path().display().to_string();
    let read = json!({"session_id": "s1", "cwd": cwd, "tool_name": "Read", "tool_input": {"file_path": big.display().to_string()}});
    assert!(hook("read-guard.sh", &read, dir.path(), Some(&stats)).contains("\"limit\": 250"));
    let script = dir.path().join("tool.py");
    std::fs::write(&script, "def run(:\n    pass\n").unwrap();
    let edit = json!({"session_id": "s1", "hook_event_name": "PostToolUse", "cwd": cwd, "tool_name": "Write", "tool_input": {"file_path": script.display().to_string()}});
    let stop = json!({"session_id": "s1", "hook_event_name": "Stop", "cwd": cwd});
    hook("verify-gate.sh", &edit, dir.path(), Some(&stats));
    assert!(hook("verify-gate.sh", &stop, dir.path(), Some(&stats)).contains("block"));
    std::fs::write(&script, "def run():\n    pass\n").unwrap();
    assert_eq!(hook("verify-gate.sh", &stop, dir.path(), Some(&stats)), "");
    let text = std::fs::read_to_string(&stats).unwrap();
    assert!(!text.contains("secret"), "no file content in the statistics");
    let lines: Vec<Value> = text.lines().map(|l| serde_json::from_str(l).unwrap()).collect();
    let kinds: Vec<(String, String)> = lines.iter().map(|l| (l["kind"].as_str().unwrap().into(), l["result"].as_str().unwrap_or("").into())).collect();
    assert_eq!(kinds, [("read_guard".into(), "".into()), ("gate".into(), "block".into()), ("gate".into(), "pass".into())]);
    assert_eq!((lines[0]["lines"].as_u64(), lines[0]["read"].as_u64()), (Some(900), Some(250)));
    assert_eq!(lines[0]["session"], "s1");
    // An image or another binary file is not limited and not counted.
    let image = dir.path().join("shot.png");
    std::fs::write(&image, b"\x89PNG\r\n\x1a\n\0".repeat(600)).unwrap();
    let binary_file = dir.path().join("blob.dat");
    std::fs::write(&binary_file, b"\0\x01\n".repeat(600)).unwrap();
    for path in [&image, &binary_file] {
        let input = json!({"session_id": "s1", "cwd": cwd, "tool_name": "Read", "tool_input": {"file_path": path.display().to_string()}});
        assert_eq!(hook("read-guard.sh", &input, dir.path(), Some(&stats)), "", "{}", path.display());
    }
    assert_eq!(std::fs::read_to_string(&stats).unwrap().lines().count(), 3);
    // A hook that runs from the repository (no ZCIE home with a config folder) writes nothing.
    hook("read-guard.sh", &read, dir.path(), None);
    assert!(!workspace_root().join("adapters/claude-code/stats").exists());
}

/// TS-23-01: the daemon writes one activity line for each query, expand, capture and job.
#[test]
fn ts_23_01_the_daemon_logs_reads_and_writes() {
    let engine = zcie_tests::TestEngine::start();
    let repo = zcie_tests::ledger_repo(engine.dir.path());
    engine.register(&repo);
    // The engine opens the log at startup, in `<home>/stats`.
    let log = engine.engine().ingest.counters.activity.get().expect("the activity log is open").path().to_path_buf();
    engine.capture("user.message", "s1", None, "fees are rounded down");
    engine.wait_idle();
    let _ = engine.query("fee rounding", 400);
    let text = std::fs::read_to_string(&log).expect("the activity log");
    let ops: Vec<Value> = text.lines().map(|l| serde_json::from_str(l).unwrap()).collect();
    let has = |op: &str| ops.iter().any(|o| o["op"] == op && o["ts"].as_str().is_some() && o["ms"].as_f64().is_some());
    assert!(has("capture") && has("query") && has("job"), "{text}");
    let query = ops.iter().find(|o| o["op"] == "query").unwrap();
    assert_eq!(query["question"], "fee rounding");
    assert!(query["results"].as_u64().is_some() && query["tokens"].as_u64().is_some());
}
