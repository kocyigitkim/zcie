//! TK-01 and TK-02 — Token regression (`docs/13-test-plan.md` section 6).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeMap;
use std::path::PathBuf;

use zcie_tests::{TestEngine, fixture_repo};

const DEFAULT_BUDGET: u32 = 250;
const QUESTIONS: [&str; 20] = [
    "where is attach_to_bone defined",
    "where is WeaponSocket defined",
    "definition of update_animation",
    "build_report",
    "Report",
    "who calls attach_to_bone",
    "quaternion interpolation",
    "weapon socket bone",
    "animation controller",
    "report path basename",
    "what did we do for weapon attachment",
    "history of the weapon socket",
    "which commit implemented #89",
    "#89",
    "attachToBone",
    "bone name console log",
    "idle and attack poses jitter",
    "broken function",
    "complete_function",
    "nonexistent symbol xyz",
];

fn token_counts() -> BTreeMap<String, u32> {
    let engine = TestEngine::start();
    let repo = fixture_repo(engine.dir.path(), true);
    engine.register(&repo);
    engine.capture("user.message", "s1", Some("t1"), "Weapon attachment moves during animation #89. Fix attachToBone.");
    engine.wait_idle();
    QUESTIONS
        .iter()
        .map(|q| {
            let response = engine.query(q, DEFAULT_BUDGET);
            (q.to_string(), zcie_types::estimate_tokens(&serde_json::to_string(&response).unwrap()))
        })
        .collect()
}

#[test]
fn tk_01_and_tk_02_responses_stay_within_budget_and_baseline() {
    let counts = token_counts();
    for (question, tokens) in &counts {
        assert!(*tokens <= DEFAULT_BUDGET, "TK-01: {question} uses {tokens} tokens");
    }
    let baseline_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("token-regression").join("baseline.json");
    if !baseline_path.exists() {
        std::fs::write(&baseline_path, serde_json::to_string_pretty(&counts).unwrap()).unwrap();
        return;
    }
    let baseline: BTreeMap<String, u32> = serde_json::from_slice(&std::fs::read(&baseline_path).unwrap()).unwrap();
    for (question, tokens) in &counts {
        if let Some(base) = baseline.get(question) {
            let limit = (*base as f64 * 1.10).ceil() as u32;
            assert!(*tokens <= limit.max(*base + 2), "TK-02: {question} grew from {base} to {tokens}");
        }
    }
}
