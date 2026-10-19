//! TS-22-01 to TS-22-03 (ADR-022): session records from fixture transcripts, the incremental scan,
//! and the savings report.

use std::io::Write;
use std::path::Path;

use serde_json::{Value, json};

use super::report::{self, Root, Stat};
use super::scan::{self, Session};

fn user(text: &str, time: &str) -> Value {
    json!({"type": "user", "cwd": "/work/alpha", "entrypoint": "cli", "timestamp": time, "message": {"role": "user", "content": text}})
}

/// An assistant record of message `id` with one content part; the usage repeats on each part.
fn assistant(id: &str, part: Value, time: &str) -> Value {
    let usage = json!({"input_tokens": 10, "cache_creation_input_tokens": 100, "cache_read_input_tokens": 1000, "output_tokens": 5});
    json!({"type": "assistant", "cwd": "/work/alpha", "timestamp": time, "message": {"id": id, "usage": usage, "content": [part]}})
}

fn tool(name: &str, input: Value) -> Value {
    json!({"type": "tool_use", "name": name, "input": input})
}

/// The records of the main fixture session.
fn main_records() -> Vec<Value> {
    let t = "2026-10-18T10:00:00Z";
    vec![
        user("explain the fee rule", t),
        json!({"type": "attachment", "timestamp": t, "attachment": {"type": "hook_success", "command": "/Users/u/.zcie/hooks/inject-context.sh", "content": "ZCIE context (local index)\n- fee.rs"}}),
        assistant("m1", json!({"type": "text", "text": "Reading."}), t),
        assistant("m1", tool("Read", json!({"file_path": "/work/alpha/fee.rs"})), t),
        assistant("m1", tool("Bash", json!({"command": "grep -rn fee ."})), t),
        json!({"type": "user", "timestamp": t, "message": {"content": [{"type": "tool_result", "content": "ok"}]}}),
        json!({"type": "user", "isMeta": true, "timestamp": t, "message": {"content": "meta text"}}),
        user("<task-notification>done</task-notification>", t),
        assistant("m2", tool("Grep", json!({"pattern": "fee"})), "2026-10-19T09:00:00Z"),
        user("now fix it", "2026-10-19T09:00:00Z"),
    ]
}

fn write_lines(path: &Path, records: &[Value], append: bool) {
    std::fs::create_dir_all(path.parent().expect("a parent")).expect("folder");
    let mut file = std::fs::OpenOptions::new().create(true).append(append).write(true).truncate(!append).open(path).expect("open");
    for record in records {
        writeln!(file, "{record}").expect("write");
    }
}

#[test]
fn ts_22_01_a_session_counts_turns_tokens_tools_and_capsules() {
    let dir = tempfile::tempdir().expect("tmp");
    let projects = dir.path().join("projects");
    write_lines(&projects.join("-work-alpha/s1.jsonl"), &main_records(), false);
    write_lines(&projects.join("-work-alpha/s1/subagents/agent-a.jsonl"), &[assistant("m9", tool("Glob", json!({"pattern": "*.rs"})), "2026-10-19T09:01:00Z")], false);
    let mut automated = vec![user("benchmark task", "2026-10-18T11:00:00Z")];
    automated[0]["entrypoint"] = json!("sdk-cli");
    write_lines(&projects.join("-work-alpha/s2.jsonl"), &automated, false);
    let sessions = scan::scan(&projects, &dir.path().join("cache.json")).expect("scan");
    let s1 = &sessions["s1"];
    assert_eq!((s1.turns, s1.reads, s1.searches), (2, 1, 3), "2 typed messages; Read; grep, Grep, Glob");
    assert_eq!((s1.input, s1.cache_write, s1.cache_read, s1.output), (30, 300, 3000, 15), "m1, m2 and m9 once each");
    assert!(s1.zcie && !s1.automated);
    assert_eq!(s1.capsules, 1);
    assert_eq!(s1.capsule_chars, "ZCIE context (local index)\n- fee.rs".chars().count() as u64);
    assert_eq!((s1.first.as_str(), s1.last.as_str()), ("2026-10-18T10:00:00Z", "2026-10-19T09:01:00Z"));
    assert_eq!(s1.days["2026-10-18"].tokens, 1115);
    assert_eq!(s1.days["2026-10-19"].turns, 1);
    assert!(sessions["s2"].automated, "a program started s2");
}

#[test]
fn ts_22_02_an_incremental_scan_equals_a_full_scan() {
    let dir = tempfile::tempdir().expect("tmp");
    let projects = dir.path().join("projects");
    let file = projects.join("-work-alpha/s1.jsonl");
    let records = main_records();
    write_lines(&file, &records[..4], false);
    // A line that the client is still writing (no newline) waits for the next scan.
    let mut open = std::fs::OpenOptions::new().append(true).open(&file).expect("open");
    let half = records[4].to_string();
    write!(open, "{}", &half[..20]).expect("write");
    let cache = dir.path().join("cache.json");
    let first = scan::scan(&projects, &cache).expect("scan");
    assert_eq!(first["s1"].reads, 1);
    writeln!(open, "{}", &half[20..]).expect("write");
    write_lines(&file, &records[5..], true);
    let incremental = scan::scan(&projects, &cache).expect("scan");
    let full = scan::scan(&projects, &dir.path().join("fresh-cache.json")).expect("scan");
    assert_eq!(incremental["s1"], full["s1"]);
    assert_eq!(incremental["s1"].searches, 2, "the completed line was read once");
    // A file that became shorter is read again from the start.
    write_lines(&file, &records[..1], false);
    assert_eq!(scan::scan(&projects, &cache).expect("scan")["s1"].turns, 1);
}

fn session(cwd: &str, zcie: bool, tokens_per_turn: u64, turns: u64) -> Session {
    let mut s = Session { cwd: cwd.into(), zcie, turns, input: tokens_per_turn * turns, first: "2026-10-18T10:00:00Z".into(), last: "2026-10-18T11:00:00Z".into(), ..Session::default() };
    s.days.insert("2026-10-18".into(), scan::Day { tokens: s.input, turns });
    s
}

fn root(cwd: &str) -> Root {
    Root { path: cwd.trim_end_matches("/sub").to_string(), id: None }
}

#[test]
fn ts_22_03_the_report_compares_the_groups_of_each_project() {
    let mut sessions = vec![
        session("/work/alpha", true, 100, 2),
        session("/work/alpha/sub", true, 200, 2),
        session("/work/alpha", true, 300, 2),
        session("/work/alpha", false, 400, 1),
        session("/work/alpha", false, 500, 1),
        session("/work/alpha", false, 600, 1),
        session("/work/beta", true, 100, 1),
        session("/work/beta", false, 900, 1),
        session("/x/repo", false, 10, 1),
        session("/y/repo", false, 10, 1),
        session("/private/tmp/bench", true, 1, 1),
    ];
    let mut automated = session("/work/beta", false, 5, 1);
    automated.automated = true;
    let mut old = session("/work/beta", false, 5, 1);
    old.last = "2026-01-01T00:00:00Z".into();
    sessions.extend([automated, old, Session { cwd: "/work/beta".into(), ..session("/work/beta", false, 0, 0) }]);
    let stats = vec![
        Stat { ts: "2026-10-19T08:00:00Z".into(), kind: "read_guard".into(), cwd: "/work/alpha/sub".into(), lines: 900, read: 250, ..Stat::default() },
        Stat { ts: "2026-10-19T08:00:00Z".into(), kind: "gate".into(), cwd: "/work/alpha".into(), result: "block".into(), ..Stat::default() },
    ];
    let report = report::build(sessions, stats, "2026-10-01", &mut root);
    let alpha = report.projects.iter().find(|p| p.name == "alpha").expect("alpha");
    assert_eq!((alpha.zcie.sessions, alpha.plain.sessions), (3, 3));
    assert_eq!((alpha.zcie.tokens_per_turn, alpha.plain.tokens_per_turn), (Some(200.0), Some(500.0)));
    assert_eq!(alpha.saving.percent, Some(60.0));
    assert_eq!(alpha.saving.tokens, Some(1800), "6 turns × (500 − 200)");
    assert_eq!((alpha.guard.events, alpha.guard.file_lines, alpha.gate.block), (1, 900, 1));
    let beta = report.projects.iter().find(|p| p.name == "beta").expect("beta");
    assert_eq!(beta.plain.sessions, 1, "automated, old and empty sessions are not counted");
    assert_eq!(beta.saving.tokens, Some(800), "one session of each is enough: 1 turn × (900 − 100)");
    assert_eq!(beta.saving.note, "observed: 1 session with ZCIE against 1 session with no ZCIE");
    let names: Vec<&str> = report.projects.iter().map(|p| p.name.as_str()).collect();
    assert!(names.contains(&"x/repo") && names.contains(&"y/repo"), "{names:?}");
    assert!(!names.contains(&"bench"), "temporary folders are not projects");
    assert_eq!(report.saved_tokens, 2600);
    assert_eq!(report::median(vec![3.0, 1.0, 2.0, 10.0]), Some(2.5));
}

#[test]
fn ts_22_07_with_no_disk_the_roots_come_from_the_records() {
    let dir = tempfile::tempdir().expect("tmp");
    let path = dir.path().join("roots.json");
    std::fs::write(&path, r#"{"/work/old":{"path":"/work","id":null}}"#).expect("write");
    let session = Stat { kind: "session".into(), cwd: "/work/alpha/crate".into(), root: "/work/alpha".into(), project_id: "ws_x".into(), ..Stat::default() };
    let mut roots = super::load_roots(&path, &[session]);
    assert_eq!(super::resolve_root(&mut roots, "/work/alpha/crate", false), Root { path: "/work/alpha".into(), id: Some("ws_x".into()) });
    assert_eq!(super::resolve_root(&mut roots, "/work/old", false).path, "/work");
    let worktree = super::resolve_root(&mut roots, "/work/beta/.claude/worktrees/fix-1", false);
    assert_eq!(worktree.path, "/work/beta", "a linked worktree goes to its repository");
    assert!(!roots.contains_key("/work/beta/.claude/worktrees/fix-1"), "a guess is not recorded");
}

#[test]
fn ts_22_08_frequent_scans_do_not_rewrite_the_cache() {
    let dir = tempfile::tempdir().expect("tmp");
    let projects = dir.path().join("projects");
    let file = projects.join("-work-alpha/s1.jsonl");
    let records = main_records();
    write_lines(&file, &records[..4], false);
    let cache = dir.path().join("cache.json");
    scan::scan(&projects, &cache).expect("scan");
    let saved = std::fs::read(&cache).expect("the first scan writes the cache");
    write_lines(&file, &records[4..], true);
    let next = scan::scan(&projects, &cache).expect("scan");
    assert_eq!(next["s1"].turns, 2, "the new lines are counted");
    assert_eq!(std::fs::read(&cache).expect("cache"), saved, "no write within the save interval");
    assert_eq!(scan::scan(&projects, &cache).expect("scan")["s1"], next["s1"], "the same result from the old offsets");
}

#[test]
fn ts_22_09_a_project_with_no_baseline_gets_a_rough_saving() {
    let mut sessions = vec![session("/work/gamma", true, 100, 2)];
    sessions.extend((1..=3).map(|i| session(&format!("/work/p{i}"), false, 100 * i, 1)));
    let report = report::build(sessions.clone(), Vec::new(), "2026-10-01", &mut root);
    let gamma = report.projects.iter().find(|p| p.name == "gamma").expect("gamma");
    assert_eq!(gamma.saving.basis, "all projects");
    assert_eq!(gamma.saving.percent, Some(50.0), "median of 100, 200, 300 is 200");
    assert_eq!(gamma.saving.tokens, Some(200), "2 turns × (200 − 100)");
    assert_eq!(gamma.saving.note, "rough: 1 session with ZCIE against 3 sessions with no ZCIE in all projects");
    assert_eq!((report.saved_tokens, report.rough_saved_tokens), (0, 200));
    // With no session with no ZCIE in any project there is no value.
    let alone = report::build(sessions[..1].to_vec(), Vec::new(), "2026-10-01", &mut root);
    assert_eq!(alone.projects[0].saving.percent, None);
    assert_eq!(alone.projects[0].saving.note, "no sessions with no ZCIE in any project");
}

#[test]
fn ts_22_10_a_session_that_zcie_joined_later_is_split() {
    let dir = tempfile::tempdir().expect("tmp");
    let projects = dir.path().join("projects");
    let (t1, t2) = ("2026-10-18T10:00:00Z", "2026-10-18T12:00:00Z");
    let capsule = json!({"type": "attachment", "timestamp": t2, "attachment": {"command": "/u/.zcie/hooks/inject-context.sh", "content": "ZCIE context (x)"}});
    let records = vec![user("first task", t1), assistant("m1", json!({"type": "text"}), t1), user("second task", t2), capsule, assistant("m2", json!({"type": "text"}), t2)];
    write_lines(&projects.join("-work-alpha/s1.jsonl"), &records, false);
    write_lines(&projects.join("-work-alpha/s1/subagents/early.jsonl"), &[assistant("m8", json!({"type": "text"}), "2026-10-18T10:30:00Z")], false);
    write_lines(&projects.join("-work-alpha/s1/subagents/late.jsonl"), &[assistant("m9", json!({"type": "text"}), "2026-10-18T12:30:00Z")], false);
    let sessions = scan::scan(&projects, &dir.path().join("cache.json")).expect("scan");
    let s1 = &sessions["s1"];
    let before = s1.before.as_deref().expect("the part before ZCIE");
    assert_eq!((before.turns, before.tokens()), (1, 2230), "turn 1, m1 and the early subagent");
    assert_eq!((s1.turns, s1.tokens(), s1.zcie_since.as_str()), (1, 2230, t2), "turn 2 (its capsule came after it), m2 and the late subagent");
    let report = report::build(sessions.into_values().collect(), Vec::new(), "2026-10-01", &mut root);
    let alpha = &report.projects[0];
    assert_eq!((alpha.zcie.sessions, alpha.plain.sessions), (1, 1));
    assert_eq!(alpha.saving.percent, Some(0.0), "the same tokens per turn in both parts");
}

#[test]
fn ts_24_01_an_outlier_session_is_listed_but_left_out_of_the_saving() {
    let mut sessions = vec![session("/work/alpha", true, 100, 2), session("/work/alpha", true, 5000, 3)];
    sessions.extend([200, 300, 400].map(|t| session("/work/alpha", false, t, 1)));
    for (i, s) in sessions.iter_mut().enumerate() {
        s.id = format!("session-{i}");
    }
    sessions[1].subagents = 6;
    sessions[1].subagent_cost = 14_000;
    let mut automated = session("/work/alpha", false, 50, 1);
    automated.automated = true;
    sessions.push(automated);
    let report = report::build(sessions, Vec::new(), "2026-10-01", &mut root);
    let alpha = &report.projects[0];
    let big = alpha.sessions.iter().find(|r| r.turns == 3).expect("the large session");
    assert!(big.outlier, "5000 per turn ≥ 3 × the median 300");
    assert_eq!((big.subagents, (big.subagent_share * 100.0).round()), (6, 93.0));
    assert_eq!(alpha.saving.excluded, 1);
    assert_eq!(alpha.saving.percent, Some((1.0 - 100.0 / 300.0) * 100.0), "100 against the median 300, the outlier left out");
    assert_eq!(alpha.saving.confidence, "low", "one session with ZCIE");
    assert_eq!(alpha.sessions.len(), 5);
    assert_eq!((report.automated.sessions, report.automated.cost), (1, 50));
    assert_eq!(super::honest::confidence(4, 12, false), "medium");
    assert_eq!(super::honest::confidence(12, 12, false), "high");
}
