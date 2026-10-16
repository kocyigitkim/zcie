//! TS-07 — Search, query and context (`docs/13-test-plan.md` section 5).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::atomic::Ordering;

use zcie_tests::{TestEngine, admin, fixture_repo};
use zcie_types::{
    AnswerKind, ExpandLevel, ExpandRequest, ExpandStatus, QueryClass, QueryMode, QueryRequest, QueryResponse,
};

fn indexed() -> TestEngine {
    let engine = TestEngine::start();
    let repo = fixture_repo(engine.dir.path(), true);
    engine.register(&repo);
    engine.wait_idle();
    engine
}

fn symbol_ref(response: &QueryResponse, title: &str) -> String {
    response.results.iter().find(|r| r.kind == "symbol" && r.title.ends_with(title)).expect("symbol result").entity_ref.clone()
}

fn expand(engine: &TestEngine, entity_ref: &str, expected: Option<&str>) -> zcie_types::ExpandItem {
    let request = ExpandRequest {
        workspace: "atheron".into(),
        refs: vec![entity_ref.into()],
        level: ExpandLevel::Source,
        token_budget: Some(2000),
        expected_revision: expected.map(str::to_string),
    };
    engine.engine().expand(&admin(), request).expect("expand").items.remove(0)
}

#[test]
fn ts_07_01_definition_question_uses_exact_symbol_route() {
    let engine = indexed();
    let response = engine.query("where is attach_to_bone defined", 250);
    assert_eq!(response.class, QueryClass::SymbolDefinition);
    assert_eq!(response.answer_kind, AnswerKind::Local);
    assert!(symbol_ref(&response, "attach_to_bone").starts_with("sym:"));
}

#[test]
fn ts_07_02_word_only_in_notes_uses_lexical_route() {
    let engine = indexed();
    let response = engine.query("quaternion interpolation", 250);
    assert_eq!(response.class, QueryClass::Search);
    assert!(response.results.iter().any(|r| r.kind == "file" && r.path.as_deref() == Some("docs/notes.md")));
}

#[test]
fn ts_07_03_second_identical_query_is_a_cache_hit() {
    let engine = indexed();
    let hits_before = engine.engine().ingest.counters.cache_hits.load(Ordering::Relaxed);
    let first = engine.query("quaternion interpolation", 250);
    let second = engine.query("quaternion interpolation", 250);
    assert_eq!(first.query_id, second.query_id);
    assert_eq!(engine.engine().ingest.counters.cache_hits.load(Ordering::Relaxed), hits_before + 1);
}

#[test]
fn ts_07_05_response_obeys_token_budget_and_is_valid_json() {
    let engine = indexed();
    let response = engine.query("weapon socket bone", 250);
    let json = serde_json::to_string(&response).unwrap();
    assert!(zcie_types::estimate_tokens(&json) <= 250, "tokens {}", zcie_types::estimate_tokens(&json));
    let _: serde_json::Value = serde_json::from_str(&json).unwrap();
}

#[test]
fn ts_07_06_small_budget_truncates_with_cursor_and_no_cut_items() {
    let engine = indexed();
    let response = engine.query("weapon socket bone animation report", 60);
    assert!(response.truncated);
    let cursor = response.cursor.clone().expect("cursor");
    let symbols: Vec<_> = response.results.iter().filter(|r| r.kind == "symbol").collect();
    assert!(symbols.iter().all(|r| r.range.is_some()));
    let request = QueryRequest {
        workspace: "atheron".into(),
        question: "weapon socket bone animation report".into(),
        mode: QueryMode::Auto,
        token_budget: Some(400),
        cursor: Some(cursor),
        ..QueryRequest::default()
    };
    let next = engine.engine().query(&admin(), request).expect("next page");
    assert!(!next.results.is_empty());
    let first_refs: Vec<_> = response.results.iter().map(|r| &r.entity_ref).collect();
    assert!(next.results.iter().all(|r| !first_refs.contains(&&r.entity_ref)));
}

#[test]
fn ts_07_07_query_without_verified_edge_makes_no_verified_claim() {
    let engine = indexed();
    engine.capture("user.message", "s1", Some("t1"), "Please fix attachToBone for #77");
    engine.wait_idle();
    let response = engine.query("which commit implemented #77", 1000);
    assert_ne!(response.answer_kind, AnswerKind::Local);
    assert!(response.answer.is_none());
    assert!(!response.results.iter().any(|r| r.kind == "commit"));
    assert!(response.note.is_some());
}

#[test]
fn ts_07_08_expand_symbol_at_source_level_returns_span_lines() {
    let engine = indexed();
    let response = engine.query("where is attach_to_bone defined", 250);
    let item = expand(&engine, &symbol_ref(&response, "attach_to_bone"), None);
    assert_eq!(item.status, ExpandStatus::Ok);
    let content = item.content.expect("content");
    assert!(content.trim_start().starts_with("pub fn attach_to_bone"));
    assert!(content.trim_end().ends_with('}'));
}

#[test]
fn ts_07_09_expand_with_old_revision_reports_changed() {
    let engine = indexed();
    let response = engine.query("where is attach_to_bone defined", 250);
    let item = expand(&engine, &symbol_ref(&response, "attach_to_bone"), Some("deadbeef"));
    assert_eq!(item.status, ExpandStatus::Changed);
}

#[test]
fn ts_07_10_rebuilt_search_index_gives_same_results() {
    let engine = indexed();
    let refs = |r: QueryResponse| r.results.into_iter().map(|i| i.entity_ref).collect::<Vec<_>>();
    let before = refs(engine.query("quaternion interpolation hilt", 1000));
    engine.engine().rebuild_search(&admin()).expect("rebuild");
    engine.wait_idle();
    let after = refs(engine.query("quaternion interpolation hilt", 1000));
    assert!(!before.is_empty());
    assert_eq!(before, after);
}

