//! TS-16 — Vector search in the engine (ADR-011). Needs the model in `ZCIE_TEST_MODEL` or
//! `~/.zcie/models/potion-base-8M` (`zcie vector install`); without it the tests print a reason and return.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;
use std::time::{Duration, Instant};

use zcie_tests::{TestEngine, admin};
use zcie_types::api::{ForgetRequest, ForgetSelector};

const DECISION: &str = "Decision: the ledger keeps all balances in Postgres 16.";
const QUESTION: &str = "where is financial data persisted";

fn model_folder() -> Option<PathBuf> {
    let folder = std::env::var("ZCIE_TEST_MODEL").map(PathBuf::from).unwrap_or_else(|_| {
        PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(".zcie/models/potion-base-8M")
    });
    if folder.join("model.safetensors").exists() {
        return Some(folder);
    }
    println!("skipped: no model in {}", folder.display());
    None
}

fn engine(vector: bool, folder: &std::path::Path) -> TestEngine {
    let model = folder.display().to_string();
    TestEngine::start_with(|config| {
        config.vector.enabled = vector;
        config.vector.model = model;
    })
}

fn rank(engine: &TestEngine) -> Option<usize> {
    let response = engine.query(QUESTION, 2000);
    response.results.iter().position(|r| r.title.contains("Postgres 16")).map(|p| p + 1)
}

#[test]
fn ts_16_01_semantic_memory_question_is_found_only_with_vectors() {
    let Some(folder) = model_folder() else { return };
    let mut ranks = Vec::new();
    for vector in [false, true] {
        let engine = engine(vector, &folder);
        engine.capture("user.message", "s-v", Some("1"), DECISION);
        engine.wait_idle();
        ranks.push(rank(&engine));
    }
    assert_eq!(ranks[0], None, "the question shares no word with the decision and its class reads the search index");
    assert_eq!(ranks[1], Some(1), "{ranks:?}");
}

#[test]
fn ts_16_02_vectors_are_filled_at_start_and_follow_forget() {
    let Some(folder) = model_folder() else { return };
    let mut engine = engine(true, &folder);
    engine.capture("user.message", "s-v", Some("1"), DECISION);
    engine.wait_idle();
    engine.restart();
    let started = Instant::now();
    while rank(&engine).is_none() && started.elapsed() < Duration::from_secs(20) {
        std::thread::sleep(Duration::from_millis(100));
    }
    assert_eq!(rank(&engine), Some(1), "the startup fill restores the vectors");
    let selector = ForgetSelector { session_id: Some("s-v".into()), ..ForgetSelector::default() };
    engine.engine().forget(&admin(), ForgetRequest { workspace: "atheron".into(), selector, reason: "test".into() }).unwrap();
    engine.wait_idle();
    assert_eq!(rank(&engine), None, "a forgotten record is not returned");
}
