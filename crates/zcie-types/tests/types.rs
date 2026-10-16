//! TS-01: types (docs/13-test-plan.md section 5).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use zcie_types::*;

#[test]
fn ids_with_correct_prefix_parse() {
    assert!(EventId::parse(EventId::new().as_str()).is_ok());
    assert!(FileId::parse(FileId::new().as_str()).is_ok());
    assert!(SymbolId::parse(SymbolId::new().as_str()).is_ok());
    assert!(MemoryId::parse(MemoryId::new().as_str()).is_ok());
    assert!(TaskId::parse(TaskId::new().as_str()).is_ok());
    assert!(EdgeId::parse(EdgeId::new().as_str()).is_ok());
    assert!(JobId::parse(JobId::new().as_str()).is_ok());
    assert_eq!(CommitId::parse("commit_abc123").unwrap().sha(), "abc123");
}

#[test]
fn ids_with_wrong_prefix_fail() {
    assert!(EventId::parse("sym_01ABC").is_err());
    assert!(TaskId::parse("task_").is_err());
    assert!(CommitId::parse("abc123").is_err());
}

#[test]
fn refs_parse_and_format() {
    let r = Ref::parse("sym:sym_01ABC").unwrap();
    assert_eq!((r.kind.as_str(), r.id.as_str()), ("sym", "sym_01ABC"));
    assert_eq!(r.to_string(), "sym:sym_01ABC");
    assert!(Ref::parse("bad:x").is_err());
    assert!(Ref::parse("nocolon").is_err());
}

fn scope() -> Scope {
    Scope::workspace("user-001", "ws_1")
}

#[test]
fn records_round_trip_through_json() {
    let edge = Edge {
        v: 1,
        edge_id: EdgeId::new(),
        scope: scope(),
        from: "task:task_1".into(),
        relation: Relation::Implements,
        to: "commit:commit_abc".into(),
        status: EdgeStatus::Verified,
        evidence: Evidence { kind: EvidenceKind::CommitTrailer, source_event_ids: vec![EventId::new()], revision: None, note: None },
        confidence: 1.0,
        created_utc: now_utc(),
        generation: 1,
    };
    let back: Edge = serde_json::from_str(&serde_json::to_string(&edge).unwrap()).unwrap();
    assert_eq!(back, edge);
    let job = JobRecord::new(JobKind::IndexFile, scope(), JobPriority::Background, 3, "wt_1:a.rs".into());
    let back: JobRecord = serde_json::from_value(serde_json::to_value(&job).unwrap()).unwrap();
    assert_eq!(back, job);
    let task = TaskRecord {
        v: 1, task_id: TaskId::new(), scope: scope(), title: "t".into(), state: TaskState::Open,
        external_ref: Some("#89".into()), created_from: EventId::new(), created_utc: now_utc(), history: vec![], generation: 1,
    };
    let back: TaskRecord = serde_json::from_value(serde_json::to_value(&task).unwrap()).unwrap();
    assert_eq!(back, task);
}

#[test]
fn relation_serializes_screaming_snake_case() {
    assert_eq!(serde_json::to_string(&Relation::ChangedInCommit).unwrap(), "\"CHANGED_IN_COMMIT\"");
    for r in Relation::ALL {
        assert_eq!(serde_json::to_string(&r).unwrap(), format!("\"{}\"", r.name()));
        assert_eq!(Relation::from_name(r.name()), Some(r));
    }
}

#[test]
fn event_type_serializes_dotted_name() {
    assert_eq!(serde_json::to_string(&EventType::UserMessage).unwrap(), "\"user.message\"");
}

#[test]
fn evidence_max_status_agrees_with_data_model_table() {
    use EvidenceKind::*;
    for k in [ClientTrace, UserConfirmation, CommitTrailer, AstResolved, TestOutcome, GitHistory, Derivation] {
        assert_eq!(k.max_status(), EdgeStatus::Verified, "{k:?}");
    }
    assert_eq!(TextMention.max_status(), EdgeStatus::Inferred);
    assert_eq!(AstApproximate.max_status(), EdgeStatus::Inferred);
    assert_eq!(Temporal.max_status(), EdgeStatus::Candidate);
}

#[test]
fn scope_contains_narrower_but_not_wider_or_other_workspace() {
    let wide = scope();
    let narrow = Scope { repository_id: Some("repo_1".into()), worktree_id: Some("wt_1".into()), ..scope() };
    assert!(wide.contains(&narrow));
    assert!(!narrow.contains(&wide));
    assert!(!wide.contains(&Scope::workspace("user-001", "ws_2")));
    let other_tenant = Scope { tenant_id: "other".into(), ..scope() };
    assert!(!wide.contains(&other_tenant));
}
