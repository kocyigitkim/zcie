//! TS-05 — Memory and graph (`docs/13-test-plan.md` section 5).
#![allow(clippy::unwrap_used, clippy::expect_used)]

#[path = "t2_common.rs"]
mod common;

use common::{T, edges_from, requirement_of};
use zcie_graph::{Direction, TraverseOptions, neighbors, new_edge, put_edge};
use zcie_memory::extraction::tier_a::find_matches;
use zcie_store::Batch;
use zcie_tests::{TestEngine, admin, fixture_repo};
use zcie_types::api::CorrectRequest;
use zcie_types::{EdgeStatus, Evidence, EvidenceKind, MemoryStatus, Relation, Scope};

const WS: &str = "ws_graphtest";

fn evidence(kind: EvidenceKind) -> Evidence {
    Evidence { kind, source_event_ids: Vec::new(), revision: Some("r1".into()), note: None }
}

fn store_edge(engine: &TestEngine, from: &str, to: &str, status: EdgeStatus, kind: EvidenceKind) {
    let store = &engine.engine().store;
    let mut batch = Batch::new();
    let scope = Scope::workspace("p", WS);
    put_edge(store, &mut batch, new_edge(&scope, from, Relation::RelatedTo, to, status, evidence(kind))).unwrap();
    store.write_batch(batch, true).unwrap();
}

fn options(max_depth: usize) -> TraverseOptions {
    TraverseOptions { relations: Vec::new(), direction: Direction::Out, max_depth, max_fanout: 50, workspace_id: WS.into() }
}

#[test]
fn tier_a_patterns_find_each_kind_of_match() {
    let text = "Fix #89 and JIRA-12 in src/animation/weapon-socket.ts: call attach_to_bone, see cc83c0dbd665.\n\
                thread 'main' panicked at src/x.rs:3\nWe decided to use quaternions. Always run tests first. task:task_01ARZ3NDEKTSV4RRFFQ69G5FAV";
    let m = find_matches(text);
    assert!(m.paths.contains(&"src/animation/weapon-socket.ts".to_string()));
    assert!(m.symbol_names.contains(&"attach_to_bone".to_string()));
    assert!(m.external_task_refs.contains(&"#89".to_string()) && m.external_task_refs.contains(&"JIRA-12".to_string()));
    assert_eq!(m.internal_task_ids, vec!["task_01ARZ3NDEKTSV4RRFFQ69G5FAV".to_string()]);
    assert!(m.shas.contains(&"cc83c0dbd665".to_string()));
    assert_eq!(m.error_lines.len(), 1);
    assert!(m.decisions.iter().any(|d| d.contains("We decided")));
    assert!(m.preferences.iter().any(|p| p.starts_with("Always")));
}

#[test]
fn message_with_task_ref_makes_requirement_and_task() {
    let engine = TestEngine::start();
    let receipt = engine.capture("user.message", "s1", Some("1"), "Fix #89 weapon socket");
    engine.wait_idle();
    let requirement = requirement_of(&engine, &receipt.event_id);
    let ws = common::workspace_id(&engine);
    let task = zcie_memory::find_by_external_ref(&engine.engine().store, &Batch::new(), T, &ws, "#89").unwrap().expect("task");
    let edges = edges_from(&engine, &format!("task:{}", task.task_id), Relation::RequestedBy);
    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].to, format!("mem:{}", requirement.memory_id));
    assert_eq!(edges[0].status, EdgeStatus::Verified);
}

#[test]
fn temporal_evidence_lowers_verified_to_candidate() {
    let engine = TestEngine::start();
    store_edge(&engine, "task:task_A", "commit:commit_B", EdgeStatus::Verified, EvidenceKind::Temporal);
    let edges = zcie_graph::edges_from(&engine.engine().store, T, "task:task_A", None).unwrap();
    assert_eq!(edges[0].status, EdgeStatus::Candidate);
}

#[test]
fn path_with_inferred_edge_has_inferred_status() {
    let engine = TestEngine::start();
    store_edge(&engine, "mem:mem_A", "task:task_B", EdgeStatus::Verified, EvidenceKind::Derivation);
    store_edge(&engine, "task:task_B", "file:file_C", EdgeStatus::Inferred, EvidenceKind::TextMention);
    let items = neighbors(&engine.engine().store, T, "mem:mem_A", &options(2)).unwrap();
    let item = items.iter().find(|i| i.entity_ref == "file:file_C").expect("reached");
    assert_eq!(item.path_status, EdgeStatus::Inferred);
    assert_eq!(item.depth, 2);
}

#[test]
fn traversal_with_depth_one_stops_at_direct_neighbors() {
    let engine = TestEngine::start();
    store_edge(&engine, "mem:mem_A", "task:task_B", EdgeStatus::Verified, EvidenceKind::Derivation);
    store_edge(&engine, "task:task_B", "file:file_C", EdgeStatus::Verified, EvidenceKind::Derivation);
    let items = neighbors(&engine.engine().store, T, "mem:mem_A", &options(1)).unwrap();
    assert_eq!(items.len(), 1);
    assert!(items.iter().all(|i| i.depth == 1 && i.entity_ref != "file:file_C"));
}

#[test]
fn correction_supersedes_old_record_and_keeps_it() {
    let engine = TestEngine::start();
    let receipt = engine.capture("user.message", "s1", None, "Use linear interpolation for bones");
    engine.wait_idle();
    let old = requirement_of(&engine, &receipt.event_id);
    let fix = engine.capture("user.message", "s1", None, "Correction: use quaternions");
    let request = CorrectRequest { text: "Use quaternion interpolation for bones".into(), source_event_id: fix.event_id };
    let new = engine.engine().correct_memory(&admin(), old.memory_id.as_str(), request).unwrap();
    let stored = zcie_memory::get_memory(&engine.engine().store, &Batch::new(), T, old.memory_id.as_str()).unwrap().unwrap();
    assert_eq!(stored.status, MemoryStatus::Superseded);
    assert_eq!(stored.superseded_by, Some(new.memory_id.clone()));
    assert_eq!(stored.text, old.text);
    let supersedes = edges_from(&engine, &format!("mem:{}", new.memory_id), Relation::Supersedes);
    assert_eq!(supersedes[0].to, format!("mem:{}", old.memory_id));
}

#[test]
fn task_timeline_lists_events_in_sequence_order() {
    let engine = TestEngine::start();
    engine.capture("user.message", "s1", None, "Fix #89 weapon socket");
    engine.wait_idle();
    let ws = common::workspace_id(&engine);
    let task = zcie_memory::find_by_external_ref(&engine.engine().store, &Batch::new(), T, &ws, "#89").unwrap().unwrap();
    let task_ref = format!("task:{}", task.task_id);
    let payload = format!("{{\"name\":\"socket\",\"status\":\"passed\",\"refs\":[\"{task_ref}\"]}}");
    engine.capture("test.result", "s1", None, &payload);
    engine.wait_idle();
    let items = engine.engine().get_timeline(&admin(), &task_ref).unwrap();
    assert!(items.len() >= 2, "{items:?}");
    assert!(items.windows(2).all(|w| w[0].seq < w[1].seq));
    assert_eq!(items[0].event_type, "user.message");
    assert!(items.iter().any(|i| i.event_type == "test.result"));
}

fn affects_file_edges(engine: &TestEngine, message_turn: &str, file_turn: &str) -> Vec<zcie_types::Edge> {
    let repo = fixture_repo(engine.dir.path(), true);
    engine.register(&repo);
    engine.wait_idle();
    let message = engine.capture("user.message", "s1", Some(message_turn), "Fix the weapon socket alignment");
    engine.capture("file.changed", "s1", Some(file_turn), r#"{"path":"src/animation/weapon_socket.rs"}"#);
    engine.wait_idle();
    let requirement = requirement_of(engine, &message.event_id);
    edges_from(engine, &format!("mem:{}", requirement.memory_id), Relation::AffectsFile)
}

#[test]
fn file_change_in_same_turn_links_requirement_with_client_trace() {
    let engine = TestEngine::start();
    let edges = affects_file_edges(&engine, "1", "1");
    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].status, EdgeStatus::Verified);
    assert_eq!(edges[0].evidence.kind, EvidenceKind::ClientTrace);
    assert!(edges[0].to.starts_with("file:"));
}

#[test]
fn file_change_in_other_turn_makes_no_verified_link() {
    let engine = TestEngine::start();
    let edges = affects_file_edges(&engine, "1", "2");
    assert!(edges.iter().all(|e| e.status != EdgeStatus::Verified), "{edges:?}");
}

/// Regression: a decision stays findable when many similar plain messages are stored
/// (found in the Claude Code benchmark, ADR-008 I-44).
#[test]
fn decision_is_found_among_many_similar_messages() {
    let engine = zcie_tests::TestEngine::start();
    engine.capture("user.message", "d1", Some("1"), "We decided to raise the merge threshold to 12, because merges ran too often.");
    for index in 0..40 {
        let text = format!("Question {index}: what about the merge threshold in the merge policy?");
        engine.capture("user.message", "noise", Some(&index.to_string()), &text);
    }
    engine.wait_idle();
    let response = engine.query("What did we decide about the merge threshold?", 600);
    let top: Vec<&str> = response.results.iter().take(3).map(|r| r.title.as_str()).collect();
    assert!(top.iter().any(|t| t.contains("raise the merge threshold to 12")), "{top:?}");
}
