//! Pages of a response: the capsule of one page and the cursor of the rest (moved out of
//! `planner.rs` to keep the file short).

use zcie_context::{build_capsule, parse_cursor};
use zcie_types::{QueryResponse, ResultItem};

use crate::error::{Error, Result};
use crate::planner::{QueryContext, QueryEngine};

impl QueryEngine {
    pub(crate) fn paginate(&self, ctx: &QueryContext, base: QueryResponse, mut items: Vec<ResultItem>, budget: u32, max_results: usize, offset: usize) -> QueryResponse {
        let rest = if items.len() > max_results { items.split_off(max_results) } else { Vec::new() };
        let capsule = build_capsule(base, items, budget, offset);
        let mut response = capsule.response;
        let mut remaining = capsule.remaining;
        remaining.extend(rest);
        if !remaining.is_empty() {
            let next_offset = offset + response.results.len();
            response.truncated = true;
            response.cursor = Some(zcie_context::make_cursor(&response.query_id, next_offset));
            let mut page_base = response.clone();
            page_base.results.clear();
            self.cache.put_page(&response.query_id, page_base, remaining, &ctx.scope.workspace_id, &ctx.auth.principal_id);
        }
        response
    }

    pub(crate) fn next_page(&self, ctx: &QueryContext, cursor: &str, budget: u32) -> Result<QueryResponse> {
        let (query_id, offset) = parse_cursor(cursor).ok_or_else(|| Error::Invalid("bad cursor".into()))?;
        let page = self.cache.get_page(&query_id).ok_or_else(|| Error::NotFound("cursor expired".into()))?;
        if page.principal_id != ctx.auth.principal_id || page.workspace_id != ctx.scope.workspace_id {
            return Err(Error::Forbidden);
        }
        let max_results = self.config.max_results;
        Ok(self.paginate(ctx, page.base, page.remaining, budget, max_results, offset))
    }
}
