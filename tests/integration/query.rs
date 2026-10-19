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
        with: Vec::new(),
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


#[test]
fn ts_07_11_match_coverage_is_high_for_a_matching_question_and_zero_for_chat() {
    let engine = indexed();
    let matching = engine.query("which function attaches to the bone", 250);
    assert!(matching.match_coverage.unwrap_or(0.0) >= 0.6, "{:?}", matching.match_coverage);
    let chat = engine.query("hello, thank you very much", 250);
    assert_eq!(chat.match_coverage, Some(0.0));
}

#[test]
fn ts_07_12_commit_question_without_a_task_falls_back_to_search() {
    let engine = TestEngine::start();
    let repo = zcie_tests::fixture_repo(engine.dir.path(), true);
    std::fs::write(repo.join("docs/notes.md"), "Bone offsets use quaternions.\n").unwrap();
    zcie_tests::commit_all(&repo, "fix: raise the bone offset limit from 4 to 6");
    engine.register(&repo);
    engine.wait_idle();
    let response = engine.query("which commit changed the bone offset limit", 250);
    assert_eq!(response.class, QueryClass::Implementation);
    assert!(response.results.iter().any(|r| r.kind == "commit" && r.title.contains("bone offset limit")), "{:?}", response.results);
}

#[test]
fn ts_07_13_same_symbol_in_two_worktrees_of_a_project_is_one_result() {
    let engine = TestEngine::start();
    let first = zcie_tests::fixture_repo(&engine.dir.path().join("a"), true);
    let second = zcie_tests::fixture_repo(&engine.dir.path().join("b"), true);
    engine.register(&first);
    engine.register(&second);
    engine.wait_idle();
    let response = engine.query("where is attach_to_bone defined", 1000);
    let copies = response.results.iter().filter(|r| r.kind == "symbol" && r.title.ends_with("attach_to_bone") && r.path.as_deref() == Some("src/animation/weapon_socket.rs")).count();
    assert_eq!(copies, 1, "{:?}", response.results);
}

#[test]
fn ts_07_14_long_message_shows_the_passage_that_answers_the_question() {
    let engine = indexed();
    let filler = "The weapon socket keeps the attachment offsets of each bone in a table. ".repeat(40);
    let text = format!("{filler}Our release train leaves on the first Tuesday of each month. {filler}");
    engine.capture("user.message", "s-long", Some("1"), &text);
    engine.wait_idle();
    let response = engine.query("when does the release train leave", 1000);
    let item = response.results.iter().find(|r| r.kind == "memory" || r.kind == "event").expect("the long message is found");
    let snippet = item.snippet.as_deref().unwrap_or_default();
    assert!(snippet.contains("first Tuesday"), "{snippet}");
    assert!(snippet.chars().count() <= zcie_query::relevance::SNIPPET_WINDOW_CHARS + 4, "{}", snippet.len());
}

#[test]
fn ts_07_15_turkish_inflected_question_finds_the_fact() {
    let engine = indexed();
    engine.capture("user.message", "s-tr", Some("1"), "Müşteri API'si için rate limit dakikada 600 istek olacak.");
    engine.capture("user.message", "s-tr", Some("2"), "En önemli mimari kararım: Çekirdek motor Rust ile geliştirilmeli.");
    engine.wait_idle();
    let limit = engine.query("Müşteri API'sine dakikada kaç istek sınırı koyduk?", 1000);
    assert!(limit.results.iter().any(|r| r.title.contains("600")), "{:?}", limit.results);
    assert!(limit.match_coverage.unwrap_or(0.0) >= 0.6, "{:?}", limit.match_coverage);
    let core = engine.query("Kullanıcı çekirdek motorun hangi dille yazılmasını istedi?", 1000);
    assert_eq!(core.class, QueryClass::Timeline);
    assert!(core.results.iter().any(|r| r.title.contains("Rust")), "{:?}", core.results);
}

#[test]
fn ts_07_16_two_search_rebuilds_keep_the_conversations() {
    let engine = indexed();
    engine.capture("user.message", "s-rb", Some("1"), "Decision: the pendulum cache keeps entries for nine minutes.");
    engine.wait_idle();
    engine.engine().rebuild_search(&admin()).unwrap();
    engine.engine().rebuild_search(&admin()).unwrap();
    engine.wait_idle();
    let response = engine.query("pendulum cache nine minutes", 1000);
    assert!(response.results.iter().any(|r| r.title.contains("pendulum cache")), "{:?}", response.results);
}

#[test]
fn ts_07_17_passage_of_a_long_pasted_document_is_found() {
    let engine = indexed();
    let filler = "The design has many parts and each part has a purpose in the system. ".repeat(300);
    let text = format!("<pasted_content id=\"1\">{filler}My most important architecture decision: the core engine is written in Rust. {filler}</pasted_content>");
    engine.capture("user.message", "s-spec", Some("1"), &text);
    engine.wait_idle();
    let response = engine.query("in which language is the core engine written", 1000);
    let found = response.results.iter().any(|r| r.snippet.as_deref().unwrap_or_default().contains("written in Rust"));
    assert!(found, "{:?}", response.results.iter().map(|r| (&r.kind, &r.title)).collect::<Vec<_>>());
}

#[test]
fn ts_07_18_tool_logs_do_not_push_out_the_user_message() {
    let engine = indexed();
    engine.capture("user.message", "s-log", Some("1"), "Decision: the quokka importer runs every night.");
    for i in 0..12 {
        let payload = format!("{{\"command\":\"grep quokka importer nightly {i}\",\"exit_code\":0,\"output\":\"quokka importer nightly\"}}");
        engine.capture("command.run", "s-log", Some(&format!("{}", i + 2)), &payload);
    }
    engine.wait_idle();
    let response = engine.query("when does the quokka importer run", 600);
    assert_eq!(response.results.first().map(|r| r.kind.as_str()), Some("memory"), "{:?}", response.results.iter().map(|r| &r.title).collect::<Vec<_>>());
}

#[test]
fn ts_17_11_turkish_and_english_words_of_one_meaning_match() {
    use zcie_query::synonyms::{equivalents, same_stem};
    assert!(equivalents("tooll").any(|w| w == "arac"), "toolların matches araç");
    assert!(equivalents("arac").any(|w| w == "tool"));
    assert!(same_stem("onay", "onayi") && !same_stem("tool", "token"));
    let engine = TestEngine::start();
    for (i, text) in ["Loglarda e-posta adresleri maskelenmeli.", "Bu toolların hepsi context'te bilinmeli fakat minimum token harcanmalı, maliyet artmasın.", "Sürümler her ayın ilk salı çıkıyor."].iter().enumerate() {
        engine.capture("user.message", "s-syn", Some(&i.to_string()), text);
    }
    engine.wait_idle();
    let response = engine.query("Kullanıcı araç tanımları ve token maliyeti hakkında ne istedi?", 1000);
    let first = response.results.iter().find(|r| r.kind == "memory").map(|r| r.title.clone()).unwrap_or_default();
    assert!(first.contains("toolların"), "{first}");
    assert!(response.match_coverage.unwrap_or_default() >= 0.6, "{:?}", response.match_coverage);
}

#[test]
fn ts_17_13_generic_format_words_are_not_names() {
    use zcie_query::relevance::match_coverage;
    let item = |kind: &str, title: &str| zcie_types::ResultItem {
        entity_ref: format!("x:{title}"),
        kind: kind.into(),
        title: title.into(),
        path: None,
        range: None,
        status: Some("active".into()),
        evidence_status: Some("user_requirement:tier_a.v1".into()),
        revision: None,
        score: 1.0,
        snippet: None,
        project: None,
        context: None,
        follow_up: None,
    };
    assert_eq!(match_coverage("JSON ile YAML arasındaki fark ne?", &[item("symbol", "json_tokens"), item("memory", "JSON yanıtları küçük tut.")]), 0.0);
    assert_eq!(match_coverage("kısa bir commit mesajı öner: README düzeltildi", &[item("memory", "README dosyasını Türkçe yaz.")]), 0.0);
    assert!(match_coverage("Hangi PostgreSQL sürümünü kullanıyoruz?", &[item("memory", "PostgreSQL 17 kullanacağız.")]) >= 0.6, "a project name still counts");
    assert_eq!(match_coverage("Hangi PostgreSQL sürümünü kullanıyoruz?", &[item("symbol", "PostgresPool")]), 0.0, "a symbol does not count as a name match");
}
