//! TS-06 — Code intelligence (`docs/13-test-plan.md` section 5).
#![allow(clippy::unwrap_used, clippy::expect_used)]

#[path = "t2_common.rs"]
mod common;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use common::{T, edges_from, file_record, symbols};
use zcie_store::Batch;
use zcie_tests::{TestEngine, commit_all, fixture_repo};
use zcie_types::api::RegisterRepositoryResponse;
use zcie_types::{EdgeStatus, EvidenceKind, ParserQuality, Relation, SymbolKind};

fn indexed_fixture(engine: &TestEngine) -> (std::path::PathBuf, RegisterRepositoryResponse) {
    let repo = fixture_repo(engine.dir.path(), true);
    let registered = engine.register(&repo);
    engine.wait_idle();
    (repo, registered)
}

fn wait_until(mut check: impl FnMut() -> bool) {
    let started = Instant::now();
    while !check() {
        assert!(started.elapsed() < Duration::from_secs(20), "condition not reached");
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn discovery_excludes_env_and_marks_binary_files() {
    let engine = TestEngine::start();
    let (_, registered) = indexed_fixture(&engine);
    let wt = &registered.worktree_id;
    assert!(file_record(&engine, wt, ".env").is_none());
    let icon = file_record(&engine, wt, "assets/icon.bin").expect("icon indexed");
    assert!(icon.is_binary);
    assert!(file_record(&engine, wt, "docs/notes.md").is_some());
}

#[test]
fn rust_method_is_extracted_with_span() {
    let engine = TestEngine::start();
    indexed_fixture(&engine);
    let found = symbols(&engine, "attach_to_bone");
    let symbol = found.iter().find(|s| s.language == "rust").expect("rust symbol");
    assert_eq!(symbol.qualified_name, "WeaponSocket::attach_to_bone");
    assert_eq!(symbol.kind, SymbolKind::Method);
    assert_eq!(symbol.span, [8, 10]);
    assert_eq!(symbol.path, "src/animation/weapon_socket.rs");
}

#[test]
fn typescript_method_is_extracted() {
    let engine = TestEngine::start();
    indexed_fixture(&engine);
    let found = symbols(&engine, "attachToBone");
    assert!(found.iter().any(|s| s.qualified_name == "WeaponSocket.attachToBone" && s.kind == SymbolKind::Method));
}

#[test]
fn python_function_and_class_are_extracted() {
    let engine = TestEngine::start();
    indexed_fixture(&engine);
    assert!(symbols(&engine, "build_report").iter().any(|s| s.kind == SymbolKind::Function));
    assert!(symbols(&engine, "Report").iter().any(|s| s.kind == SymbolKind::Class && s.language == "python"));
}

#[test]
fn file_with_syntax_error_is_marked_partial() {
    let engine = TestEngine::start();
    indexed_fixture(&engine);
    let found = symbols(&engine, "complete_function");
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].parser_quality, ParserQuality::PartialSyntaxError);
}

#[test]
fn unchanged_file_keeps_symbol_ids_and_skips_parse() {
    let engine = TestEngine::start();
    let (repo, registered) = indexed_fixture(&engine);
    let before_ids: Vec<_> = symbols(&engine, "attach_to_bone").into_iter().map(|s| s.symbol_id).collect();
    let before = file_record(&engine, &registered.worktree_id, "src/animation/weapon_socket.rs").unwrap();
    engine.capture("session.start", "s1", None, "");
    let path = repo.join("src/animation/weapon_socket.rs");
    std::fs::write(&path, std::fs::read(&path).unwrap()).unwrap();
    std::thread::sleep(Duration::from_millis(300));
    engine.wait_idle();
    let after = file_record(&engine, &registered.worktree_id, "src/animation/weapon_socket.rs").unwrap();
    let after_ids: Vec<_> = symbols(&engine, "attach_to_bone").into_iter().map(|s| s.symbol_id).collect();
    assert_eq!(before_ids, after_ids);
    assert_eq!(before.indexed_seq, after.indexed_seq);
}

#[test]
fn deleted_function_loses_symbol_and_edges() {
    let engine = TestEngine::start();
    let (repo, registered) = indexed_fixture(&engine);
    let old = symbols(&engine, "attach_to_bone").into_iter().find(|s| s.language == "rust").unwrap();
    std::fs::write(repo.join("src/animation/weapon_socket.rs"), "pub struct WeaponSocket {\n    pub bone: String,\n}\n").unwrap();
    wait_until(|| symbols(&engine, "attach_to_bone").iter().all(|s| s.language != "rust"));
    engine.wait_idle();
    let old_ref = format!("sym:{}", old.symbol_id);
    let incoming = zcie_graph::edges_to(&engine.engine().store, T, &old_ref, None).unwrap();
    assert!(incoming.is_empty());
    let file = file_record(&engine, &registered.worktree_id, "src/animation/weapon_socket.rs").unwrap();
    let defines = edges_from(&engine, &format!("file:{}", file.file_id), Relation::DefinesSymbol);
    assert!(defines.iter().all(|e| e.to != old_ref));
    assert!(!defines.is_empty());
}

fn task_ref(engine: &TestEngine, external: &str) -> String {
    let ws = common::workspace_id(engine);
    let task = zcie_memory::find_by_external_ref(&engine.engine().store, &Batch::new(), T, &ws, external).unwrap().expect("task");
    format!("task:{}", task.task_id)
}

fn commit_and_capture(engine: &TestEngine, repo: &std::path::Path, message: &str) {
    std::fs::write(repo.join("web/bone.ts"), format!("export interface Bone {{\n  name: string; // {message}\n}}\n")).unwrap();
    let sha = commit_all(repo, message);
    engine.capture("git.commit", "git", None, &format!("{{\"sha\":\"{sha}\"}}"));
    engine.wait_idle();
}

#[test]
fn commit_trailer_makes_verified_implementation_link() {
    let engine = TestEngine::start();
    let (repo, _) = indexed_fixture(&engine);
    engine.capture("user.message", "s1", Some("1"), "Fix #89 bone naming");
    engine.wait_idle();
    commit_and_capture(&engine, &repo, "Fix bone naming\n\nZCIE-Task: #89");
    let edges = edges_from(&engine, &task_ref(&engine, "#89"), Relation::Implements);
    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].status, EdgeStatus::Verified);
    assert_eq!(edges[0].evidence.kind, EvidenceKind::CommitTrailer);
}

/// Regression: with two repositories in one workspace, a `git.commit` event without a
/// repository ID resolves its worktree by the commit SHA (found in a Claude Code session test).
#[test]
fn commit_event_finds_its_repository_when_workspace_has_two() {
    let engine = TestEngine::start();
    let (repo, _) = indexed_fixture(&engine);
    let other_parent = engine.dir.path().join("other");
    std::fs::create_dir_all(&other_parent).unwrap();
    engine.register(&fixture_repo(&other_parent, true));
    engine.wait_idle();
    engine.capture("user.message", "s1", Some("1"), "Fix #93 bone naming");
    engine.wait_idle();
    commit_and_capture(&engine, &repo, "Fix bone naming\n\nZCIE-Task: #93");
    wait_until(|| !edges_from(&engine, &task_ref(&engine, "#93"), Relation::Implements).is_empty());
    let edges = edges_from(&engine, &task_ref(&engine, "#93"), Relation::Implements);
    assert_eq!(edges[0].evidence.kind, EvidenceKind::CommitTrailer);
}

#[test]
fn commit_without_trailer_makes_no_implementation_link() {
    let engine = TestEngine::start();
    let (repo, _) = indexed_fixture(&engine);
    engine.capture("user.message", "s1", Some("1"), "Fix #91 bone naming");
    engine.wait_idle();
    std::thread::sleep(Duration::from_secs(1));
    commit_and_capture(&engine, &repo, "Fix bone naming");
    let edges = edges_from(&engine, &task_ref(&engine, "#91"), Relation::Implements);
    assert!(edges.iter().all(|e| e.status != EdgeStatus::Verified), "{edges:?}");
}

/// Measures the watcher output: each `FileChange` becomes exactly one file job.
#[test]
fn rapid_writes_to_one_file_give_one_change() {
    let dir = tempfile::tempdir().unwrap();
    let changes = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&changes);
    let exclude = zcie_security::ExcludeSet::with_defaults(&[]).unwrap();
    let handler = Box::new(move |_change: zcie_fs::FileChange| {
        counter.fetch_add(1, Ordering::SeqCst);
    });
    let root = dir.path().canonicalize().unwrap();
    let _watcher = zcie_fs::FileWatcher::start(root.clone(), Duration::from_millis(200), exclude, handler).unwrap();
    std::thread::sleep(Duration::from_millis(300));
    for i in 0..20 {
        std::fs::write(root.join("a.rs"), format!("fn f{i}() {{}}\n")).unwrap();
        std::thread::sleep(Duration::from_millis(5));
    }
    std::thread::sleep(Duration::from_millis(1200));
    assert_eq!(changes.load(Ordering::SeqCst), 1);
}

#[test]
fn folder_without_git_is_indexed_with_git_unavailable() {
    let engine = TestEngine::start();
    let repo = fixture_repo(engine.dir.path(), false);
    let registered = engine.register(&repo);
    engine.wait_idle();
    assert!(!registered.git_available);
    assert!(symbols(&engine, "build_report").iter().any(|s| s.kind == SymbolKind::Function));
    assert!(file_record(&engine, &registered.worktree_id, ".env").is_none());
}
