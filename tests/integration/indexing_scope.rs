//! TS-19 — Only added folders are indexed (ADR-015). A folder that a conversation import or a
//! Claude Code session registers maps the memory to its project; it is not scanned or watched.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;
use std::time::{Duration, Instant};

use zcie_store::{Cf, layout};
use zcie_tests::{TestEngine, admin};
use zcie_types::api::{EnsureProjectRequest, ProjectStatus};
use zcie_types::{JobKind, JobPriority, JobRecord, Scope};

fn folder(root: &Path) {
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join(".gitignore"), "build/\n").unwrap();
    std::fs::write(root.join("src/fee.rs"), "pub fn apply_late_fee(cents: i64) -> i64 {\n    cents + 250\n}\n").unwrap();
}

fn ensure(engine: &TestEngine, root: &Path, index: bool) -> String {
    let request = EnsureProjectRequest { project_id: None, name: Some("billing".into()), root: Some(root.display().to_string()), index };
    engine.engine().ensure_project(&admin(), request).unwrap().project_id
}

fn status(engine: &TestEngine, id: &str) -> ProjectStatus {
    engine.wait_idle();
    engine.engine().project_status(&admin(), id).unwrap()
}

#[test]
fn ts_19_01_memory_only_folder_is_not_indexed_until_added() {
    let engine = TestEngine::start();
    let root = engine.dir.path().join("billing");
    folder(&root);
    let id = ensure(&engine, &root, false);
    let before = status(&engine, &id);
    assert_eq!((before.files, before.indexed, before.ready), (0, false, true), "a hook or an import registers the folder for the memory only");
    assert_eq!(ensure(&engine, &root, true), id, "the same folder keeps its project");
    let added = status(&engine, &id);
    assert!(added.indexed && added.files >= 2 && added.symbols >= 1, "{added:?}");
    ensure(&engine, &root, false);
    assert!(status(&engine, &id).indexed, "a later registration for the memory does not turn the index off");
}

#[test]
fn ts_19_02_watcher_skips_gitignored_build_outputs() {
    let engine = TestEngine::start();
    let root = engine.dir.path().join("billing");
    folder(&root);
    let id = ensure(&engine, &root, true);
    let files = status(&engine, &id).files;
    std::fs::create_dir_all(root.join("build")).unwrap();
    std::fs::write(root.join("build/out.rs"), "pub fn generated() {}\n").unwrap();
    std::fs::write(root.join("src/tax.rs"), "pub fn apply_tax(cents: i64) -> i64 {\n    cents\n}\n").unwrap();
    let started = Instant::now();
    while status(&engine, &id).files < files + 1 && started.elapsed() < Duration::from_secs(10) {
        std::thread::sleep(Duration::from_millis(200));
    }
    std::thread::sleep(Duration::from_secs(1));
    assert_eq!(status(&engine, &id).files, files + 1, "src/tax.rs is indexed; build/out.rs is ignored");
}

#[test]
fn ts_19_03_startup_drops_file_jobs_of_folders_that_are_not_indexed() {
    let mut engine = TestEngine::start();
    let root = engine.dir.path().join("notes");
    folder(&root);
    let id = ensure(&engine, &root, false);
    let worktree = zcie_memory::worktrees_of_workspace(&engine.engine().store, "local", &id).unwrap().remove(0);
    let scope = Scope { worktree_id: Some(worktree.worktree_id.clone()), ..Scope::workspace("user-001", &id) };
    let job = JobRecord::new(JobKind::IndexFile, scope, JobPriority::Background, 1, zcie_types::job::file_target(&worktree.worktree_id, "src/fee.rs"));
    let key = layout::pending_job("local", job.source_seq, job.job_id.as_str());
    engine.engine().store.put_sync(Cf::Jobs, &key, &job).unwrap();
    engine.restart();
    assert!(!engine.engine().store.exists(Cf::Jobs, &key).unwrap(), "the old job of a memory-only folder is removed");
    assert_eq!(status(&engine, &id).files, 0);
}

#[test]
fn ts_19_04_overview_question_gets_the_project_card() {
    let engine = TestEngine::start();
    let root = engine.dir.path().join("platform");
    folder(&root);
    std::fs::write(root.join("README.md"), "# Platform\nRuns the billing service and the report service with Docker.\n").unwrap();
    std::fs::write(root.join("compose.yml"), "services: {}\n").unwrap();
    let inner = root.join("billing-app");
    folder(&inner);
    let id = ensure(&engine, &root, true);
    ensure_named(&engine, &inner, "billing-app");
    engine.wait_idle();
    let request = zcie_types::QueryRequest { workspace: id, question: "Bu proje hakkında bana bilgi ver".into(), token_budget: Some(1200), ..zcie_types::QueryRequest::default() };
    let response = engine.engine().query(&admin(), request).unwrap();
    let answer = response.answer.clone().unwrap_or_default();
    assert!(answer.contains("Top level:") && answer.contains("compose.yml") && answer.contains("src/"), "{answer}");
    assert!(answer.contains("billing-app/ = project billing-app"), "the inner project is named: {answer}");
    let readme = response.results.iter().find(|r| r.kind == "file").and_then(|r| r.snippet.clone()).unwrap_or_default();
    assert!(readme.contains("Runs the billing service"), "the README start is in the card: {readme}");
}

fn ensure_named(engine: &TestEngine, root: &Path, name: &str) -> String {
    let request = EnsureProjectRequest { project_id: None, name: Some(name.into()), root: Some(root.display().to_string()), index: false };
    engine.engine().ensure_project(&admin(), request).unwrap().project_id
}

#[test]
fn ts_19_05_assistant_answers_are_stored_and_found() {
    let engine = TestEngine::start_with(|config| config.capture.assistant_messages = true);
    engine.capture("user.message", "s-ans", Some("1"), "Raporlama servisi neden yavaş?");
    engine.capture("assistant.message", "s-ans", Some("1"), "Raporlama servisi yavaş çünkü rapor sorgusu indekssiz invoice_lines tablosunu tarıyor; invoice_lines(customer_id) indeksi eklenmeli.");
    engine.wait_idle();
    let response = engine.query("raporlama servisi neden yavaştı", 1200);
    let found = response.results.iter().any(|r| r.title.contains("assistant.message") || r.snippet.as_deref().unwrap_or_default().contains("invoice_lines"));
    assert!(found, "{:?}", response.results.iter().map(|r| &r.title).collect::<Vec<_>>());
}

fn fee_record(engine: &TestEngine, id: &str) -> zcie_types::FileRecord {
    let worktree = zcie_memory::worktrees_of_workspace(&engine.engine().store, "local", id).unwrap().remove(0);
    let key = layout::path("local", &worktree.worktree_id, "src/fee.rs");
    let file: zcie_store::values::FileIdValue = engine.engine().store.get(Cf::Lookup, &key).unwrap().unwrap();
    engine.engine().store.get(Cf::Entities, &layout::entity("local", "file", &file.file_id)).unwrap().unwrap()
}

#[test]
fn ts_19_06_blob_sweep_keeps_the_contents_of_indexed_files() {
    let engine = TestEngine::start();
    let root = engine.dir.path().join("billing");
    folder(&root);
    let id = ensure(&engine, &root, true);
    engine.wait_idle();
    let record = fee_record(&engine, &id);
    engine.engine().objects.sweep_orphans(&engine.engine().store, "local", Duration::ZERO).unwrap();
    assert!(engine.engine().objects.get_text(&record.content_hash).unwrap().contains("apply_late_fee"), "the sweep keeps the file content");
}

#[test]
fn ts_19_07_a_missing_file_content_comes_back_at_the_next_scan() {
    let engine = TestEngine::start();
    let root = engine.dir.path().join("billing");
    folder(&root);
    let id = ensure(&engine, &root, true);
    engine.wait_idle();
    let record = fee_record(&engine, &id);
    engine.engine().objects.delete(&record.content_hash).unwrap();
    assert!(!engine.engine().objects.exists(&record.content_hash));
    ensure(&engine, &root, true);
    engine.wait_idle();
    assert!(engine.engine().objects.exists(&record.content_hash), "the scan stores the content again");
}
