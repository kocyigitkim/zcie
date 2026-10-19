//! Batch expand (ADR-017, D-063; the windows of SWE-agent). One `zcie_expand` call returns line
//! ranges `path:start-end` of several files, and for a symbol ref the lines of its callers and its
//! tests. The model then reads only the parts it needs, not whole files one by one.

use zcie_store::values::FileIdValue;
use zcie_store::{Cf, layout};
use zcie_types::{ExpandItem, ExpandLevel, ExpandRequest, ExpandStatus, FileRecord, SymbolRecord, truncate_chars};

use crate::error::{Error, Result};
use crate::planner::{QueryContext, QueryEngine};

/// Most line ranges in one expand request.
pub const MAX_RANGES: usize = 10;
/// Most lines of one range. A longer range gets a "more" note.
pub const MAX_RANGE_LINES: u32 = 100;
/// Most related lines of each kind (`callers`, `tests`).
pub const MAX_RELATED: usize = 5;
/// Files that use a name that one request reads, at most.
const MAX_USER_FILES: usize = 40;
/// Characters of one related line.
const RELATED_CHARS: usize = 160;
/// The permitted values of `with`.
const WITH_VALUES: [&str; 2] = ["callers", "tests"];

/// Checks the limits of a batch request: at most 10 ranges, and known `with` values.
pub(crate) fn check_request(request: &ExpandRequest) -> Result<()> {
    if request.refs.iter().filter(|r| parse_range(r).is_some()).count() > MAX_RANGES {
        return Err(Error::Invalid(format!("refs must have at most {MAX_RANGES} line ranges")));
    }
    if let Some(bad) = request.with.iter().find(|w| !WITH_VALUES.contains(&w.as_str())) {
        return Err(Error::Invalid(format!("unknown with value: {bad} (use callers, tests)")));
    }
    Ok(())
}

/// Returns (path, start, end) of a line range `path:start-end` or `path:line`. Lines start at 1.
pub(crate) fn parse_range(value: &str) -> Option<(String, u32, u32)> {
    let (path, lines) = value.rsplit_once(':')?;
    if path.is_empty() || path.contains(char::is_whitespace) || !(path.contains('.') || path.contains('/')) {
        return None;
    }
    let (start, end) = match lines.split_once('-') {
        Some((a, b)) => (a.trim().parse().ok()?, b.trim().parse().ok()?),
        None => {
            let line = lines.trim().parse().ok()?;
            (line, line)
        }
    };
    (start >= 1 && end >= start).then(|| (path.to_string(), start, end))
}

/// True for a test file path.
fn is_test_path(path: &str) -> bool {
    let lower = path.to_lowercase();
    lower.contains("test") || lower.contains("spec")
}

impl QueryEngine {
    /// Returns the `file:` ref of a path in the scope project. An absolute path inside a root
    /// folder of the project is made relative.
    fn file_of_path(&self, ctx: &QueryContext, path: &str) -> Result<Option<String>> {
        let t = &self.config.tenant;
        for worktree in zcie_memory::worktrees_of_workspace(&self.store, t, &ctx.scope.workspace_id)? {
            let relative = path.strip_prefix(&format!("{}/", worktree.root_path)).unwrap_or(path).trim_start_matches("./");
            if let Some(file) = self.store.get::<FileIdValue>(Cf::Lookup, &layout::path(t, &worktree.worktree_id, relative))? {
                return Ok(Some(format!("file:{}", file.file_id)));
            }
        }
        Ok(None)
    }

    /// Expands one line range. The scope check and the change check are those of the file ref.
    pub(crate) fn expand_range(&self, ctx: &QueryContext, value: &str, (path, start, end): (String, u32, u32)) -> Result<ExpandItem> {
        let mut item = match self.file_of_path(ctx, &path)? {
            Some(file_ref) => self.expand_one(ctx, &file_ref, ExpandLevel::Source, None)?,
            None => ExpandItem { entity_ref: String::new(), status: ExpandStatus::NotFound, revision: None, path: Some(path.clone()), range: None, content: None, provenance: Vec::new(), more: None, related: Vec::new() },
        };
        item.entity_ref = value.to_string();
        let Some(text) = item.content.take() else { return Ok(item) };
        let lines: Vec<&str> = text.lines().collect();
        let total = u32::try_from(lines.len()).unwrap_or(u32::MAX);
        let wanted_end = end.min(total);
        if start > wanted_end {
            item.range = None;
            return Ok(item);
        }
        let last = wanted_end.min(start + MAX_RANGE_LINES - 1);
        item.range = Some([start, last]);
        item.content = Some(lines[(start - 1) as usize..last as usize].join("\n"));
        let shown = item.path.clone().unwrap_or(path);
        item.more = (last < wanted_end).then(|| format!("{shown}:{}-{wanted_end}", last + 1));
        Ok(item)
    }

    /// Adds the lines of the callers and the tests of a symbol ref (`with`). Each kind has at most
    /// `MAX_RELATED` lines: "caller path:line text [status: reason]" or "test path:line text [...]".
    /// The callers are resolved calls (D-071): verified ones first, never a call of another
    /// definition with the same name.
    pub(crate) fn add_related(&self, ctx: &QueryContext, item: &mut ExpandItem, with: &[String]) -> Result<()> {
        let ok = matches!(item.status, ExpandStatus::Ok | ExpandStatus::Changed);
        let Some(id) = item.entity_ref.strip_prefix("sym:").filter(|_| ok && !with.is_empty()) else { return Ok(()) };
        let t = &self.config.tenant;
        let Some(symbol) = self.store.get::<SymbolRecord>(Cf::Entities, &layout::entity(t, "sym", id))? else { return Ok(()) };
        let (mut callers, mut tests) = (Vec::new(), Vec::new());
        let inside = |c: &crate::calls::Caller| c.path == symbol.path && (symbol.span[0]..=symbol.span[1]).contains(&c.line);
        for caller in self.callers_of(ctx, &symbol, MAX_USER_FILES)?.into_iter().filter(|c| !inside(c)) {
            let test = is_test_path(&caller.path);
            let (list, label) = if test { (&mut tests, "test") } else { (&mut callers, "caller") };
            if list.len() >= MAX_RELATED {
                continue;
            }
            let text = self.line_of(&caller.file_id, caller.line)?.unwrap_or_default();
            list.push(format!("{label} {}:{} {text} [{}: {}]", caller.path, caller.line, caller.status.name(), caller.reason));
        }
        let wants = |kind: &str| with.iter().any(|w| w == kind);
        item.related = callers.into_iter().filter(|_| wants("callers")).chain(tests.into_iter().filter(|_| wants("tests"))).collect();
        Ok(())
    }

    /// Returns one line of a stored file, trimmed and cut to `RELATED_CHARS`.
    fn line_of(&self, file_id: &str, line: u32) -> Result<Option<String>> {
        let Some(file) = self.store.get::<FileRecord>(Cf::Entities, &layout::entity(&self.config.tenant, "file", file_id))? else { return Ok(None) };
        let Ok(text) = self.objects.get_text(&file.content_hash) else { return Ok(None) };
        Ok(text.lines().nth(line as usize - 1).map(|l| truncate_chars(l.trim(), RELATED_CHARS)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranges_are_parsed_and_refs_are_not() {
        assert_eq!(parse_range("src/a.rs:10-20"), Some(("src/a.rs".into(), 10, 20)));
        assert_eq!(parse_range("README.md:5"), Some(("README.md".into(), 5, 5)));
        assert_eq!(parse_range("sym:sym_01ABC"), None);
        assert_eq!(parse_range("file:file_01"), None);
        assert_eq!(parse_range("src/a.rs:20-10"), None);
        assert_eq!(parse_range("src/a.rs:0-3"), None);
    }
}
