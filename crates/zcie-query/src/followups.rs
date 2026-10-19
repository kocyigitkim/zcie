//! Follow-up messages of the top user messages (ADR-013). A user often refines an idea in the
//! next messages ("peki şunu istiyorum: bunu otomatik yapsın ama maliyet artmasın"). The top
//! request records of a response get the next short user messages of their session.

use std::collections::HashSet;

use zcie_store::{Batch, ObjectStore, Store};
use zcie_types::{ResultItem, truncate_chars};

/// Top request records that get follow-up messages.
const FOLLOW_UP_ITEMS: usize = 3;
/// Next user messages per record.
const FOLLOW_UPS: usize = 2;
/// Longest next message that counts as a follow-up (a longer one starts a new topic).
const MAX_FOLLOW_UP_SOURCE_CHARS: usize = 400;
/// Characters of one follow-up in the response.
const FOLLOW_UP_CHARS: usize = 150;

/// Sets `follow_up` on the first `FOLLOW_UP_ITEMS` request records of `items` that are chat
/// messages (a pasted document is not refined by the next messages). Errors leave an item
/// unchanged: follow-ups are extra context, not evidence.
pub fn add_follow_ups(store: &Store, objects: &ObjectStore, tenant: &str, items: &mut [ResultItem]) {
    // A follow-up that a result already shows, or that another result got, is not shown again (ADR-017).
    let mut shown: HashSet<String> = items.iter().flat_map(|i| [key(&i.title), key(i.snippet.as_deref().unwrap_or_default())]).collect();
    let chat = |i: &ResultItem| i.snippet.as_deref().unwrap_or(&i.title).chars().count() <= MAX_FOLLOW_UP_SOURCE_CHARS && !i.title.contains("<pasted_content");
    let requests = items.iter_mut().filter(|i| i.kind == "memory" && i.evidence_status.as_deref().is_some_and(|e| e.starts_with("user_requirement")) && chat(i));
    for item in requests.take(FOLLOW_UP_ITEMS) {
        let next: Vec<String> = follow_ups(store, objects, tenant, &item.entity_ref).unwrap_or_default().into_iter().filter(|t| shown.insert(key(t))).collect();
        if !next.is_empty() {
            item.follow_up = Some(next.join(" | "));
        }
    }
}

/// The comparison key of a text: its first 80 characters, lowercase, white space joined.
fn key(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase().chars().take(80).collect()
}

fn follow_ups(store: &Store, objects: &ObjectStore, tenant: &str, entity_ref: &str) -> Option<Vec<String>> {
    let id = entity_ref.strip_prefix("mem:")?;
    let record = zcie_memory::get_memory(store, &Batch::new(), tenant, id).ok()??;
    let source = record.source_event_ids.first()?;
    let event = zcie_store::events::get_event_by_id(store, tenant, source.as_str()).ok()??;
    let next = zcie_memory::session_user_messages(store, objects, &event, true, FOLLOW_UPS).ok()?;
    Some(next.into_iter().take_while(|t| t.chars().count() <= MAX_FOLLOW_UP_SOURCE_CHARS).map(|t| truncate_chars(&t, FOLLOW_UP_CHARS)).collect())
}
