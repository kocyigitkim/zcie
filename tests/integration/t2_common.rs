//! Helpers shared by the TS-04, TS-05 and TS-06 test files.
#![allow(dead_code)]

use zcie_store::values::FileIdValue;
use zcie_store::{Cf, Store, layout};
use zcie_tests::TestEngine;
use zcie_types::{Edge, FileRecord, MemoryKind, MemoryRecord, Relation, SymbolRecord};

/// The tenant of the tests.
pub const T: &str = "local";

/// Counts keys with a prefix in a column family.
pub fn count(store: &Store, cf: Cf, prefix: &[u8]) -> usize {
    store.prefix_keys(cf, prefix, usize::MAX).unwrap().len()
}

/// Counts all entities of one kind.
pub fn count_entities(engine: &TestEngine, kind: &str) -> usize {
    count(&engine.engine().store, Cf::Entities, &layout::entity_prefix(T, kind))
}

/// Counts all forward edges.
pub fn count_edges(engine: &TestEngine) -> usize {
    count(&engine.engine().store, Cf::EdgesOut, &zcie_store::prefix(&[T]))
}

/// Reads the file record of a worktree path.
pub fn file_record(engine: &TestEngine, worktree_id: &str, path: &str) -> Option<FileRecord> {
    let store = &engine.engine().store;
    let id: FileIdValue = store.get(Cf::Lookup, &layout::path(T, worktree_id, path)).unwrap()?;
    store.get(Cf::Entities, &layout::entity(T, "file", &id.file_id)).unwrap()
}

/// Finds symbols by name in the test workspace.
pub fn symbols(engine: &TestEngine, name: &str) -> Vec<SymbolRecord> {
    let store = &engine.engine().store;
    let ws = workspace_id(engine);
    zcie_symbols::find_by_name(store, T, &ws, name, 20).unwrap()
}

/// Returns the ID of the test workspace.
pub fn workspace_id(engine: &TestEngine) -> String {
    let value: zcie_store::values::WorkspaceIdValue =
        engine.engine().store.get(Cf::Lookup, &layout::workspace_name(T, "atheron")).unwrap().unwrap();
    value.workspace_id
}

/// Returns the requirement record made from an event.
pub fn requirement_of(engine: &TestEngine, event_id: &str) -> MemoryRecord {
    zcie_memory::memories_of_event(&engine.engine().store, T, event_id)
        .unwrap()
        .into_iter()
        .find(|m| m.kind == MemoryKind::UserRequirement)
        .expect("requirement record")
}

/// Reads the outgoing edges of a ref with one relation.
pub fn edges_from(engine: &TestEngine, from: &str, relation: Relation) -> Vec<Edge> {
    zcie_graph::edges_from(&engine.engine().store, T, from, Some(relation)).unwrap()
}
