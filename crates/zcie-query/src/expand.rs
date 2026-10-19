//! Expand refs into evidence, source or audit content (`docs/09-query-and-context.md` section 6.1).

use std::path::Path;

use zcie_context::{clamp_budget, json_tokens};
use zcie_store::{Batch, Cf, layout};
use zcie_types::{
    CommitRecord, ExpandItem, ExpandLevel, ExpandRequest, ExpandResponse, ExpandStatus, FileRecord, Freshness, Ref,
    SymbolRecord, WorktreeRecord, truncate_chars,
};

use crate::error::{Error, Result};
use crate::planner::{QueryContext, QueryEngine};

/// Maximum refs in one expand request.
pub const MAX_EXPAND_REFS: usize = 20;
/// Default budget of an expand request.
const DEFAULT_EXPAND_BUDGET: u32 = 1500;
/// Lines of a symbol or file at the evidence level.
const EVIDENCE_LINES: usize = 12;
/// Characters of an event payload at the source level.
const EVENT_SOURCE_CHARS: usize = 4000;
/// Largest file that an expansion reads from disk.
const MAX_READ_BYTES: u64 = 4 * 1024 * 1024;

impl QueryEngine {
    /// Expands refs under a token budget. Never cuts one item in the middle.
    pub fn expand(&self, ctx: &QueryContext, request: &ExpandRequest) -> Result<ExpandResponse> {
        if request.refs.is_empty() || request.refs.len() > MAX_EXPAND_REFS {
            return Err(Error::Invalid("refs must have 1 to 20 items".into()));
        }
        crate::expand_batch::check_request(request)?;
        let budget = clamp_budget(request.token_budget, DEFAULT_EXPAND_BUDGET, self.config.max_budget);
        let mut items = Vec::new();
        let mut used = 0u32;
        let mut truncated = false;
        for value in &request.refs {
            let mut item = match crate::expand_batch::parse_range(value) {
                Some(range) => self.expand_range(ctx, value, range)?,
                None => self.expand_one(ctx, value, request.level, request.expected_revision.as_deref())?,
            };
            self.add_related(ctx, &mut item, &request.with)?;
            let cost = json_tokens(&item) + 1;
            if used + cost > budget {
                truncated = true;
                if !items.is_empty() {
                    break;
                }
                item.content = None;
            }
            used += json_tokens(&item) + 1;
            items.push(item);
        }
        let changed = items.iter().any(|i| i.status == ExpandStatus::Changed);
        let freshness = if changed { Freshness::Partial } else { Freshness::Current };
        let cursor = truncated.then(|| format!("expand:{}", items.len()));
        Ok(ExpandResponse { items, freshness, token_estimate: used, truncated, cursor })
    }

    pub(crate) fn expand_one(&self, ctx: &QueryContext, value: &str, level: ExpandLevel, expected: Option<&str>) -> Result<ExpandItem> {
        let mut item = ExpandItem { entity_ref: value.to_string(), status: ExpandStatus::NotFound, revision: None, path: None, range: None, content: None, provenance: Vec::new(), more: None, related: Vec::new() };
        if level == ExpandLevel::Audit && !ctx.auth.is_admin() {
            item.status = ExpandStatus::Forbidden;
            return Ok(item);
        }
        let Some(loaded) = self.load_allowed(ctx, value)? else {
            let exists = crate::items::load_ref(&self.store, &self.objects, &self.config.tenant, value)?.is_some();
            item.status = if exists { ExpandStatus::Forbidden } else { ExpandStatus::NotFound };
            return Ok(item);
        };
        item.status = ExpandStatus::Ok;
        item.path = loaded.item.path.clone();
        item.range = loaded.item.range;
        item.revision = loaded.item.revision.clone();
        let Ok(parsed) = Ref::parse(value) else { return Ok(item) };
        match parsed.kind.as_str() {
            "sym" => self.expand_symbol(&mut item, &parsed.id, level)?,
            "file" => self.expand_file(&mut item, &parsed.id, level)?,
            "mem" | "task" => self.expand_record(&mut item, &parsed, &loaded.item.title)?,
            "commit" => self.expand_commit(&mut item, &parsed.id)?,
            "ev" => self.expand_event(&mut item, &parsed.id, level)?,
            _ => {}
        }
        if let (Some(expected), Some(revision)) = (expected, &item.revision)
            && !revision.starts_with(expected.trim_start_matches("b3:"))
            && !expected.trim_start_matches("b3:").starts_with(revision.as_str())
        {
            item.status = ExpandStatus::Changed;
        }
        Ok(item)
    }

    fn worktree_root(&self, worktree_id: Option<&String>) -> Option<String> {
        let key = layout::registry(&self.config.tenant, "worktree", worktree_id?);
        self.store.get::<WorktreeRecord>(Cf::Registry, &key).ok().flatten().map(|w| w.root_path)
    }

    fn current_text(&self, root: Option<&str>, path: &str, stored_hash: &str) -> (Option<String>, bool) {
        let file = root.map(|root| Path::new(root).join(path));
        // A file that grew past the read limit is shown from the stored copy (never read whole).
        let readable = file.as_ref().and_then(|f| std::fs::metadata(f).ok()).is_some_and(|m| m.len() <= MAX_READ_BYTES);
        if let Some(file) = file.filter(|_| readable)
            && let Ok(bytes) = std::fs::read(file)
        {
            let changed = zcie_store::ObjectStore::hash(&bytes) != stored_hash;
            return (Some(String::from_utf8_lossy(&bytes).into_owned()), changed);
        }
        (self.objects.get_text(stored_hash).ok(), false)
    }

    fn expand_symbol(&self, item: &mut ExpandItem, id: &str, level: ExpandLevel) -> Result<()> {
        let key = layout::entity(&self.config.tenant, "sym", id);
        let Some(symbol) = self.store.get::<SymbolRecord>(Cf::Entities, &key)? else { return Ok(()) };
        let root = self.worktree_root(symbol.scope.worktree_id.as_ref());
        let (text, changed) = self.current_text(root.as_deref(), &symbol.path, &symbol.file_hash);
        let Some(text) = text else { return Ok(()) };
        let mut span = symbol.span;
        if changed {
            item.status = ExpandStatus::Changed;
            match self.reparse_span(&symbol, &text) {
                Some(new_span) => span = new_span,
                None => return Ok(()),
            }
        }
        item.range = Some(span);
        let lines: Vec<&str> = text.lines().collect();
        let start = (span[0] as usize).saturating_sub(1);
        let mut end = (span[1] as usize).min(lines.len());
        if level == ExpandLevel::Evidence {
            end = end.min(start + EVIDENCE_LINES);
        }
        item.content = (start < end).then(|| lines[start..end].join("\n"));
        Ok(())
    }

    fn reparse_span(&self, symbol: &SymbolRecord, text: &str) -> Option<[u32; 2]> {
        let parsed = zcie_parse::ParserPool::new().parse_file(&symbol.language, &symbol.path, text).ok()?;
        parsed.symbols.into_iter().find(|s| s.qualified_name == symbol.qualified_name && s.kind == symbol.kind).map(|s| s.span)
    }

    fn expand_file(&self, item: &mut ExpandItem, id: &str, level: ExpandLevel) -> Result<()> {
        let key = layout::entity(&self.config.tenant, "file", id);
        let Some(file) = self.store.get::<FileRecord>(Cf::Entities, &key)? else { return Ok(()) };
        if file.is_binary {
            item.content = Some(format!("binary file, {} bytes, hash {}", file.size, file.content_hash));
            return Ok(());
        }
        let root = self.worktree_root(file.scope.worktree_id.as_ref());
        let (text, changed) = self.current_text(root.as_deref(), &file.path, &file.content_hash);
        if changed {
            item.status = ExpandStatus::Changed;
        }
        item.content = text.map(|t| match level {
            ExpandLevel::Evidence => t.lines().take(EVIDENCE_LINES).collect::<Vec<_>>().join("\n"),
            _ => t,
        });
        Ok(())
    }

    fn expand_record(&self, item: &mut ExpandItem, parsed: &Ref, title: &str) -> Result<()> {
        let batch = Batch::new();
        let t = &self.config.tenant;
        if parsed.kind == "mem" {
            if let Some(record) = zcie_memory::get_memory(&self.store, &batch, t, &parsed.id)? {
                item.provenance = record.source_event_ids.iter().map(|e| format!("ev:{e}")).collect();
                item.content = Some(format!("{} ({}, {}): {}", record.kind.name(), record.status.name(), record.confidence, record.text));
            }
            return Ok(());
        }
        if let Some(task) = zcie_memory::get_task(&self.store, &batch, t, &parsed.id)? {
            item.provenance = std::iter::once(format!("ev:{}", task.created_from)).chain(task.history.iter().map(|h| format!("ev:{}", h.event_id))).collect();
            let links = self.link_summary(&item.entity_ref)?;
            item.content = Some(format!("{title}\nstate: {}\n{links}", task.state.name()));
        }
        Ok(())
    }

    fn link_summary(&self, entity_ref: &str) -> Result<String> {
        let t = &self.config.tenant;
        let mut lines = Vec::new();
        for edge in zcie_graph::edges_from(&self.store, t, entity_ref, None)? {
            lines.push(format!("-{}-> {} [{} {}]", edge.relation.name(), edge.to, edge.status.name(), edge.evidence.kind.name()));
        }
        for edge in zcie_graph::edges_to(&self.store, t, entity_ref, None)? {
            lines.push(format!("<-{}- {} [{} {}]", edge.relation.name(), edge.from, edge.status.name(), edge.evidence.kind.name()));
        }
        Ok(lines.join("\n"))
    }

    fn expand_commit(&self, item: &mut ExpandItem, id: &str) -> Result<()> {
        let key = layout::entity(&self.config.tenant, "commit", id);
        if let Some(commit) = self.store.get::<CommitRecord>(Cf::Entities, &key)? {
            item.content = Some(format!("{} {} {}\n{}\n{}", commit.sha, commit.author, commit.author_utc, commit.message.trim(), commit.changed_paths.join("\n")));
        }
        Ok(())
    }

    fn expand_event(&self, item: &mut ExpandItem, id: &str, level: ExpandLevel) -> Result<()> {
        let Some(event) = zcie_store::events::get_event_by_id(&self.store, &self.config.tenant, id)? else { return Ok(()) };
        let text = zcie_store::events::payload_text(&self.objects, &event)?.unwrap_or_default();
        let limit = match level {
            ExpandLevel::Evidence => EVENT_SOURCE_CHARS / 8,
            ExpandLevel::Source => EVENT_SOURCE_CHARS,
            ExpandLevel::Audit => usize::MAX,
        };
        item.revision = Some(format!("seq {}", event.seq));
        item.content = Some(format!("{} {} {}\n{}", event.event_type.name(), event.timestamp_utc, event.source.app, truncate_chars(&text, limit)));
        Ok(())
    }
}
