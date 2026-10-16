//! TS-07-04 — Read-your-writes: a changed file is never returned as current old content.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use zcie_tests::{TestEngine, fixture_repo};
use zcie_types::Freshness;

#[test]
fn ts_07_04_changed_file_is_not_returned_as_current_old_content() {
    let engine = TestEngine::start();
    let repo = fixture_repo(engine.dir.path(), true);
    engine.register(&repo);
    engine.wait_idle();
    let file = repo.join("src/animation/weapon_socket.rs");
    let text = std::fs::read_to_string(&file).unwrap().replace("attach_to_bone", "hold_on_bone");
    std::fs::write(&file, text).unwrap();
    engine.capture("file.changed", "s1", Some("t1"), r#"{"path":"src/animation/weapon_socket.rs"}"#);
    let response = engine.query("where is attach_to_bone defined", 250);
    let stale = response.results.iter().any(|r| r.kind == "symbol" && r.title.ends_with("attach_to_bone"));
    assert!(!(stale && response.freshness == Freshness::Current), "old symbol returned as current");
    engine.wait_idle();
    let after = engine.query("where is attach_to_bone defined", 250);
    assert!(!after.results.iter().any(|r| r.kind == "symbol" && r.title.ends_with("attach_to_bone")));
    let new = engine.query("where is hold_on_bone defined", 250);
    assert_eq!(new.freshness, Freshness::Current);
    assert!(new.answer.unwrap_or_default().contains("weapon_socket.rs"));
}
