//! TS-20 — Context efficiency (ADR-017, `docs/13-test-plan.md` section 5): the engine part.
//! The hook part (TS-20-02 hook output, TS-20-03 order) is in `context_hooks.rs`.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use zcie_store::values::FileRefsValue;
use zcie_store::{Cf, layout};
use zcie_tests::{TestEngine, admin, commit_all, fixture_repo, ledger_repo};
use zcie_types::{ExpandLevel, ExpandRequest, QueryMode, QueryRequest, QueryResponse, estimate_tokens};

const GENERIC: &str = include_str!("../../benchmarks/user-ideas/generic.json");

fn ask(engine: &TestEngine, question: &str, mode: QueryMode, budget: u32) -> QueryResponse {
    let request = QueryRequest { workspace: "atheron".into(), question: question.into(), mode, token_budget: Some(budget), ..QueryRequest::default() };
    engine.engine().query(&admin(), request).expect("query")
}

fn ledger_engine(change: impl FnOnce(&mut zcie_core::EngineConfig)) -> (TestEngine, std::path::PathBuf, String) {
    let engine = TestEngine::start_with(change);
    let repo = ledger_repo(engine.dir.path());
    let worktree = engine.register(&repo).worktree_id;
    engine.wait_idle();
    (engine, repo, worktree)
}

fn file_refs(engine: &TestEngine, worktree: &str, path: &str) -> Vec<String> {
    let store = &engine.engine().store;
    let files = zcie_store::files::current_files(store, "local", worktree, 100).unwrap();
    let (_, file_id) = files.into_iter().find(|(p, _)| p == path).expect("file");
    store.get::<FileRefsValue>(Cf::Lookup, &layout::file_refs("local", &file_id)).unwrap().map(|r| r.names.into_iter().map(|(name, _)| name).collect()).unwrap_or_default()
}

#[test]
fn ts_20_01_a_full_text_and_a_follow_up_are_shown_once() {
    let engine = TestEngine::start();
    let message = "Karar: faturalar PDF olarak saklanacak. Karar: PDF dosyaları on yıl tutulacak ve sonra silinecek; bu kural bütün faturalar için geçerli.";
    engine.capture("user.message", "s-dup", Some("1"), message);
    engine.capture("user.message", "s-dup", Some("2"), "Peki fatura PDF arşivi otomatik mi silinecek?");
    engine.wait_idle();
    let response = engine.query("faturalar PDF olarak nasıl saklanacak ve kaç yıl tutulacak?", 1500);
    let texts: Vec<&str> = response.results.iter().filter(|r| matches!(r.kind.as_str(), "memory" | "event")).filter_map(|r| r.snippet.as_deref()).filter(|s| s.chars().count() >= 80).collect();
    let mut unique = texts.clone();
    unique.sort_unstable();
    unique.dedup();
    assert_eq!(texts.len(), unique.len(), "a full text is shown once: {texts:?}");
    for item in response.results.iter().filter(|r| r.follow_up.is_some()) {
        let next = item.follow_up.as_deref().unwrap_or_default();
        let repeated = response.results.iter().any(|other| other.entity_ref != item.entity_ref && other.title.len() >= 20 && next.contains(other.title.trim_end_matches('…')));
        assert!(!repeated, "a follow-up that is also a result is not repeated: {next}");
    }
}

#[test]
fn ts_20_02_generic_prompts_get_capsule_false() {
    let engine = TestEngine::start();
    // Facts of a project (as in benchmarks/memory-30); a plan that has the words of a generic prompt.
    engine.capture("user.message", "s-gen", Some("1"), "Plan: önce veritabanı şemasını oluştur, sonra API katmanını yaz.");
    engine.capture("user.message", "s-gen", Some("2"), "Kod incelemesini Elif yapıyor; tutarlar kuruş cinsinden i64.");
    engine.capture("user.message", "s-gen", Some("3"), "Karar: CI her commit için bütün testleri çalıştıracak.");
    engine.wait_idle();
    let prompts: Vec<String> = serde_json::from_str(GENERIC).unwrap();
    assert!(prompts.iter().any(|p| p == "bunların planını oluştur"));
    for prompt in &prompts {
        let response = engine.query(prompt, 1200);
        assert_eq!(response.capsule, Some(false), "{prompt}: {:?}", response.results.iter().map(|r| &r.title).collect::<Vec<_>>());
    }
    let fact = engine.query("Kod incelemesini kim yapıyor?", 1200);
    assert_ne!(fact.capsule, Some(false), "a short fact question keeps its capsule");
}

#[test]
fn ts_20_04_file_references_follow_the_code() {
    let (engine, repo, worktree) = ledger_engine(|_| {});
    assert!(file_refs(&engine, &worktree, "src/invoice.rs").iter().any(|n| n == "apply_late_fee"));
    std::fs::write(repo.join("src/invoice.rs"), "pub struct Invoice {\n    pub amount: i64,\n}\n").unwrap();
    commit_all(&repo, "drop the fee call");
    engine.capture("file.changed", "s-refs", Some("1"), r#"{"path":"src/invoice.rs"}"#);
    engine.wait_idle();
    assert!(!file_refs(&engine, &worktree, "src/invoice.rs").iter().any(|n| n == "apply_late_fee"), "the reference is deleted");
}

#[test]
fn ts_20_05_map_keeps_its_budget_and_follows_the_question() {
    let (engine, _repo, _) = ledger_engine(|_| {});
    let plain = ask(&engine, "overview", QueryMode::Map, 300).answer.expect("map");
    assert!(estimate_tokens(&plain) <= 300, "{plain}");
    let seeded = ask(&engine, "normalize the currency symbols", QueryMode::Map, 300).answer.expect("map");
    let position = |map: &str| map.find("src/currency.rs").unwrap_or(usize::MAX);
    assert!(position(&seeded) < position(&plain), "the seeded file ranks higher:\n{seeded}\n---\n{plain}");
    assert!(seeded.starts_with("src/currency.rs"), "{seeded}");
}

#[test]
fn ts_20_06_symbols_are_found_by_their_context() {
    let (engine, _repo, _) = ledger_engine(|_| {});
    let response = ask(&engine, "late payment charges overdue", QueryMode::Search, 1500);
    let symbol = response.results.iter().find(|r| r.kind == "symbol" && r.path.as_deref() == Some("src/fees.rs")).expect("a symbol of src/fees.rs");
    assert!(!symbol.title.contains("context") && !symbol.snippet.as_deref().unwrap_or_default().contains("context:"), "{symbol:?}");
}

#[test]
fn ts_20_07_a_multi_hop_question_gets_the_whole_chain() {
    let engine = TestEngine::start();
    let repo = fixture_repo(engine.dir.path(), true);
    engine.register(&repo);
    engine.capture("user.message", "s1", Some("1"), "Fix #89: weapon attachment moves during animation");
    let file = repo.join("src/animation/weapon_socket.rs");
    let text = std::fs::read_to_string(&file).unwrap().replace("self.bone = bone.to_string();", "self.bone = bone.trim().to_string();");
    std::fs::write(&file, text).unwrap();
    let sha = commit_all(&repo, "Fix weapon socket alignment\n\nZCIE-Task: #89");
    engine.capture("git.commit", "git", None, &serde_json::json!({ "sha": sha }).to_string());
    engine.wait_idle();
    let task = engine.query("#89", 2000).results.into_iter().find(|r| r.kind == "task").expect("task").entity_ref;
    let payload = serde_json::json!({ "name": "weapon-socket.integration", "status": "passed", "refs": [task] }).to_string();
    engine.capture("test.result", "s1", Some("2"), &payload);
    engine.wait_idle();
    let response = engine.query("#89 hangi dosyayı değiştirdi ve hangi test geçti?", 2000);
    let first: Vec<&zcie_types::ResultItem> = response.results.iter().take(6).collect();
    let has = |kind: &str| first.iter().any(|r| r.kind == kind);
    let test = first.iter().any(|r| r.title.contains("weapon-socket.integration") || r.snippet.as_deref().unwrap_or_default().contains("weapon-socket.integration"));
    assert!(has("task") && has("commit") && has("file") && test, "{:?}", response.results.iter().map(|r| (&r.kind, &r.title, r.score)).collect::<Vec<_>>());
}

#[test]
fn ts_20_08_compression_keeps_the_needed_sentences_in_order() {
    let filler = "Bu cümle konu dışı bir açıklama içeriyor ve soruyla ilgisi yok. ".repeat(14);
    let text = format!("{filler}Gecikme faizi oranı yüzde 2.25 olacak. {filler}Faiz değişikliği Mart sürümünde yapılacak. {filler}");
    assert!(text.chars().count() >= 2_000);
    let out = zcie_query::compress::compress(&text, "gecikme faizi oranı ne zaman değişecek, hangi sürümde?", zcie_query::compress::MAX_ITEM_CHARS);
    let rate = out.find("yüzde 2.25").expect("first sentence");
    let release = out.find("Mart sürümünde").expect("second sentence");
    assert!(rate < release && out.contains('…'), "{out}");
    assert!(out.chars().count() <= zcie_query::compress::MAX_ITEM_CHARS + 2, "{} characters", out.chars().count());
}

#[test]
fn ts_20_09_locate_gives_files_and_line_ranges() {
    let (engine, _repo, _) = ledger_engine(|_| {});
    let response = ask(&engine, "add a tax to the late fee", QueryMode::Locate, 1200);
    let answer = response.answer.clone().expect("answer");
    assert!(answer.starts_with("Files to change, best first: src/fees.rs"), "{answer}");
    let lines: Vec<String> = response.results.iter().filter(|r| r.kind == "symbol").map(|r| {
        let range = r.range.expect("range");
        format!("{}:{}-{} {} — {}", r.path.as_deref().unwrap_or_default(), range[0], range[1], r.title, r.snippet.as_deref().unwrap_or_default())
    }).collect();
    assert!(!lines.is_empty());
    assert!(estimate_tokens(&format!("{answer}\n{}", lines.join("\n"))) <= 300, "{lines:?}");
}

#[test]
fn ts_20_10_batch_expand_reads_ranges_and_related_lines() {
    let (engine, _repo, _) = ledger_engine(|_| {});
    let refs = ["src/fees.rs:1-5", "src/invoice.rs:1-3", "src/db.rs:1-4", "src/report.rs:1-250"].map(String::from).to_vec();
    let request = ExpandRequest { workspace: "atheron".into(), refs, level: ExpandLevel::Source, token_budget: Some(4000), expected_revision: None, with: Vec::new() };
    let response = engine.engine().expand(&admin(), request).unwrap();
    assert_eq!(response.items.len(), 4);
    assert!(response.items[..3].iter().all(|i| i.content.as_deref().is_some_and(|c| !c.is_empty())), "{:?}", response.items);
    let long = &response.items[3];
    assert_eq!(long.range, Some([1, 100]));
    assert_eq!(long.more.as_deref(), Some("src/report.rs:101-250"));
    let symbol = ask(&engine, "where is apply_late_fee defined", QueryMode::Auto, 600).results.into_iter().find(|r| r.kind == "symbol").expect("symbol").entity_ref;
    let with = vec!["callers".to_string(), "tests".to_string()];
    let request = ExpandRequest { workspace: "atheron".into(), refs: vec![symbol], level: ExpandLevel::Evidence, token_budget: Some(2000), expected_revision: None, with };
    let related = engine.engine().expand(&admin(), request).unwrap().items.remove(0).related;
    assert!(related.iter().any(|l| l.starts_with("caller src/invoice.rs:9 ")), "{related:?}");
    assert!(related.iter().any(|l| l.starts_with("test tests/fees_test.rs:")), "{related:?}");
}

fn long_message() -> String {
    let filler = "Bu cümle konu dışı bir açıklama içeriyor ve soruyla ilgisi yok. ".repeat(14);
    format!("{filler}Gecikme faizi oranı yüzde 2.25 olacak. {filler}Faiz değişikliği Mart sürümünde yapılacak. {filler}")
}

fn snippet_has_both(engine: &TestEngine) -> bool {
    engine.capture("user.message", "s-long", Some("1"), &long_message());
    engine.wait_idle();
    let response = engine.query("gecikme faizi oranı ne zaman değişecek, hangi sürümde?", 1500);
    let snippet = response.results.iter().filter(|r| matches!(r.kind.as_str(), "memory" | "event")).find_map(|r| r.snippet.clone()).unwrap_or_default();
    snippet.contains("yüzde 2.25") && snippet.contains("Mart sürümünde")
}

/// The score of `grace_days` (a neighbor of `apply_late_fee` in the graph: same file).
fn neighbor_score(graph_rank: bool) -> f32 {
    let (engine, _repo, _) = ledger_engine(|c| c.context.graph_rank = graph_rank);
    let response = ask(&engine, "how is apply_late_fee used", QueryMode::Search, 2000);
    response.results.iter().find(|r| r.title == "grace_days").map_or(0.0, |r| r.score)
}

#[test]
fn ts_20_11_each_context_setting_turns_its_behavior_off() {
    let rules_off = TestEngine::start_with(|c| c.context.capsule_rules = false);
    rules_off.capture("user.message", "s-gen", Some("1"), "Plan: önce testleri yazalım, sonra bunların hepsini oluştur.");
    rules_off.wait_idle();
    assert_eq!(rules_off.query("bunların planını oluştur", 1200).capsule, None, "capsule_rules = false");
    drop(rules_off);
    let (no_map, _, _) = ledger_engine(|c| c.context.repo_map_budget = 0);
    assert!(ask(&no_map, "overview", QueryMode::Map, 300).answer.is_none(), "repo_map_budget = 0");
    drop(no_map);
    let (on, off) = (neighbor_score(true), neighbor_score(false));
    assert!(on > off, "graph_rank = false: the graph neighbor gets no extra score ({on} > {off})");
    assert!(snippet_has_both(&TestEngine::start()), "compress = true keeps both sentences");
    assert!(!snippet_has_both(&TestEngine::start_with(|c| c.context.compress = false)), "compress = false: one window only");
}
