//! TS-08 — Multitenancy and scope.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use zcie_api::ApiError;
use zcie_tests::engine::capture_request;
use zcie_tests::{TestEngine, admin, read_only_auth, workspace_auth};
use zcie_types::{QueryRequest, QueryResponse};

fn capture_in(engine: &TestEngine, workspace: &str, text: &str) {
    let mut request = capture_request("user.message", "s1", Some("t1"), text);
    request.workspace = workspace.into();
    engine.engine().capture_event(&admin(), request).expect("capture");
}

fn query_as(engine: &TestEngine, auth: &zcie_security::AuthContext, workspace: &str, question: &str) -> Result<QueryResponse, ApiError> {
    let request = QueryRequest { workspace: workspace.into(), question: question.into(), token_budget: Some(1000), ..QueryRequest::default() };
    engine.engine().query(auth, request)
}

fn two_workspaces() -> TestEngine {
    let engine = TestEngine::start();
    capture_in(&engine, "ws-a", "Alpha requirement for #5 about the sword hilt");
    capture_in(&engine, "ws-b", "Beta requirement for #5 about the sword hilt");
    engine.wait_idle();
    engine
}

#[test]
fn ts_08_01_client_of_workspace_a_cannot_query_workspace_b() {
    let engine = two_workspaces();
    let auth = workspace_auth("alice", &["ws-a"]);
    assert!(matches!(query_as(&engine, &auth, "ws-b", "sword"), Err(ApiError::Forbidden(_))));
    assert!(query_as(&engine, &auth, "ws-a", "sword").is_ok());
}

#[test]
fn ts_08_02_traversal_from_workspace_a_reaches_no_node_of_workspace_b() {
    let engine = two_workspaces();
    let b_refs: Vec<String> = query_as(&engine, &admin(), "ws-b", "#5").unwrap().results.into_iter().map(|r| r.entity_ref).collect();
    assert!(!b_refs.is_empty());
    let a = query_as(&engine, &workspace_auth("alice", &["ws-a"]), "ws-a", "#5 sword hilt requirement").unwrap();
    assert!(!a.results.is_empty());
    assert!(a.results.iter().all(|r| !b_refs.contains(&r.entity_ref)));
    assert!(a.results.iter().all(|r| !r.title.contains("Beta")));
}

#[test]
fn ts_08_03_same_query_from_two_principals_uses_two_cache_entries() {
    let engine = two_workspaces();
    let alice = workspace_auth("alice", &["ws-a"]);
    let carol = workspace_auth("carol", &["ws-a"]);
    let a1 = query_as(&engine, &alice, "ws-a", "sword hilt").unwrap();
    let a2 = query_as(&engine, &alice, "ws-a", "sword hilt").unwrap();
    let c1 = query_as(&engine, &carol, "ws-a", "sword hilt").unwrap();
    assert_eq!(a1.query_id, a2.query_id);
    assert_ne!(a1.query_id, c1.query_id);
}

#[test]
fn ts_08_04_read_only_key_cannot_capture() {
    let engine = TestEngine::start();
    let request = capture_request("user.message", "s1", None, "hello");
    let result = engine.engine().capture_event(&read_only_auth("bob", &["atheron"]), request);
    assert!(matches!(result, Err(ApiError::Forbidden(_))));
}
