//! TS-10 — Forget.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use zcie_tests::engine::capture_request;
use zcie_tests::{TestEngine, admin};
use zcie_types::api::{ForgetRequest, ForgetSelector};

const SECRET_TEXT: &str = "Zebra unicorn marmalade protocol must stay private";

fn forget_event(engine: &TestEngine, event_id: &str) {
    let request = ForgetRequest {
        workspace: "atheron".into(),
        selector: ForgetSelector { entity_ref: Some(format!("ev:{event_id}")), ..ForgetSelector::default() },
        reason: "test".into(),
    };
    let receipt = engine.engine().forget(&admin(), request).expect("forget");
    assert_eq!(receipt.events_removed, 1);
}

fn mentions(engine: &TestEngine, event_id: &str) -> bool {
    let response = engine.query("unicorn marmalade protocol", 1000);
    response.results.iter().any(|r| r.entity_ref.contains(event_id) || r.title.contains("unicorn"))
}

#[test]
fn ts_10_01_forgotten_event_is_not_returned() {
    let engine = TestEngine::start();
    let receipt = engine.capture("user.message", "s1", Some("t1"), SECRET_TEXT);
    engine.wait_idle();
    assert!(mentions(&engine, &receipt.event_id));
    forget_event(&engine, &receipt.event_id);
    engine.wait_idle();
    assert!(!mentions(&engine, &receipt.event_id));
}

#[test]
fn ts_10_02_forgotten_hit_is_dropped_before_search_delete_completes() {
    let engine = TestEngine::start();
    let receipt = engine.capture("user.message", "s1", Some("t1"), SECRET_TEXT);
    engine.wait_idle();
    forget_event(&engine, &receipt.event_id);
    assert!(!mentions(&engine, &receipt.event_id));
}

#[test]
fn ts_10_03_replay_after_forget_is_a_duplicate() {
    let engine = TestEngine::start();
    let mut request = capture_request("user.message", "s1", Some("t1"), SECRET_TEXT);
    request.idempotency_key = "test:fixed-key".into();
    let first = engine.engine().capture_event(&admin(), request.clone()).unwrap();
    engine.wait_idle();
    forget_event(&engine, &first.event_id);
    let again = engine.engine().capture_event(&admin(), request).unwrap();
    assert!(again.duplicate);
    assert_eq!(again.event_id, first.event_id);
}

#[test]
fn ts_10_04_blob_of_only_forgotten_event_is_deleted() {
    let engine = TestEngine::start();
    let receipt = engine.capture("user.message", "s1", Some("t1"), SECRET_TEXT);
    engine.wait_idle();
    let hash = zcie_store::ObjectStore::hash(SECRET_TEXT.as_bytes());
    let hex = hash.trim_start_matches("b3:");
    let blob = engine.home().join("instances/local/state/objects").join(&hex[0..2]).join(&hex[2..4]).join(format!("{hex}.zst"));
    assert!(blob.exists());
    forget_event(&engine, &receipt.event_id);
    let swept = engine.engine().objects.sweep_orphans(&engine.engine().store, "local", std::time::Duration::ZERO).unwrap();
    assert!(!blob.exists(), "blob still exists (sweep removed {swept})");
}
