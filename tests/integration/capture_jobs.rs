//! TS-04 — Capture and jobs (`docs/13-test-plan.md` section 5).
#![allow(clippy::unwrap_used, clippy::expect_used)]

#[path = "t2_common.rs"]
mod common;

use std::sync::Arc;
use std::time::{Duration, Instant};

use common::{T, count, count_edges, count_entities};
use zcie_api::ApiError;
use zcie_store::{Cf, layout};
use zcie_tests::engine::capture_request;
use zcie_tests::{TestEngine, admin, fixture_repo};
use zcie_types::{JobKind, JobPriority, JobRecord, Scope};

#[test]
fn capture_user_message_is_durable_with_payload() {
    let engine = TestEngine::start();
    let receipt = engine.capture("user.message", "s1", Some("1"), "Fix the weapon socket");
    assert!(receipt.durable);
    assert!(!receipt.duplicate);
    let store = &engine.engine().store;
    let event = zcie_store::events::get_event_by_id(store, T, &receipt.event_id).unwrap().expect("event");
    let text = zcie_store::events::payload_text(&engine.engine().objects, &event).unwrap();
    assert_eq!(text.as_deref(), Some("Fix the weapon socket"));
}

#[test]
fn capture_with_same_idempotency_key_returns_same_event() {
    let engine = TestEngine::start();
    let request = capture_request("user.message", "s1", Some("1"), "hello world");
    let first = engine.engine().capture_event(&admin(), request.clone()).unwrap();
    let second = engine.engine().capture_event(&admin(), request).unwrap();
    assert_eq!(first.event_id, second.event_id);
    assert!(second.duplicate);
    assert_eq!(engine.engine().ingest.event_seq(), 1);
    assert_eq!(count(&engine.engine().store, Cf::Events, &layout::events_prefix(T)), 1);
}

#[test]
fn concurrent_captures_with_same_key_store_one_event() {
    let engine = TestEngine::start();
    let request = capture_request("user.message", "s1", Some("1"), "same message");
    let shared = Arc::clone(engine.engine());
    let handles: Vec<_> = (0..10)
        .map(|_| {
            let engine = Arc::clone(&shared);
            let request = request.clone();
            std::thread::spawn(move || engine.capture_event(&admin(), request).unwrap().event_id)
        })
        .collect();
    let ids: Vec<String> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    assert!(ids.iter().all(|id| id == &ids[0]));
    assert_eq!(count(&engine.engine().store, Cf::Events, &layout::events_prefix(T)), 1);
}

#[test]
fn capture_larger_than_limit_is_rejected() {
    let engine = TestEngine::start_with(|c| c.ingest.max_payload_bytes = 100);
    let request = capture_request("user.message", "s1", None, &"x".repeat(101));
    let error = engine.engine().capture_event(&admin(), request).unwrap_err();
    assert_eq!(error, ApiError::PayloadTooLarge);
}

#[test]
fn capture_redacts_aws_key_before_storage() {
    let engine = TestEngine::start();
    let receipt = engine.capture("user.message", "s1", None, "my key is AKIAABCDEFGHIJKLMNOP ok");
    let store = &engine.engine().store;
    let event = zcie_store::events::get_event_by_id(store, T, &receipt.event_id).unwrap().unwrap();
    let text = zcie_store::events::payload_text(&engine.engine().objects, &event).unwrap().unwrap();
    assert!(text.contains("[REDACTED:R-01]"), "{text}");
    assert!(!text.contains("AKIAABCDEFGHIJKLMNOP"));
    assert_eq!(event.redactions, 1);
}

#[test]
fn assistant_message_is_forbidden_when_policy_disables_it() {
    let engine = TestEngine::start();
    let request = capture_request("assistant.message", "s1", None, "I fixed it");
    let error = engine.engine().capture_event(&admin(), request).unwrap_err();
    assert!(matches!(error, ApiError::Forbidden(_)), "{error:?}");
    assert_eq!(engine.engine().ingest.event_seq(), 0);
}

#[test]
fn job_that_fails_three_times_goes_to_dead_letter() {
    let engine = TestEngine::start();
    let repo = fixture_repo(engine.dir.path(), true);
    let registered = engine.register(&repo);
    engine.wait_idle();
    let mut scope = Scope::workspace("user-001", &common::workspace_id(&engine));
    scope.repository_id = Some(registered.repository_id);
    scope.worktree_id = Some(registered.worktree_id);
    let sha = "0123456789abcdef0123456789abcdef01234567".to_string();
    let job = JobRecord::new(JobKind::IndexCommit, scope, JobPriority::Interactive, 0, sha);
    let job_id = engine.engine().ingest.enqueue_job(job).unwrap();
    let dead_key = layout::dead_job(T, job_id.as_str());
    let started = Instant::now();
    while !engine.engine().store.exists(Cf::Jobs, &dead_key).unwrap() {
        assert!(started.elapsed() < Duration::from_secs(20), "job did not reach dead letter");
        std::thread::sleep(Duration::from_millis(50));
    }
    let file = engine.home().join("instances/local/queue/dead-letter").join(format!("{job_id}.json"));
    assert!(file.exists());
    let dead: JobRecord = engine.engine().store.get(Cf::Jobs, &dead_key).unwrap().unwrap();
    assert_eq!(dead.attempts, 3);
}

#[test]
fn extract_memory_job_replay_makes_no_duplicates() {
    let engine = TestEngine::start();
    let receipt = engine.capture("user.message", "s1", Some("1"), "Fix #89 attach_to_bone in src/a.rs, we decided to use quaternions");
    engine.wait_idle();
    let before = (count_entities(&engine, "mem"), count_entities(&engine, "task"), count_edges(&engine));
    let scope = Scope::workspace("user-001", &common::workspace_id(&engine));
    let job = JobRecord::new(JobKind::ExtractMemory, scope, JobPriority::Interactive, receipt.seq, receipt.event_id);
    for _ in 0..2 {
        let key = layout::pending_job(T, job.source_seq, job.job_id.as_str());
        engine.engine().store.put_sync(Cf::Jobs, &key, &job).unwrap();
        engine.engine().ingest.run_job(&job).unwrap();
    }
    let after = (count_entities(&engine, "mem"), count_entities(&engine, "task"), count_edges(&engine));
    assert_eq!(before, after);
    assert!(before.0 >= 2 && before.1 == 1);
}

#[test]
fn full_queue_keeps_jobs_durable_and_processes_them_later() {
    let engine = TestEngine::start_with(|c| {
        c.ingest.queue_capacity = 1;
        c.ingest.worker_count = 1;
    });
    let receipts: Vec<_> = (0..20).map(|i| engine.capture("user.message", "s1", None, &format!("message number {i}"))).collect();
    assert!(receipts.iter().all(|r| r.durable));
    engine.wait_idle();
    for receipt in receipts {
        assert!(!zcie_memory::memories_of_event(&engine.engine().store, T, &receipt.event_id).unwrap().is_empty());
    }
}
