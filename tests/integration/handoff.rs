//! TS-33 — Session handoff (ADR-033) and subagent notes: the PreCompact and SessionStart note of `session-handoff.sh`
//! and the new-topic message of `session-guard.sh`. Each test runs the script with a fake transcript.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::io::Write;
use std::process::{Command, Stdio};

use serde_json::{Value, json};
use zcie_tests::workspace_root;

/// Runs `script` with a hook input and extra environment. Returns stdout.
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

/// A hook script of `adapters/claude-code/hooks`.
fn hook(name: &str) -> std::path::PathBuf {
    workspace_root().join("adapters/claude-code/hooks").join(name)
}

/// Writes a transcript: two user prompts (one is an image), an edit of src/fees.rs, a sidechain
/// edit of another file that must be ignored, and a last answer. The main-thread usage reads
/// `cache_read` tokens.
fn write_transcript(dir: &std::path::Path, cache_read: u64) -> std::path::PathBuf {
    let usage = |read: u64| json!({"input_tokens": 10, "cache_read_input_tokens": read, "cache_creation_input_tokens": 500, "output_tokens": 5});
    let records = [
        json!({"type": "user", "isSidechain": false, "message": {"role": "user", "content": "Fix the late fee rounding in src/fees.rs"}}),
        json!({"type": "user", "isSidechain": false, "message": {"role": "user", "content": "[Image #1] see this"}}),
        json!({"type": "assistant", "isSidechain": false, "message": {"role": "assistant", "usage": usage(cache_read), "content": [
            {"type": "text", "text": "Rounding fixed."},
            {"type": "tool_use", "name": "Edit", "input": {"file_path": "/proj/src/fees.rs"}}]}}),
        json!({"type": "assistant", "isSidechain": true, "message": {"role": "assistant", "usage": usage(5), "content": [
            {"type": "text", "text": "SIDECHAIN ANSWER"},
            {"type": "tool_use", "name": "Edit", "input": {"file_path": "/proj/src/other.rs"}}]}}),
        json!({"type": "user", "isSidechain": false, "message": {"role": "user", "content": "Now add a test for it"}}),
        json!({"type": "assistant", "isSidechain": false, "message": {"role": "assistant", "usage": usage(cache_read), "content": [
            {"type": "text", "text": "Added the test."}]}}),
    ];
    let path = dir.join("session.jsonl");
    let text: String = records.iter().map(|r| format!("{r}\n")).collect();
    std::fs::write(&path, text).unwrap();
    path
}

#[test]
fn ts_27_01_precompact_writes_the_handoff_of_the_session() {
    let dir = tempfile::tempdir().unwrap();
    let transcript = write_transcript(dir.path(), 200_000);
    let handoff = dir.path().join("handoff").display().to_string();
    let input = json!({"hook_event_name": "PreCompact", "session_id": "abcdef123456", "transcript_path": transcript.display().to_string(), "cwd": "/proj"});
    let out = run_hook(hook("session-handoff.sh"), &input, "http://127.0.0.1:9", dir.path(), &[("ZCIE_HANDOFF_DIR", handoff.as_str())]);
    assert!(out.is_empty(), "PreCompact prints nothing: {out}");
    let note = std::fs::read_to_string(dir.path().join("handoff/atheron.md")).unwrap();
    assert!(note.starts_with("ZCIE handoff from the previous session (abcdef12, "), "{note}");
    assert!(note.contains("- Fix the late fee rounding in src/fees.rs\n- Now add a test for it\n"), "{note}");
    assert!(!note.contains("[Image"), "image prompts are skipped: {note}");
    assert!(note.contains("Files changed:\n- src/fees.rs\n"), "relative to the cwd: {note}");
    assert!(!note.contains("other.rs") && !note.contains("SIDECHAIN"), "sidechains are ignored: {note}");
    assert!(note.ends_with("Last answer:\nAdded the test.\n"), "{note}");
}

#[test]
fn ts_27_02_after_a_clear_another_sessions_handoff_prints_once() {
    let dir = tempfile::tempdir().unwrap();
    let transcript = write_transcript(dir.path(), 200_000);
    let handoff = dir.path().join("handoff").display().to_string();
    let env = [("ZCIE_HANDOFF_DIR", handoff.as_str())];
    let event = |source: &str, session: &str| json!({"hook_event_name": "SessionStart", "source": source, "session_id": session, "cwd": "/proj", "transcript_path": transcript.display().to_string()});
    let pre = json!({"hook_event_name": "PreCompact", "session_id": "abcdef123456", "transcript_path": transcript.display().to_string(), "cwd": "/proj"});
    run_hook(hook("session-handoff.sh"), &pre, "http://127.0.0.1:9", dir.path(), &env);
    let url = "http://127.0.0.1:9";
    let first = run_hook(hook("session-handoff.sh"), &event("clear", "newsession99"), url, dir.path(), &env);
    assert!(first.starts_with("ZCIE handoff from the previous session (abcdef12"), "{first}");
    assert!(run_hook(hook("session-handoff.sh"), &event("clear", "newsession99"), url, dir.path(), &env).is_empty(), "once per session");
    assert!(run_hook(hook("session-handoff.sh"), &event("clear", "abcdef123456"), url, dir.path(), &env).is_empty(), "not the note of the same session");
}

#[test]
fn ts_27_03_a_new_topic_in_a_long_session_gets_the_handoff_message() {
    let dir = tempfile::tempdir().unwrap();
    let transcript = write_transcript(dir.path(), 200_000);
    let handoff = dir.path().join("handoff").display().to_string();
    let env = [("ZCIE_HANDOFF_DIR", handoff.as_str())];
    let prompt = |text: &str| json!({"session_id": "abcdef123456", "transcript_path": transcript.display().to_string(), "cwd": "/proj", "prompt": text});
    let url = "http://127.0.0.1:9";
    let topic = run_hook(hook("session-guard.sh"), &prompt("Write a poem about the weather today"), url, dir.path(), &env);
    assert!(topic.contains("new topic") && topic.contains("200k tokens"), "{topic}");
    assert!(wait_for_note(&dir.path().join("handoff/atheron.md")), "the handoff is written in the background");
    // Another folder: a prompt that names the changed file gets the plain size message.
    let other = tempfile::tempdir().unwrap();
    let named = run_hook(hook("session-guard.sh"), &prompt("Please fix the rounding in fees.rs"), url, other.path(), &env);
    assert!(named.contains("this session is about 200k tokens") && !named.contains("new topic"), "{named}");
}

#[test]
fn ts_27_04_a_session_of_50k_tokens_gets_no_message() {
    let dir = tempfile::tempdir().unwrap();
    let transcript = write_transcript(dir.path(), 50_000);
    let input = json!({"session_id": "abcdef123456", "transcript_path": transcript.display().to_string(), "cwd": "/proj", "prompt": "Write a poem about the weather today"});
    let out = run_hook(hook("session-guard.sh"), &input, "http://127.0.0.1:9", dir.path(), &[]);
    assert!(out.is_empty(), "{out}");
}

#[test]
fn ts_27_05_a_finished_subagent_leaves_notes_for_the_next_one() {
    let dir = tempfile::tempdir().unwrap();
    let agent = dir.path().join("agent.jsonl");
    let records = [
        json!({"type": "assistant", "message": {"role": "assistant", "content": [{"type": "tool_use", "name": "Read", "input": {"file_path": "/proj/src/fees.rs", "offset": 10, "limit": 20}}]}}),
        json!({"type": "assistant", "message": {"role": "assistant", "content": [{"type": "tool_use", "name": "Bash", "input": {"command": "sed -n '1,40p' /proj/src/rounding.rs | head"}}]}}),
        json!({"type": "assistant", "message": {"role": "assistant", "content": [{"type": "text", "text": "Found it."}]}}),
    ];
    let text: String = records.iter().map(|r| format!("{r}\n")).collect();
    std::fs::write(&agent, text).unwrap();
    let url = "http://127.0.0.1:9";
    let stop = json!({"hook_event_name": "SubagentStop", "session_id": "s-main", "cwd": "/proj", "agent_type": "general-purpose", "agent_id": "a1",
        "agent_transcript_path": agent.display().to_string(), "last_assistant_message": "Rounding uses f64.\n\nSecond line.\nThird line.\nFourth line is cut."});
    assert_eq!(run_hook(hook("subagent-context.sh"), &stop, url, dir.path(), &[]), "", "SubagentStop prints nothing");
    let queued = json!({"hook_event_name": "PreToolUse", "session_id": "s-main", "cwd": "/proj", "tool_name": "Agent", "tool_input": {"prompt": "Check the fee rounding"}});
    assert_eq!(run_hook(hook("subagent-context.sh"), &queued, url, dir.path(), &[]), "");
    // The daemon is not running: no capsule, but the notes and the report line.
    let start = json!({"hook_event_name": "SubagentStart", "session_id": "s-main", "cwd": "/proj", "agent_id": "a2", "agent_type": "general-purpose"});
    let out = run_hook(hook("subagent-context.sh"), &start, url, dir.path(), &[]);
    let value: Value = serde_json::from_str(&out).expect("the notes context");
    let context = value["hookSpecificOutput"]["additionalContext"].as_str().unwrap();
    assert!(context.contains("Earlier subagents of this session"), "{context}");
    assert!(context.contains("read src/fees.rs:10-29, src/rounding.rs:1-40"), "{context}");
    assert!(context.contains("found: Rounding uses f64. Second line. Third line."), "{context}");
    assert!(!context.contains("Fourth") && !context.contains("Context from the ZCIE index"), "{context}");
    assert!(context.ends_with("End with a short report: at most 15 lines; write details into files, not into the report."), "{context}");
}

#[test]
fn ts_27_06_the_guard_returns_at_once_on_a_large_transcript_and_the_note_follows() {
    let dir = tempfile::tempdir().unwrap();
    // About 60 MB of tool results, then the small records of ts_27_01 (the last usage is 200k tokens).
    let big = format!("{}\n", json!({"type": "user", "isSidechain": false, "message": {"role": "user", "content": [{"type": "tool_result", "content": "x".repeat(1_000_000)}]}}));
    let transcript = dir.path().join("big.jsonl");
    let small = std::fs::read_to_string(write_transcript(dir.path(), 200_000)).unwrap();
    std::fs::write(&transcript, big.repeat(60) + &small).unwrap();
    let handoff = dir.path().join("handoff").display().to_string();
    let env = [("ZCIE_HANDOFF_DIR", handoff.as_str())];
    let prompt = json!({"session_id": "abcdef123456", "transcript_path": transcript.display().to_string(), "cwd": "/proj", "prompt": "Write a poem about the weather today"});
    let started = std::time::Instant::now();
    let out = run_hook(hook("session-guard.sh"), &prompt, "http://127.0.0.1:9", dir.path(), &env);
    let took = started.elapsed();
    assert!(took < std::time::Duration::from_secs(2), "the guard took {took:?}");
    assert!(out.contains("new topic"), "{out}");
    let note = dir.path().join("handoff/atheron.md");
    assert!(wait_for_note(&note), "the background writer wrote the note");
    assert!(std::fs::read_to_string(&note).unwrap().contains("Fix the late fee rounding in src/fees.rs"));
}

/// Waits up to 10 s for a note that the detached writer creates. Returns whether it exists.
fn wait_for_note(path: &std::path::Path) -> bool {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !path.exists() && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    path.exists()
}
