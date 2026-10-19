//! Source-slice evidence (ADR-019, D-069). An edit, explain or trace prompt gets the exact source of
//! its best symbols: the whole symbol when it is short, else the signature and the lines with the
//! question terms. The text comes from the stored copy of the indexed revision (a query never reads
//! project folders, ADR-016), so the text, the lines and the hash agree. A slice needs a live read
//! when a change of its file is pending or its folder is not watched.

use std::collections::{BTreeSet, HashSet};

use zcie_store::{Cf, layout};
use zcie_types::{EvidenceItem, FileRecord, QueryIntent, QueryRequest, QueryResponse, ResultItem, SymbolRecord, WorktreeRecord, estimate_tokens};

use crate::error::Result;
use crate::planner::{QueryContext, QueryEngine};

/// Most symbols that give slices for an edit prompt.
const EDIT_SYMBOLS: usize = 3;
/// Most symbols that give slices for an explain or trace prompt.
const EXPLAIN_SYMBOLS: usize = 5;
/// A symbol with this many lines or fewer is given whole.
const WHOLE_SYMBOL_LINES: u32 = 60;
/// Lines of a long symbol that are always given (the signature).
const SIGNATURE_LINES: u32 = 3;
/// Lines given before and after a line with a question term.
const AROUND_LINES: u32 = 3;
/// Smallest token budget of the evidence; else half of the request budget.
const MIN_EVIDENCE_BUDGET: u32 = 300;
/// Fixed token cost of one item (fields and the capsule line).
const ITEM_OVERHEAD_TOKENS: u32 = 25;

/// True for a test symbol (its file or its name is a test).
fn is_test(item: &ResultItem) -> bool {
    let path = item.path.as_deref().unwrap_or_default().to_lowercase();
    let name = item.title.to_lowercase();
    let file = path.rsplit('/').next().unwrap_or_default();
    let last = name.rsplit("::").next().unwrap_or_default();
    path.contains("/tests/")
        || path.starts_with("tests/")
        || ["tests.rs", "test.rs", "conftest.py"].contains(&file)
        || file.starts_with("test_")
        || file.contains("_test.")
        || file.contains(".test.")
        || file.contains(".spec.")
        || name.contains("tests::")
        || last.starts_with("test_")
}

/// Returns the line ranges (1-based, inclusive) to give of a symbol span.
fn select_ranges(lines: &[&str], [start, end]: [u32; 2], terms: &HashSet<String>) -> Vec<[u32; 2]> {
    if end < start {
        return Vec::new();
    }
    if end - start < WHOLE_SYMBOL_LINES {
        return vec![[start, end]];
    }
    let mut keep: BTreeSet<u32> = (start..=(start + SIGNATURE_LINES - 1).min(end)).collect();
    for number in start..=end {
        let line = lines.get(number as usize - 1).copied().unwrap_or_default();
        let found = crate::relevance::stems(line).iter().any(|s| terms.iter().any(|t| crate::synonyms::same_stem(s, t)));
        if found && keep.len() < WHOLE_SYMBOL_LINES as usize {
            keep.extend(number.saturating_sub(AROUND_LINES).max(start)..=(number + AROUND_LINES).min(end));
        }
    }
    let mut ranges: Vec<[u32; 2]> = Vec::new();
    for number in keep {
        match ranges.last_mut() {
            Some(last) if last[1] + 1 == number => last[1] = number,
            _ => ranges.push([number, number]),
        }
    }
    ranges
}

impl QueryEngine {
    /// Sets the intent of a response and, for an edit, explain or trace prompt, its evidence.
    pub(crate) fn add_evidence(&self, ctx: &QueryContext, request: &QueryRequest, response: &mut QueryResponse) -> Result<()> {
        let intent = crate::intent::select_intent(&request.question, request.mode, response.class);
        response.intent = Some(intent);
        let symbols = match intent {
            QueryIntent::Edit => EDIT_SYMBOLS,
            QueryIntent::Explain | QueryIntent::Trace => EXPLAIN_SYMBOLS,
            _ => return Ok(()),
        };
        let budget = (response.token_budget / 2).max(MIN_EVIDENCE_BUDGET);
        let wants_tests = request.question.to_lowercase().contains("test");
        let terms = crate::relevance::stems(&request.question);
        let mut used = 0;
        let best: Vec<ResultItem> = response.results.iter().filter(|r| r.kind == "symbol" && (wants_tests || !is_test(r))).take(symbols).cloned().collect();
        for item in best {
            for slice in self.slices(ctx, &item, &terms)? {
                let cost = estimate_tokens(&slice.text) + ITEM_OVERHEAD_TOKENS;
                if used + cost > budget {
                    response.token_estimate += used;
                    return Ok(());
                }
                used += cost;
                response.evidence.push(slice);
            }
        }
        response.token_estimate += used;
        Ok(())
    }

    /// Returns the slices of one symbol result, or nothing when its stored revision is gone.
    fn slices(&self, ctx: &QueryContext, item: &ResultItem, terms: &HashSet<String>) -> Result<Vec<EvidenceItem>> {
        let t = &self.config.tenant;
        let Some(id) = item.entity_ref.strip_prefix("sym:") else { return Ok(Vec::new()) };
        let Some(symbol) = self.store.get::<SymbolRecord>(Cf::Entities, &layout::entity(t, "sym", id))? else { return Ok(Vec::new()) };
        let Some(file) = self.store.get::<FileRecord>(Cf::Entities, &layout::entity(t, "file", symbol.file_id.as_str()))? else { return Ok(Vec::new()) };
        // The symbol must belong to the stored revision, else its lines do not match the text.
        if file.deleted || file.content_hash != symbol.file_hash {
            return Ok(Vec::new());
        }
        let Ok(text) = self.objects.get_text(&file.content_hash) else { return Ok(Vec::new()) };
        let lines: Vec<&str> = text.lines().collect();
        let live = self.needs_live_read(&symbol);
        let ranges = select_ranges(&lines, symbol.span, terms);
        let whole = (ranges.len() > 1 || ranges.first().is_some_and(|r| *r != symbol.span)).then(|| format!("{}:{}-{}", symbol.path, symbol.span[0], symbol.span[1]));
        Ok(ranges
            .into_iter()
            .filter(|[a, b]| *a >= 1 && (*b as usize) <= lines.len())
            .map(|[a, b]| EvidenceItem {
                entity_ref: item.entity_ref.clone(),
                project: ctx.scope.workspace_id.clone(),
                path: symbol.path.clone(),
                start_line: a,
                end_line: b,
                content_hash: file.content_hash.clone(),
                kind: "source_slice".into(),
                provenance: "indexed_file".into(),
                status: item.status.clone().unwrap_or_else(|| "candidate".into()),
                text: lines[a as usize - 1..b as usize].join("\n"),
                requires_live_read: live,
                more: whole.clone(),
            })
            .collect())
    }

    /// True when the file of a symbol can differ from the indexed revision: a change of the path is
    /// pending, or the folder is not watched (ADR-015).
    fn needs_live_read(&self, symbol: &SymbolRecord) -> bool {
        if self.ingest.overlay.contains_path(&symbol.path) {
            return true;
        }
        let Some(worktree_id) = symbol.scope.worktree_id.as_ref() else { return true };
        let key = layout::registry(&self.config.tenant, "worktree", worktree_id);
        !self.store.get::<WorktreeRecord>(Cf::Registry, &key).ok().flatten().is_some_and(|w| w.indexed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_short_symbol_is_whole_and_a_long_one_has_the_signature_and_the_term_lines() {
        let terms: HashSet<String> = ["purge"].iter().map(|s| s.to_string()).collect();
        let lines: Vec<String> = (1..=200).map(|i| if i == 150 { "    purge_files(x);".into() } else { format!("    step_{i}();") }).collect();
        let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
        assert_eq!(select_ranges(&refs, [10, 40], &terms), vec![[10, 40]]);
        assert_eq!(select_ranges(&refs, [100, 199], &terms), vec![[100, 102], [147, 153]]);
    }
}
