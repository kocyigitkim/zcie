//! Timeline of an entity (`docs/08-memory-and-graph.md` section 7).

use std::collections::BTreeMap;

use zcie_graph::{Direction, TraverseOptions, neighbors};
use zcie_store::{Batch, ObjectStore, Store};
use zcie_types::api::TimelineItem;
use zcie_types::{EventEnvelope, Ref, truncate_chars};

use crate::error::Result;
use crate::records::get_memory;
use crate::tasks::get_task;

/// Maximum timeline items.
pub const MAX_TIMELINE_ITEMS: usize = 200;
/// Maximum characters of a timeline summary.
const SUMMARY_CHARS: usize = 160;
/// Timeline traversal depth.
const TIMELINE_DEPTH: usize = 2;

/// Returns the events linked to `start_ref` and its neighbors, in sequence order.
pub fn timeline(store: &Store, objects: &ObjectStore, tenant: &str, workspace_id: &str, start_ref: &str, max_fanout: usize) -> Result<Vec<TimelineItem>> {
    let options = TraverseOptions {
        relations: Vec::new(),
        direction: Direction::Both,
        max_depth: TIMELINE_DEPTH,
        max_fanout,
        workspace_id: workspace_id.to_string(),
    };
    let mut events: BTreeMap<String, Vec<String>> = BTreeMap::new();
    collect_entity_events(store, tenant, start_ref, &mut events)?;
    for item in neighbors(store, tenant, start_ref, &options)? {
        collect_entity_events(store, tenant, &item.entity_ref, &mut events)?;
        for id in &item.edge.evidence.source_event_ids {
            add_event(&mut events, id.as_str(), &item.entity_ref);
        }
    }
    let mut envelopes = Vec::new();
    for (event_id, refs) in events {
        if let Some(event) = zcie_store::events::get_event_by_id(store, tenant, &event_id)?
            && event.scope.workspace_id == workspace_id
            && !event.forgotten
        {
            envelopes.push((event, refs));
        }
    }
    envelopes.sort_by_key(|(event, _)| event.seq);
    envelopes.truncate(MAX_TIMELINE_ITEMS);
    Ok(envelopes.into_iter().map(|(event, refs)| to_item(objects, &event, refs)).collect())
}

fn add_event(events: &mut BTreeMap<String, Vec<String>>, event_id: &str, entity_ref: &str) {
    let refs = events.entry(event_id.to_string()).or_default();
    if !entity_ref.starts_with("ev:") && !refs.iter().any(|r| r == entity_ref) {
        refs.push(entity_ref.to_string());
    }
}

fn collect_entity_events(store: &Store, tenant: &str, entity_ref: &str, events: &mut BTreeMap<String, Vec<String>>) -> Result<()> {
    let Ok(parsed) = Ref::parse(entity_ref) else { return Ok(()) };
    let batch = Batch::new();
    match parsed.kind.as_str() {
        "ev" => add_event(events, &parsed.id, entity_ref),
        "mem" => {
            if let Some(record) = get_memory(store, &batch, tenant, &parsed.id)? {
                for id in &record.source_event_ids {
                    add_event(events, id.as_str(), entity_ref);
                }
            }
        }
        "task" => {
            if let Some(task) = get_task(store, &batch, tenant, &parsed.id)? {
                add_event(events, task.created_from.as_str(), entity_ref);
                for change in &task.history {
                    add_event(events, change.event_id.as_str(), entity_ref);
                }
            }
        }
        _ => {}
    }
    Ok(())
}

fn to_item(objects: &ObjectStore, event: &EventEnvelope, refs: Vec<String>) -> TimelineItem {
    let text = zcie_store::events::payload_text(objects, event).ok().flatten().unwrap_or_default();
    TimelineItem {
        seq: event.seq,
        event_id: event.event_id.0.clone(),
        event_type: event.event_type.name().to_string(),
        timestamp_utc: event.timestamp_utc.clone(),
        summary: truncate_chars(&text, SUMMARY_CHARS),
        refs,
    }
}
