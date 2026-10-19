//! Corrections in plain messages (ADR-013): "Düzeltme: PostgreSQL 16 değil, 17 kullanacağız" or
//! "Correction: the limit is 900, not 600" supersedes the older statement that it corrects, so that
//! only the current value stays active.

use std::collections::HashSet;

use zcie_store::recovery::for_each_page;
use zcie_store::{Batch, Cf, Store, layout};
use zcie_types::{EdgeStatus, EventEnvelope, EvidenceKind, MemoryKind, MemoryRecord, MemoryStatus, Relation};

use crate::error::Result;
use crate::records::{evidence, put_memory};

/// Starts of a correction message (after letter folding, lowercase).
const MARKERS: [&str; 7] = ["duzeltme", "correction", "guncelleme", "update:", "aslinda", "yanlis soyledim", "i was wrong"];
/// Shortest word that counts in the overlap of two statements.
const MIN_WORD_CHARS: usize = 3;
/// Least shared words of a corrected statement without an old value.
const MIN_SHARED_WORDS: usize = 3;
/// Score of a statement that holds the old value ("16" in "16 değil").
const OLD_VALUE_SCORE: usize = 3;
/// Memory records that one page of the search reads.
const PAGE: usize = 1_000;

fn fold(text: &str) -> String {
    text.chars()
        .flat_map(char::to_lowercase)
        .map(|c| match c {
            'ç' => 'c',
            'ğ' => 'g',
            'ı' | 'i' => 'i',
            'ö' => 'o',
            'ş' => 's',
            'ü' => 'u',
            other => other,
        })
        .collect()
}

fn words(text: &str) -> Vec<String> {
    fold(text).split(|c: char| !c.is_alphanumeric()).filter(|w| w.chars().count() >= MIN_WORD_CHARS || w.chars().all(|c| c.is_ascii_digit()) && !w.is_empty()).map(str::to_string).collect()
}

/// True for a message that corrects an earlier statement.
pub fn is_correction(text: &str) -> bool {
    let folded = fold(text.trim());
    MARKERS.iter().any(|m| folded.starts_with(m)) || folded.contains(" degil,") || folded.contains(", not ")
}

/// Returns the old values of a correction: the word before "değil" ("16 değil") or after "not".
fn old_values(text: &str) -> Vec<String> {
    let all = words(text);
    let mut out = Vec::new();
    for (i, word) in all.iter().enumerate() {
        if word == "degil" && i > 0 {
            out.push(all[i - 1].clone());
        }
        if word == "not" && i + 1 < all.len() {
            out.push(all[i + 1].clone());
        }
    }
    out
}

/// Finds the statement that `new` corrects and marks it `Superseded`. Returns the changed refs.
pub fn apply_correction(store: &Store, batch: &mut Batch, event: &EventEnvelope, new: &MemoryRecord) -> Result<Vec<String>> {
    if !is_correction(&new.text) {
        return Ok(Vec::new());
    }
    let Some(mut old) = corrected_record(store, event, new)? else { return Ok(Vec::new()) };
    old.status = MemoryStatus::Superseded;
    old.superseded_by = Some(new.memory_id.clone());
    old.generation += 1;
    put_memory(batch, &old)?;
    let note = evidence(EvidenceKind::UserConfirmation, &[&event.event_id], Some("correction in a message"));
    let edge = zcie_graph::new_edge(&new.scope, &format!("mem:{}", new.memory_id), Relation::Supersedes, &format!("mem:{}", old.memory_id), EdgeStatus::Verified, note);
    zcie_graph::put_edge(store, batch, edge)?;
    Ok(vec![format!("mem:{}", old.memory_id)])
}

/// Returns the active earlier statement of the same project that shares the most words with the
/// correction; a statement with an old value scores higher. `None` if no statement is close enough.
fn corrected_record(store: &Store, event: &EventEnvelope, new: &MemoryRecord) -> Result<Option<MemoryRecord>> {
    let new_words: HashSet<String> = words(&new.text).into_iter().collect();
    let old_values = old_values(&new.text);
    let mut best: Option<(usize, MemoryRecord)> = None;
    let prefix = layout::entity_prefix(&event.scope.tenant_id, "mem");
    for_each_page::<MemoryRecord>(store, Cf::Entities, &prefix, PAGE, &mut |page| {
        for (_, record) in page {
            let candidate = record.scope.workspace_id == new.scope.workspace_id
                && record.memory_id != new.memory_id
                && !record.source_event_ids.contains(&event.event_id)
                && record.status == MemoryStatus::Active
                && matches!(record.kind, MemoryKind::UserRequirement | MemoryKind::DesignDecision | MemoryKind::Preference | MemoryKind::Constraint)
                && record.valid_from <= new.valid_from
                && !record.text.contains('?');
            if !candidate {
                continue;
            }
            let record_words: HashSet<String> = words(&record.text).into_iter().collect();
            let shared = record_words.intersection(&new_words).count();
            let holds_old = old_values.iter().any(|v| record_words.contains(v));
            let score = shared + if holds_old { OLD_VALUE_SCORE } else { 0 };
            let close = holds_old && shared >= 2 || shared >= MIN_SHARED_WORDS;
            if close && best.as_ref().is_none_or(|(s, b)| score > *s || score == *s && record.valid_from > b.valid_from) {
                best = Some((score, record));
            }
        }
        Ok(())
    })?;
    Ok(best.map(|(_, record)| record))
}
