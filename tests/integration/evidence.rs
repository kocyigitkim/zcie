//! TS-21 — Evidence-first context (ADR-019, `docs/13-test-plan.md` section 5): the engine part.
//! The hook part (TS-21-07, TS-21-08) is in `context_hooks.rs`; TS-21-02 is the benchmark check
//! of `benchmarks/isolated`.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use zcie_tests::{TestEngine, admin, commit_all, ledger_repo};
use zcie_types::{QueryIntent, QueryMode, QueryRequest, QueryResponse};

const CODE_TASKS: &str = include_str!("../../benchmarks/context/code.json");
const LOCATE_TASKS: &str = include_str!("../../benchmarks/context/locate.json");

fn ask(engine: &TestEngine, question: &str, explain: bool) -> QueryResponse {
    let request = QueryRequest { workspace: "atheron".into(), question: question.into(), mode: QueryMode::Auto, token_budget: Some(1200), explain, ..QueryRequest::default() };
    engine.engine().query(&admin(), request).expect("query")
}

fn ledger() -> (TestEngine, std::path::PathBuf) {
    let engine = TestEngine::start();
    let repo = ledger_repo(engine.dir.path());
    engine.register(&repo);
    engine.wait_idle();
    (engine, repo)
}

#[test]
fn ts_21_01_the_trace_explains_each_candidate() {
    let (engine, _) = ledger();
    let plain = ask(&engine, "how is apply_late_fee used", false);
    assert!(plain.trace.is_none());
    assert!(!serde_json::to_string(&plain).unwrap().contains("\"trace\""));
    let traced = ask(&engine, "how is apply_late_fee used", true);
    let trace = traced.trace.expect("trace");
    assert!(trace.sources.iter().any(|s| s.source == "route" && s.count > 0), "{trace:?}");
    assert!(trace.stages.answer_ms > 0.0);
    let reasons: Vec<&str> = trace.candidates.iter().map(|c| c.reason.as_str()).collect();
    assert!(reasons.contains(&"kept") && reasons.iter().all(|r| ["kept", "echo", "duplicate", "below_cut"].contains(r)), "{reasons:?}");
    for item in &traced.results {
        assert!(trace.candidates.iter().any(|c| c.entity_ref == item.entity_ref && c.reason == "kept"), "{} has no kept line", item.entity_ref);
    }
}

#[test]
fn ts_21_03_benchmark_prompts_get_the_intent_of_their_task_class() {
    let expected = |id: &str| match id {
        "T2-locate-fn" | "T4-howto-delete" | "T7-explain-gc" => QueryIntent::Explain,
        "T14-trace-flow" => QueryIntent::Trace,
        "M1-name-ledger-db" | "M2-id-orbit-celery" | "M3-name-atlas-http" | "T11-memory-merge" | "T12-memory-rule" | "T13-memory-tr" | "L4-ledger-history" => QueryIntent::Memory,
        id if id.starts_with('E') => QueryIntent::Edit,
        id if id.starts_with('C') => QueryIntent::Locate,
        _ => QueryIntent::General,
    };
    let (engine, _) = ledger();
    let tasks: Vec<serde_json::Value> = [CODE_TASKS, LOCATE_TASKS].iter().flat_map(|t| serde_json::from_str::<Vec<serde_json::Value>>(t).unwrap()).collect();
    let mut wrong = Vec::new();
    for task in &tasks {
        let id = task["id"].as_str().unwrap();
        let intent = ask(&engine, task["prompt"].as_str().unwrap(), false).intent.expect("intent");
        if intent != expected(id) {
            wrong.push(format!("{id}: {intent:?} (expected {:?})", expected(id)));
        }
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
}

#[test]
fn ts_21_04_an_edit_prompt_gets_the_exact_slice_of_the_function() {
    let (engine, repo) = ledger();
    let response = ask(&engine, "Add a cap of 50 to apply_late_fee in src/fees.rs", false);
    assert_eq!(response.intent, Some(QueryIntent::Edit));
    let slice = response.evidence.iter().find(|e| e.path == "src/fees.rs" && e.text.contains("pub fn apply_late_fee")).expect("slice of apply_late_fee");
    let lines: Vec<String> = std::fs::read_to_string(repo.join("src/fees.rs")).unwrap().lines().map(str::to_string).collect();
    assert_eq!(slice.text, lines[slice.start_line as usize - 1..slice.end_line as usize].join("\n"));
    assert_eq!((slice.start_line, slice.end_line), (4, 9));
    assert!(slice.content_hash.starts_with("b3:") && !slice.requires_live_read && slice.kind == "source_slice");
}

#[test]
fn ts_21_05_a_changed_file_gives_the_slice_of_its_new_revision() {
    let (engine, repo) = ledger();
    let before = ask(&engine, "Add a cap of 50 to apply_late_fee", false).evidence;
    let text = std::fs::read_to_string(repo.join("src/fees.rs")).unwrap().replace("amount + amount *", "amount + 2 * amount *");
    std::fs::write(repo.join("src/fees.rs"), text).unwrap();
    commit_all(&repo, "double fee");
    engine.capture("file.changed", "s-ev", Some("1"), r#"{"path":"src/fees.rs"}"#);
    engine.wait_idle();
    let after = ask(&engine, "Add a cap of 50 to apply_late_fee", false).evidence;
    let hash = |items: &[zcie_types::EvidenceItem]| items.iter().find(|e| e.path == "src/fees.rs").map(|e| (e.content_hash.clone(), e.text.clone())).expect("slice");
    let ((old_hash, _), (new_hash, new_text)) = (hash(&before), hash(&after));
    assert_ne!(old_hash, new_hash);
    assert!(new_text.contains("2 * amount"), "the slice has the new revision: {new_text}");
    // A change that the watcher saw but that is not indexed yet (pending file job): the slice
    // is never marked current (AC-37).
    let overlay = &engine.engine().ingest.overlay;
    overlay.add("any-worktree", "src/fees.rs");
    let pending = ask(&engine, "Add a cap of 50 to apply_late_fee", false).evidence;
    assert!(pending.iter().filter(|e| e.path == "src/fees.rs").all(|e| e.requires_live_read) && pending.iter().any(|e| e.path == "src/fees.rs"), "{pending:?}");
    overlay.remove("any-worktree", "src/fees.rs");
    let settled = ask(&engine, "Add a cap of 50 to apply_late_fee", false).evidence;
    assert!(settled.iter().filter(|e| e.path == "src/fees.rs").all(|e| !e.requires_live_read));
}

#[test]
fn ts_21_06_an_explanation_gets_the_bodies_of_the_connected_functions() {
    let (engine, _) = ledger();
    let response = ask(&engine, "Explain how invoice_total and apply_late_fee work together", false);
    assert_eq!(response.intent, Some(QueryIntent::Explain));
    let texts: Vec<&str> = response.evidence.iter().map(|e| e.text.as_str()).collect();
    assert!(texts.iter().any(|t| t.contains("pub fn invoice_total") && t.contains("apply_late_fee(invoice.amount, invoice.days)")), "{texts:#?}");
    assert!(texts.iter().any(|t| t.contains("pub fn apply_late_fee") && t.contains("LATE_FEE_PERCENT as i64 / 100")), "{texts:#?}");
    assert!(texts.iter().all(|t| !t.contains('…')), "source is never compressed");
}

fn related(engine: &TestEngine, question: &str, path: &str) -> Vec<String> {
    let symbol = ask(engine, question, false).results.into_iter().find(|r| r.kind == "symbol" && r.path.as_deref() == Some(path)).expect("symbol").entity_ref;
    let with = vec!["callers".to_string()];
    let request = zcie_types::ExpandRequest { workspace: "atheron".into(), refs: vec![symbol], level: zcie_types::ExpandLevel::Evidence, token_budget: Some(2000), expected_revision: None, with };
    engine.engine().expand(&admin(), request).unwrap().items.remove(0).related
}

#[test]
fn ts_21_09_equal_names_stay_apart_and_calls_resolve_through_imports() {
    let engine = TestEngine::start();
    let root = engine.dir.path().join("twins");
    let files = [
        ("src/fees.rs", "pub fn apply_charge(x: i64) -> i64 {\n    x + 1\n}\n"),
        ("src/tax.rs", "pub fn apply_charge(x: i64) -> i64 {\n    x * 2\n}\n"),
        ("src/main.rs", "use crate::fees::apply_charge;\n\nfn run() -> i64 {\n    apply_charge(1)\n}\n"),
        ("src/other.rs", "fn other() -> i64 {\n    apply_charge(2)\n}\n"),
        ("src/qualified.rs", "fn q() -> i64 {\n    tax::apply_charge(3)\n}\n"),
    ];
    for (path, text) in files {
        let file = root.join(path);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, text).unwrap();
    }
    zcie_tests::git(&root, &["init", "-q", "-b", "main"]);
    commit_all(&root, "initial");
    engine.register(&root);
    engine.wait_idle();
    let fees = related(&engine, "where is apply_charge defined", "src/fees.rs");
    assert!(fees.iter().any(|l| l.starts_with("caller src/main.rs:4 ") && l.ends_with("[verified: import]")), "{fees:#?}");
    assert!(fees.iter().any(|l| l.starts_with("caller src/other.rs:2 ") && l.contains("[candidate: 2 definitions]")), "a bare name is a candidate: {fees:#?}");
    assert!(!fees.iter().any(|l| l.contains("src/qualified.rs")), "a qualified call of the other definition is not a caller: {fees:#?}");
    let tax = related(&engine, "where is apply_charge defined", "src/tax.rs");
    assert!(tax.iter().any(|l| l.starts_with("caller src/qualified.rs:2 ") && l.ends_with("[verified: qualified path]")), "{tax:#?}");
    assert!(!tax.iter().any(|l| l.contains("src/main.rs")), "the import of fees does not make a caller of tax: {tax:#?}");
}

#[test]
fn ts_21_10_an_assistant_statement_never_overrides_a_user_fact() {
    let engine = TestEngine::start_with(|c| c.capture.assistant_messages = true);
    engine.capture("user.message", "s-a", Some("1"), "Karar: tutarlar kuruş cinsinden i64 olarak tutulacak.");
    engine.capture("assistant.message", "s-a", Some("1"), "Karar: tutarlar f64 olarak tutulacak, bu daha basit.");
    engine.wait_idle();
    let response = engine.query("Tutarları hangi tipte tutuyoruz?", 1500);
    let memories: Vec<&zcie_types::ResultItem> = response.results.iter().filter(|r| r.kind == "memory").collect();
    let user = memories.iter().position(|r| r.title.contains("i64")).expect("the user fact");
    let user_item = memories[user];
    assert_eq!(user_item.status.as_deref(), Some("active"), "the user fact is not superseded");
    if let Some(assistant) = memories.iter().position(|r| r.title.contains("f64")) {
        assert!(user < assistant, "the user fact ranks first: {memories:#?}");
        assert_eq!(memories[assistant].evidence_status.as_deref(), Some("assistant_statement"));
    }
}

#[test]
fn ts_21_14_a_corrected_value_is_marked_superseded() {
    let engine = TestEngine::start();
    engine.capture("user.message", "s-c", Some("1"), "Rate limit dakikada 600 istek olacak.");
    engine.capture("user.message", "s-c", Some("2"), "Düzeltme: rate limit 600 değil, dakikada 900 istek.");
    engine.wait_idle();
    let response = engine.query("Rate limit dakikada kaç istek?", 1500);
    let old = response.results.iter().find(|r| r.kind == "memory" && r.title.starts_with("Rate limit dakikada 600"));
    assert!(old.is_none_or(|r| r.status.as_deref() == Some("superseded")), "{:#?}", response.results);
    assert!(response.results.iter().any(|r| r.title.contains("900") && r.status.as_deref() == Some("active")));
}

#[test]
fn ts_21_11_the_hook_key_cannot_use_admin_routes_and_ranges_stay_in_the_project() {
    let (engine, _) = ledger();
    let hook = zcie_tests::workspace_auth("claude-code", &["*"]);
    let forget = zcie_types::api::ForgetRequest { workspace: "atheron".into(), selector: Default::default(), reason: "test".into() };
    assert!(engine.engine().forget(&hook, forget).is_err(), "forget needs admin");
    assert!(engine.engine().delete_project(&hook, "atheron").is_err(), "project delete needs admin");
    assert!(engine.engine().rebuild_search(&hook).is_err(), "search rebuild needs admin");
    let outside = std::env::temp_dir().join("zcie-outside.rs");
    std::fs::write(&outside, "secret\n").unwrap();
    let refs = ["../../etc/passwd:1-3".to_string(), "/etc/passwd:1-3".to_string(), format!("{}:1-1", outside.display()), "src/../../ledger/src/fees.rs:1-2".to_string()];
    let request = zcie_types::ExpandRequest { workspace: "atheron".into(), refs: refs.to_vec(), level: zcie_types::ExpandLevel::Source, token_budget: Some(2000), expected_revision: None, with: Vec::new() };
    let items = engine.engine().expand(&hook, request).unwrap().items;
    for item in &items {
        assert_eq!(item.status, zcie_types::ExpandStatus::NotFound, "{item:?}");
        assert!(item.content.is_none(), "{item:?}");
    }
    let _ = std::fs::remove_file(outside);
}

#[test]
fn ts_21_12_a_forgotten_record_does_not_come_back_after_a_restart() {
    let mut engine = TestEngine::start();
    engine.capture("user.message", "s-f", Some("1"), "Karar: deploy anahtarı ZEBRA-4471 olarak ayarlandı.");
    let secret = engine.capture("user.message", "s-secret", Some("1"), "Karar: kasa şifresi KAPLAN-9902 olacak.");
    engine.capture("user.message", "s-secret", Some("2"), "Peki bunu kim değiştirebilir?");
    engine.wait_idle();
    let found = |engine: &TestEngine, question: &str| {
        let response = engine.query(question, 2000);
        serde_json::to_string(&response).unwrap().contains("KAPLAN-9902")
    };
    assert!(found(&engine, "kasa şifresi ne olacak?"), "the record is found before the forget");
    let forget = zcie_types::api::ForgetRequest {
        workspace: "atheron".into(),
        selector: zcie_types::api::ForgetSelector { session_id: Some("s-secret".into()), ..Default::default() },
        reason: "test".into(),
    };
    let receipt = engine.engine().forget(&admin(), forget).unwrap();
    assert!(receipt.events_removed >= 2 && receipt.records_retracted >= 1, "{receipt:?}");
    engine.wait_idle();
    engine.restart();
    engine.wait_idle();
    for question in ["kasa şifresi ne olacak?", "KAPLAN-9902", "vault password", "Bu projede verdiğimiz bütün kararları listele."] {
        let leak = engine.query(question, 2000);
        let leaked: Vec<String> = leak.results.iter().filter(|r| serde_json::to_string(r).unwrap().contains("KAPLAN-9902")).map(|r| format!("{} {} {:?} {:?}", r.entity_ref, r.kind, r.title, r.follow_up)).collect();
        assert!(!found(&engine, question), "{question} returns the forgotten record: {leaked:#?} answer {:?}", leak.answer);
    }
    let event_ref = format!("ev:{}", secret.event_id);
    let request = zcie_types::ExpandRequest { workspace: "atheron".into(), refs: vec![event_ref], level: zcie_types::ExpandLevel::Audit, token_budget: Some(2000), expected_revision: None, with: Vec::new() };
    let item = engine.engine().expand(&admin(), request).unwrap().items.remove(0);
    assert!(item.content.as_deref().is_none_or(|c| !c.contains("KAPLAN-9902")), "{item:?}");
    assert!(serde_json::to_string(&engine.query("deploy anahtarı nedir?", 2000)).unwrap().contains("ZEBRA-4471"), "an unrelated record stays");
}

#[test]
fn ts_21_15_a_file_changed_with_no_event_is_not_served_from_the_cache() {
    let (engine, repo) = ledger();
    let question = "Add a cap of 50 to apply_late_fee";
    let slice = |engine: &TestEngine| ask(engine, question, false).evidence.into_iter().find(|e| e.path == "src/fees.rs").map(|e| e.text).unwrap_or_default();
    assert!(!slice(&engine).contains("3 * amount"));
    let text = std::fs::read_to_string(repo.join("src/fees.rs")).unwrap().replace("amount + amount *", "amount + 3 * amount *");
    std::fs::write(repo.join("src/fees.rs"), text).unwrap();
    // No event: the file watcher sees the change (an editor outside Claude Code).
    let started = std::time::Instant::now();
    while started.elapsed() < std::time::Duration::from_secs(20) && !slice(&engine).contains("3 * amount") {
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
    assert!(slice(&engine).contains("3 * amount"), "the cached answer is not used after the change");
}
