//! Token firewall (`docs/09-query-and-context.md` section 9).

use zcie_types::{QueryResponse, ResultItem, truncate_chars};

use crate::budget::json_tokens;
use crate::refs::make_cursor;

/// Maximum title characters of a ref-only item.
const REF_ONLY_TITLE: usize = 60;

/// A response that obeys the budget, and the results that did not fit.
#[derive(Debug, Clone)]
pub struct Capsule {
    /// The response.
    pub response: QueryResponse,
    /// Results that the budget left out, in rank order.
    pub remaining: Vec<ResultItem>,
}

/// Adds complete results in rank order until the budget is full. Never cuts a result.
///
/// `base` is the response envelope with no results. `offset` is the position of `items[0]`
/// in the full result list (for cursors).
pub fn build_capsule(mut base: QueryResponse, items: Vec<ResultItem>, budget: u32, offset: usize) -> Capsule {
    base.results.clear();
    base.token_budget = budget;
    base.truncated = false;
    base.cursor = None;
    let mut total = json_tokens(&base);
    let mut iter = items.into_iter();
    let mut remaining = Vec::new();
    for item in iter.by_ref() {
        let cost = json_tokens(&item) + 1;
        if total + cost <= budget {
            total += cost;
            base.results.push(item);
            continue;
        }
        if base.results.is_empty() {
            let reduced = ref_only(item);
            total += json_tokens(&reduced) + 1;
            base.results.push(reduced);
        } else {
            remaining.push(item);
        }
        break;
    }
    remaining.extend(iter);
    if !remaining.is_empty() {
        base.truncated = true;
        base.cursor = Some(make_cursor(&base.query_id, offset + base.results.len()));
    }
    base.token_estimate = total.max(json_tokens(&base));
    Capsule { response: base, remaining }
}

/// Returns the item with no snippet and a short title.
pub fn ref_only(mut item: ResultItem) -> ResultItem {
    item.snippet = None;
    item.title = truncate_chars(&item.title, REF_ONLY_TITLE);
    item
}
