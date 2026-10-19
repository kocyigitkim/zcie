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

#[test]
fn refs_in_a_long_pasted_document_make_no_tasks() {
    let engine = TestEngine::start();
    let document = format!("<pasted_content id=\"1\">Spec. Example: fix #89 and STE-100. {}</pasted_content>", "Details of the design. ".repeat(120));
    engine.capture("user.message", "s-doc", Some("1"), &document);
    engine.capture("user.message", "s-doc", Some("2"), "Fix #90 weapon socket");
    engine.wait_idle();
    let tasks = engine.engine().store.prefix_scan::<zcie_types::TaskRecord>(zcie_store::Cf::Entities, &zcie_store::layout::entity_prefix("local", "task"), 100).unwrap();
    let refs: Vec<String> = tasks.into_iter().filter_map(|(_, t)| t.external_ref).collect();
    assert_eq!(refs, ["#90"], "only the short request makes a task");
}

#[test]
fn plain_correction_message_supersedes_the_old_statement() {
    let engine = TestEngine::start();
    engine.capture("user.message", "s-c1", Some("1"), "Veritabanı olarak PostgreSQL 16 kullanıyoruz; MySQL'i reddettik.");
    engine.capture("user.message", "s-c1", Some("2"), "Rate limit dakikada 600 istek olacak.");
    engine.wait_idle();
    engine.capture("user.message", "s-c2", Some("1"), "Düzeltme: PostgreSQL 16 değil, PostgreSQL 17 kullanacağız.");
    engine.wait_idle();
    let records = engine.engine().store.prefix_scan::<zcie_types::MemoryRecord>(zcie_store::Cf::Entities, &zcie_store::layout::entity_prefix("local", "mem"), 100).unwrap();
    let status = |needle: &str| records.iter().find(|(_, r)| r.text.starts_with(needle)).map(|(_, r)| r.status);
    assert_eq!(status("Veritabanı olarak PostgreSQL 16"), Some(zcie_types::MemoryStatus::Superseded));
    assert_eq!(status("Rate limit dakikada 600"), Some(zcie_types::MemoryStatus::Active), "an unrelated statement stays");
    assert_eq!(status("Düzeltme: PostgreSQL 16 değil"), Some(zcie_types::MemoryStatus::Active));
    let response = engine.query("Hangi PostgreSQL sürümünü kullanıyoruz?", 1000);
    let first_memory = response.results.iter().find(|r| r.kind == "memory").map(|r| r.title.clone()).unwrap_or_default();
    assert!(first_memory.contains("17"), "{first_memory}");
}

#[test]
fn list_all_decisions_returns_every_current_statement() {
    let engine = TestEngine::start();
    let facts = ["Tutarlar kuruş cinsinden i64.", "Kod incelemesini Elif yapıyor.", "Rate limit dakikada 600 istek.", "Sürümler her ayın ilk salı çıkıyor.", "Ödeme sağlayıcısı İyzico.", "Loglarda e-posta maskeleme zorunlu.", "Doc comment'ler İngilizce.", "Gecikme faizi 2.25 olacak."];
    for (i, fact) in facts.iter().enumerate() {
        engine.capture("user.message", "s-list", Some(&format!("{i}")), fact);
    }
    engine.capture("user.message", "s-list", Some("20"), "Düzeltme: rate limit 600 değil, dakikada 900 istek.");
    engine.capture("user.message", "s-list", Some("21"), "Hangi PostgreSQL sürümünü kullanıyoruz?");
    engine.wait_idle();
    let response = engine.query("Bu projede verdiğimiz bütün kararları ve kuralları madde madde listele.", 250);
    let titles: Vec<&str> = response.results.iter().map(|r| r.title.as_str()).collect();
    assert!(response.note.as_deref().unwrap_or_default().starts_with(zcie_query::listing::LIST_NOTE), "{:?}", response.note);
    for needle in ["i64", "Elif", "900", "salı", "İyzico", "maskeleme", "İngilizce", "2.25"] {
        assert!(titles.iter().any(|t| t.contains(needle)), "{needle} is missing: {titles:?}");
    }
    assert!(!titles.iter().any(|t| t.starts_with("Rate limit dakikada 600")), "the corrected statement is not listed");
    assert!(!titles.iter().any(|t| t.contains('?')), "stored questions are not listed");
}

#[test]
fn follow_up_message_is_found_by_the_topic_of_the_message_before_it() {
    let engine = TestEngine::start();
    engine.capture("user.message", "s-f", Some("1"), "Claude Code araç tanımları her istekte çok token harcıyor.");
    engine.capture("user.message", "s-other", Some("1"), "Loglarda e-posta adresleri maskelenmeli.");
    engine.capture("user.message", "s-f", Some("2"), "Bunu Claude yine otomatik yükleyebilsin fakat maliyet artmasın.");
    engine.wait_idle();
    let records = engine.engine().store.prefix_scan::<zcie_types::MemoryRecord>(zcie_store::Cf::Entities, &zcie_store::layout::entity_prefix("local", "mem"), 100).unwrap();
    let context = |needle: &str| records.iter().find(|(_, r)| r.text.starts_with(needle)).and_then(|(_, r)| r.context.clone());
    assert_eq!(context("Bunu Claude").as_deref(), Some("Claude Code araç tanımları her istekte çok token harcıyor."), "the message before it in the same session");
    assert_eq!(context("Claude Code araç"), None, "the first message of a session has no context");
    assert_eq!(context("Loglarda"), None);
    let response = engine.query("Kullanıcı araç tanımları konusunda ne istedi?", 1000);
    let follow_up = response.results.iter().find(|r| r.title.starts_with("Bunu Claude")).expect("the follow-up message is found by its topic");
    assert!(follow_up.context.as_deref().is_some_and(|c| c.contains("araç tanımları")));
}

#[test]
fn context_backfill_fills_records_stored_before_the_context_existed() {
    let engine = TestEngine::start();
    engine.capture("user.message", "s-b", Some("1"), "Vektör araması çok dilli olmalı.");
    engine.capture("user.message", "s-b", Some("2"), "Bunu varsayılan yap.");
    engine.wait_idle();
    let store = &engine.engine().store;
    let prefix = zcie_store::layout::entity_prefix("local", "mem");
    let (key, mut record) = store.prefix_scan::<zcie_types::MemoryRecord>(zcie_store::Cf::Entities, &prefix, 100).unwrap().into_iter().find(|(_, r)| r.text.starts_with("Bunu")).unwrap();
    record.context = None;
    store.put_sync(zcie_store::Cf::Entities, &key, &record).unwrap();
    assert_eq!(zcie_memory::backfill_context(store, &engine.engine().objects, T).unwrap(), 0, "the migration ran at startup already");
    let mut batch = Batch::new();
    batch.delete(zcie_store::Cf::Registry, zcie_store::layout::migration(T, "follow_up_context_v2"));
    store.write_batch(batch, true).unwrap();
    assert_eq!(zcie_memory::backfill_context(store, &engine.engine().objects, T).unwrap(), 1);
    let stored: zcie_types::MemoryRecord = store.get(zcie_store::Cf::Entities, &key).unwrap().unwrap();
    assert_eq!(stored.context.as_deref(), Some("Vektör araması çok dilli olmalı."));
}

#[test]
fn shell_commands_typed_with_an_exclamation_mark_are_not_requests() {
    let engine = TestEngine::start();
    engine.capture("user.message", "s-sh", Some("1"), "Önbellek süresi 10 dakika olmalı.");
    engine.capture("user.message", "s-sh", Some("2"), "! zcie doctor --fix");
    engine.capture("user.message", "s-sh", Some("3"), "Bunu 5 dakikaya düşür.");
    engine.wait_idle();
    let records = engine.engine().store.prefix_scan::<zcie_types::MemoryRecord>(zcie_store::Cf::Entities, &zcie_store::layout::entity_prefix("local", "mem"), 100).unwrap();
    assert!(!records.iter().any(|(_, r)| r.text.starts_with('!')), "no record for a shell command");
    let follow_up = records.iter().find(|(_, r)| r.text.starts_with("Bunu")).map(|(_, r)| r.context.clone()).unwrap();
    assert_eq!(follow_up.as_deref(), Some("Önbellek süresi 10 dakika olmalı."), "the shell command is skipped as context");
}

#[test]
fn a_sentence_of_a_long_document_does_not_match_by_other_parts_of_the_document() {
    let engine = TestEngine::start();
    let filler = "Bu bölüm performans hedeflerini anlatır ve ölçüm yöntemini açıklar. ".repeat(30);
    let document = format!("<pasted_content id=\"1\">\nKurallar:\nNever fabricate missing relations.\n{filler}\nGeliştirme sırasında benchmark ile doğrulayacağımız hedefler vardır. Kullanıcı soru listesi tutar.\n</pasted_content>");
    engine.capture("user.message", "s-doc", Some("1"), &document);
    engine.capture("user.message", "s-ask", Some("1"), "devam et. bana soru sorma, best practice seç.");
    engine.wait_idle();
    let response = engine.query("Kullanıcı geliştirme sırasında kendisine soru sorulmasını istiyor mu?", 1000);
    let memories: Vec<&str> = response.results.iter().filter(|r| r.kind == "memory").map(|r| r.title.as_str()).collect();
    let rank = |needle: &str| memories.iter().position(|t| t.contains(needle));
    assert!(rank("soru sorma").is_some_and(|ask| rank("Never fabricate").is_none_or(|never| ask < never)), "{memories:?}");
    let never = response.results.iter().find(|r| r.title.starts_with("Never fabricate"));
    assert!(never.is_none_or(|r| r.snippet.as_deref().is_none_or(|s| s.chars().count() <= 400)), "a sentence record shows the text around it");
}

#[test]
fn a_top_request_shows_the_next_short_messages_of_its_session() {
    let engine = TestEngine::start();
    engine.capture("user.message", "s-next", Some("1"), "Bu toolların hepsi context'te bilinmeli fakat araç tanımları minimum token harcamalı.");
    engine.capture("user.message", "s-next", Some("2"), "Peki toollar otomatik mi çalışıyor?");
    engine.capture("user.message", "s-next", Some("3"), "Peki şunu istiyorum: Claude bunu yine otomatik yükleyebilsin fakat maliyet artmasın.");
    engine.capture("user.message", "s-next", Some("4"), &"Yeni konu: uzun bir belge. ".repeat(40));
    engine.wait_idle();
    let response = engine.query("Kullanıcı araç tanımları ve token konusunda ne istedi?", 1000);
    let first = response.results.iter().find(|r| r.title.starts_with("Bu toolların")).expect("the request");
    let next = first.follow_up.as_deref().unwrap_or_default();
    // A next message that the response already shows as a result is not repeated (ADR-017).
    let shown = |text: &str| next.contains(text) || response.results.iter().any(|r| !std::ptr::eq(r, first) && r.title.contains(text));
    assert!(shown("otomatik mi") && next.contains("otomatik yükleyebilsin"), "{next}");
    assert!(!next.contains("Yeni konu"), "a long message is not a follow-up");
}
