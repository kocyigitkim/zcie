//! TS-14 — Projects and central memory (ADR-009, `docs/13-test-plan.md` section 5).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde_json::{Value, json};
use zcie_api::multi::names_project;
use zcie_security::AuthContext;
use zcie_tests::engine::capture_request;
use zcie_tests::process::binary;
use zcie_tests::{DaemonProcess, TestEngine, admin, free_port, git, mcp_session_in, workspace_auth, workspace_root};
use zcie_types::api::{EnsureProjectRequest, ProjectInfo};
use zcie_types::{QueryRequest, QueryResponse, QueryScope};

const LEDGER_DECISION: &str = "Remember this decision: we use Postgres 16 for the ledger database.";
const ATLAS_DECISION: &str = "Remember this decision: we use Redis 7 for the atlas cache database.";

fn ensure(engine: &TestEngine, name: &str, id: Option<&str>) -> ProjectInfo {
    let request = EnsureProjectRequest { project_id: id.map(str::to_string), name: Some(name.into()), root: None };
    engine.engine().ensure_project(&admin(), request).expect("ensure project")
}

fn remember(engine: &TestEngine, project: &str, text: &str) {
    engine.engine().capture_event(&admin(), capture_request_in(project, text)).expect("capture");
}

fn ask(engine: &TestEngine, auth: &AuthContext, project: &str, question: &str, scope: QueryScope) -> QueryResponse {
    let request = QueryRequest {
        workspace: project.into(),
        question: question.into(),
        scope,
        cross_project: true,
        token_budget: Some(1000),
        ..QueryRequest::default()
    };
    engine.engine().query(auth, request).expect("query")
}

fn texts(response: &QueryResponse) -> String {
    response.results.iter().map(|r| format!("{} {}", r.title, r.snippet.clone().unwrap_or_default())).collect::<Vec<_>>().join("\n")
}

fn labels(response: &QueryResponse) -> Vec<String> {
    response.results.iter().filter_map(|r| r.project.clone()).collect()
}

fn two_projects() -> TestEngine {
    let engine = TestEngine::start();
    ensure(&engine, "ledger", None);
    ensure(&engine, "atlas", None);
    remember(&engine, "ledger", LEDGER_DECISION);
    remember(&engine, "atlas", ATLAS_DECISION);
    engine.wait_idle();
    engine
}

#[test]
fn ts_14_01_ensure_project_makes_one_id_and_is_idempotent() {
    let engine = TestEngine::start();
    let first = ensure(&engine, "ledger", None);
    assert!(first.project_id.starts_with("proj_"), "{}", first.project_id);
    let again = ensure(&engine, "another-name", Some(&first.project_id));
    assert_eq!(again.project_id, first.project_id);
    assert_eq!(again.name, "ledger", "an existing project keeps its name");
    let known = "proj_01M52Y8GKM6NJ53WCSMFY4W55A";
    assert_eq!(ensure(&engine, "cloned", Some(known)).project_id, known, "an ID from a cloned folder is kept");
    let bad = EnsureProjectRequest { project_id: Some("not-an-id".into()), name: None, root: None };
    assert!(engine.engine().ensure_project(&admin(), bad).is_err());
}

#[test]
fn ts_14_02_project_query_reads_only_the_current_project() {
    let engine = two_projects();
    let response = ask(&engine, &admin(), "ledger", "which database do we use", QueryScope::Project);
    let text = texts(&response);
    assert!(text.contains("Postgres"), "{text}");
    assert!(!text.contains("Redis"), "{text}");
}

#[test]
fn ts_14_03_global_query_reads_all_projects_and_labels_results() {
    let engine = two_projects();
    let response = ask(&engine, &admin(), "ledger", "which database do we use", QueryScope::Global);
    let found = labels(&response);
    assert!(found.contains(&"ledger".to_string()) && found.contains(&"atlas".to_string()), "{found:?}");
    assert!(response.cursor.is_none());
}

#[test]
fn ts_14_04_question_that_names_a_project_reads_that_project_first() {
    let engine = two_projects();
    let response = ask(&engine, &admin(), "ledger", "which database does atlas use", QueryScope::Project);
    assert_eq!(response.results.first().and_then(|r| r.project.as_deref()), Some("atlas"));
    assert!(texts(&response).contains("Redis"));
    assert!(response.note.as_deref().unwrap_or_default().starts_with("2 projects"));
}

#[test]
fn ts_14_05_global_query_keeps_the_key_scope() {
    let engine = two_projects();
    let auth = workspace_auth("user-ledger", &["ledger"]);
    let response = ask(&engine, &auth, "ledger", "which database do we use", QueryScope::Global);
    assert!(!texts(&response).contains("Redis"));
    assert_eq!(engine.engine().list_projects(&auth).unwrap().projects.len(), 1);
}

#[test]
fn ts_14_06_rename_keeps_the_id_and_the_memory() {
    let engine = two_projects();
    let id = engine.engine().project_info("ledger").unwrap().project_id;
    let renamed = engine.engine().rename_project(&admin(), "ledger", "ledger-v2").unwrap();
    assert_eq!(renamed.project_id, id);
    assert!(engine.engine().project_info("ledger").is_err());
    assert!(engine.engine().rename_project(&admin(), "ledger-v2", "atlas").is_err(), "names stay unique");
    let response = ask(&engine, &admin(), &id, "which database do we use", QueryScope::Project);
    assert!(texts(&response).contains("Postgres"));
}

#[test]
fn ts_14_07_delete_needs_admin_and_forgets_the_memory() {
    let engine = two_projects();
    let user = workspace_auth("user", &["*"]);
    assert!(engine.engine().delete_project(&user, "atlas").is_err());
    engine.engine().delete_project(&admin(), "atlas").unwrap();
    engine.wait_idle();
    let names: Vec<String> = engine.engine().list_projects(&admin()).unwrap().projects.into_iter().map(|p| p.name).collect();
    assert_eq!(names, ["ledger"]);
    let response = ask(&engine, &admin(), "ledger", "which database do we use", QueryScope::Global);
    assert!(!texts(&response).contains("Redis"), "{}", texts(&response));
}

#[test]
fn ts_14_08_names_project_matches_whole_words_only() {
    assert!(names_project("Which database does Atlas use?", "atlas"));
    assert!(names_project("atlas'nin veritabanı ne?", "atlas"));
    assert!(names_project("look in tantivy-mono", "tantivy-mono"));
    assert!(!names_project("the atlases are big", "atlas"));
    assert!(!names_project("go to ab now", "ab"), "names shorter than 3 characters do not match");
}

fn started() -> (tempfile::TempDir, DaemonProcess) {
    let dir = tempfile::tempdir().unwrap();
    let daemon = DaemonProcess::spawn(&dir.path().join("home"), free_port());
    (dir, daemon)
}

fn git_folder(parent: &Path, name: &str) -> PathBuf {
    let root = parent.join(name);
    std::fs::create_dir_all(root.join("src")).unwrap();
    git(&root, &["init", "-q", "-b", "main"]);
    root.canonicalize().unwrap()
}

fn run_session_start(daemon: &DaemonProcess, cwd: &Path, home: &Path) {
    let script = workspace_root().join("adapters/claude-code/hooks/session-start.sh");
    let mut child = Command::new("sh")
        .arg(script)
        .env("ZCIE_URL", daemon.url())
        .env("ZCIE_API_KEY", daemon.admin_key())
        .env("HOME", home)
        .env_remove("ZCIE_WORKSPACE")
        .env("TMPDIR", std::env::temp_dir())
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let input = json!({"session_id": "s-auto", "cwd": cwd, "hook_event_name": "SessionStart", "source": "startup"});
    child.stdin.take().unwrap().write_all(input.to_string().as_bytes()).unwrap();
    assert!(child.wait().unwrap().success());
}

fn id_file(root: &Path) -> Option<String> {
    std::fs::read_to_string(root.join(".claude/zcie_id")).ok().map(|t| t.trim().to_string())
}

#[test]
fn ts_14_09_session_start_hook_makes_the_id_file_once() {
    let (dir, daemon) = started();
    let root = git_folder(dir.path(), "billing");
    run_session_start(&daemon, &root.join("src"), dir.path());
    let id = id_file(&root).expect("the hook writes .claude/zcie_id in the Git root");
    assert!(zcie_types::ids::is_project_id(&id));
    run_session_start(&daemon, &root.join("src"), dir.path());
    assert_eq!(id_file(&root).as_deref(), Some(id.as_str()), "a second session keeps the ID");
    assert!(id_file(&root.join("src")).is_none());
    let info = daemon.client(&id).project(&id).unwrap();
    assert_eq!((info.name.as_str(), info.roots), ("billing", vec![root.display().to_string()]));
    let home = dir.path().join("home-folder");
    std::fs::create_dir_all(&home).unwrap();
    run_session_start(&daemon, &home, &home);
    assert!(id_file(&home).is_none(), "the home folder is never a project");
}

fn zcie(daemon: &DaemonProcess, cwd: &Path, args: &[&str]) -> String {
    let output = Command::new(binary("zcie"))
        .current_dir(cwd)
        .env_remove("ZCIE_WORKSPACE")
        .args(["--url", &daemon.url(), "--key", &daemon.admin_key(), "--home"])
        .arg(&daemon.home)
        .args(args)
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    String::from_utf8_lossy(&output.stdout).to_string()
}

#[test]
fn ts_14_10_cli_init_finds_the_project_from_a_subfolder_and_migrate_restores_the_id() {
    let (dir, daemon) = started();
    let root = git_folder(dir.path(), "atlas");
    zcie(&daemon, &root.join("src"), &["project", "init", "--name", "atlas-app"]);
    let id = id_file(&root).expect("init writes the ID file in the Git root");
    let shown: Value = serde_json::from_str(&zcie(&daemon, &root.join("src"), &["project", "show"])).unwrap();
    assert_eq!(shown["project_id"].as_str(), Some(id.as_str()));
    assert_eq!(shown["name"].as_str(), Some("atlas-app"));
    std::fs::remove_file(root.join(".claude/zcie_id")).unwrap();
    let report = zcie(&daemon, dir.path(), &["project", "migrate"]);
    assert!(report.contains("wrote"), "{report}");
    assert_eq!(id_file(&root).as_deref(), Some(id.as_str()));
}

#[test]
fn ts_14_11_mcp_finds_the_project_and_reads_global_scope() {
    let (dir, daemon) = started();
    let ledger = git_folder(dir.path(), "ledger");
    let atlas = git_folder(dir.path(), "atlas");
    zcie(&daemon, &ledger, &["project", "init"]);
    zcie(&daemon, &atlas, &["project", "init"]);
    for (project, text) in [("ledger", LEDGER_DECISION), ("atlas", ATLAS_DECISION)] {
        daemon.client(project).capture(capture_request_in(project, text)).unwrap();
    }
    daemon.wait_idle();
    let call = |id: u32, arguments: Value| json!({"jsonrpc":"2.0","id":id,"method":"tools/call","params":{"name":"zcie_query","arguments":arguments}});
    let calls = [
        call(1, json!({"question": "which database do we use"})),
        call(2, json!({"question": "which database do we use", "scope": "global"})),
    ];
    let responses = mcp_session_in(&daemon, &ledger.join("src"), &calls);
    let text = |i: usize| responses[i]["result"]["content"][0]["text"].as_str().unwrap_or_default().to_string();
    assert!(text(1).contains("Postgres") && !text(1).contains("Redis"), "{}", text(1));
    assert!(text(2).contains("Postgres") && text(2).contains("Redis"), "{}", text(2));
}

fn capture_request_in(project: &str, text: &str) -> zcie_types::api::CaptureRequest {
    let mut request = capture_request("user.message", &format!("s-{project}"), Some("1"), text);
    request.workspace = project.into();
    request
}
