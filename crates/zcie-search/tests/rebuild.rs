//! A rebuild ("delete all", then add) keeps a document whose earlier upsert was still uncommitted
//! (ADR-013): the delete of that earlier upsert must not remove the new copy.
#![allow(clippy::unwrap_used)]

use std::sync::Arc;

use zcie_search::{IndexName, SearchDoc, SearchIndex, SearchOp, SearchQuery, SearchWriter, flush};
use zcie_store::Store;

fn doc(entity_ref: &str, body: &str) -> SearchOp {
    let doc = SearchDoc {
        entity_ref: entity_ref.into(),
        kind: "memory".into(),
        tenant_id: "local".into(),
        workspace_id: "ws".into(),
        name: body.into(),
        body: body.into(),
        ..SearchDoc::default()
    };
    SearchOp::Upsert(IndexName::Conversations, Box::new(doc))
}

#[test]
fn delete_all_does_not_remove_documents_added_after_it() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(Store::open(&dir.path().join("db")).unwrap());
    let index = Arc::new(SearchIndex::open(&dir.path().join("search")).unwrap());
    let writer = SearchWriter::start(&index, store, "local", None).unwrap();
    let sender = writer.sender();
    for i in 0..10 {
        sender.send(doc(&format!("mem:filler{i}"), &format!("filler text number {i}"))).unwrap();
    }
    sender.send(doc("mem:kept", "the pendulum cache keeps nine entries")).unwrap();
    sender.send(SearchOp::DeleteAll).unwrap();
    sender.send(doc("mem:kept", "the pendulum cache keeps nine entries")).unwrap();
    flush(&sender).unwrap();
    let query = SearchQuery {
        tenant_id: "local".into(),
        workspace_id: "ws".into(),
        repository_id: None,
        text: "pendulum cache nine entries".into(),
        indexes: vec![IndexName::Conversations],
        limit: 10,
    };
    let hits = index.search(&query).unwrap();
    assert_eq!(hits.iter().map(|h| h.entity_ref.as_str()).collect::<Vec<_>>(), ["mem:kept"]);
    let fillers = index.search(&SearchQuery { text: "filler text".into(), ..query }).unwrap();
    assert!(fillers.is_empty(), "documents before the delete are gone");
    writer.stop();
}
