//! "List all decisions and rules" questions (ADR-013): the answer is every current statement of
//! the project (decisions, rules, what the user said), not the best few matches.

use zcie_store::recovery::for_each_page;
use zcie_store::{Cf, layout};
use zcie_types::{MemoryKind, MemoryRecord, MemoryStatus, QueryClass, QueryResponse, Watermarks};

use crate::error::Result;
use crate::planner::{QueryContext, QueryEngine};
use crate::routes::Candidate;

/// Phrases that ask for all decisions or rules (lowercase).
const LIST_PHRASES: [&str; 14] = [
    "bütün karar", "tüm karar", "kararları listele", "kararları ve kural", "bütün kural", "tüm kural", "kuralları listele",
    "all decisions", "all the decisions", "list the decisions", "list all decisions", "all rules", "all the rules", "list the rules",
];
/// Token budget of a list answer (the user asked for all items).
const LIST_BUDGET: u32 = 2_000;
/// Most statements in a list answer (the newest).
const LIST_MAX: usize = 40;
/// Memory records that one page of the scan reads.
const PAGE: usize = 1_000;
/// Note of a list answer; the context hooks show all its items.
pub const LIST_NOTE: &str = "all decisions and rules of the project";

/// True for a question that asks for all decisions or rules.
pub fn asks_for_all_decisions(question: &str) -> bool {
    let lower = question.to_lowercase();
    LIST_PHRASES.iter().any(|p| lower.contains(p))
}

fn is_statement(record: &MemoryRecord, workspace: &str) -> bool {
    record.scope.workspace_id == workspace
        && record.status == MemoryStatus::Active
        && matches!(record.kind, MemoryKind::DesignDecision | MemoryKind::Preference | MemoryKind::Constraint | MemoryKind::Correction | MemoryKind::UserRequirement)
        && !crate::relevance::looks_like_question(&record.text)
        && !asks_for_all_decisions(&record.text)
}

impl QueryEngine {
    /// Answers a "list all decisions" question with the current statements of the project, oldest
    /// first, titles only. Returns `None` for other questions.
    pub(crate) fn list_all(&self, ctx: &QueryContext, (class, question): (QueryClass, &str), watermarks: Watermarks, budget: u32) -> Result<Option<QueryResponse>> {
        if !asks_for_all_decisions(question) {
            return Ok(None);
        }
        let mut records: Vec<MemoryRecord> = Vec::new();
        let prefix = layout::entity_prefix(&self.config.tenant, "mem");
        for_each_page::<MemoryRecord>(&self.store, Cf::Entities, &prefix, PAGE, &mut |page| {
            records.extend(page.into_iter().map(|(_, r)| r).filter(|r| is_statement(r, &ctx.scope.workspace_id)));
            Ok(())
        })?;
        records.sort_by(|a, b| a.valid_from.cmp(&b.valid_from));
        let newest = records.len().saturating_sub(LIST_MAX);
        let mut candidates: Vec<Candidate> = Vec::new();
        for record in &records[newest..] {
            if let Some(mut candidate) = self.candidate(ctx, &format!("mem:{}", record.memory_id), 1.0, None)? {
                candidate.loaded.item.snippet = None;
                candidates.push(candidate);
            }
        }
        let note = Some(format!("{LIST_NOTE}: {}", candidates.len()));
        let mut response = self.compose(ctx, (class, note, question), candidates, watermarks, 0, budget.max(LIST_BUDGET), LIST_MAX);
        // A list of all decisions answers the prompt: it is never a "no capsule" response.
        response.capsule = None;
        Ok(Some(response))
    }
}
