//! Callers of a symbol (ADR-019, D-070 and D-071). The call sites of each file are stored at index
//! time; the query resolves them to definitions with the current index, so the order of indexing
//! does not matter. A caller is `verified` only through the same file, a qualified path or an
//! import of the file of the target; a single definition in the project gives `inferred`; more
//! definitions give `candidate`. A call that resolves to another definition is not a caller.

use zcie_store::values::FileCallsValue;
use zcie_store::{Cf, layout};
use zcie_types::{CallSite, EdgeStatus, SymbolKind, SymbolRecord};

use crate::error::Result;
use crate::planner::{QueryContext, QueryEngine};

/// Files that use a name that one resolution reads, at most.
const MAX_CALLER_FILES: usize = 200;
/// Definitions with the same name that one resolution reads, at most.
const MAX_DEFINITIONS: usize = 20;
/// File stems that name their folder (`mod.rs`, `index.ts`, `__init__.py`).
const FOLDER_STEMS: [&str; 4] = ["mod", "index", "__init__", "lib"];

/// One resolved caller.
#[derive(Debug, Clone)]
pub(crate) struct Caller {
    /// Path of the calling file.
    pub path: String,
    /// File ID of the calling file.
    pub file_id: String,
    /// Line of the call.
    pub line: u32,
    /// `Verified`, `Inferred` or `Candidate`.
    pub status: EdgeStatus,
    /// Why: `same file`, `qualified path`, `import`, `only definition`, `N definitions`.
    pub reason: String,
}

/// The lowercase names of the module of a file: its stem, and its folder for `mod.rs` and alike.
fn module_names(path: &str) -> Vec<String> {
    let mut parts: Vec<&str> = path.split('/').collect();
    let file = parts.pop().unwrap_or_default();
    let stem = file.split('.').next().unwrap_or_default().to_lowercase();
    match parts.last() {
        Some(folder) if FOLDER_STEMS.contains(&stem.as_str()) => vec![folder.to_lowercase()],
        _ => vec![stem],
    }
}

fn segments(text: &str) -> Vec<String> {
    text.split(|c: char| !(c.is_alphanumeric() || c == '_')).filter(|s| !s.is_empty()).map(str::to_lowercase).collect()
}

/// The container of a symbol (`IndexWriter` of `IndexWriter::commit`), lowercase.
fn container(symbol: &SymbolRecord) -> Option<String> {
    let qualified = symbol.qualified_name.replace("::", ".");
    qualified.rsplit_once('.').map(|(c, _)| c.rsplit('.').next().unwrap_or(c).to_lowercase())
}

/// True when `text` (an import or a qualifier) names the module or the container of `symbol`.
fn names(text: &str, symbol: &SymbolRecord) -> bool {
    let parts = segments(text);
    let modules = module_names(&symbol.path);
    parts.iter().any(|p| modules.contains(p) || container(symbol).as_deref() == Some(p.as_str()))
}

/// Resolves one call of the name of `target` in the file at `path`.
fn resolve(call: &CallSite, path: &str, imports: &[String], target: &SymbolRecord, others: &[SymbolRecord]) -> Option<(EdgeStatus, String)> {
    let local_other = others.iter().any(|d| d.path == path);
    if path == target.path && !local_other && (call.qualifier.is_none() || matches!(call.qualifier.as_deref(), Some("self" | "Self" | "this"))) {
        return Some((EdgeStatus::Verified, "same file".into()));
    }
    if let Some(qualifier) = call.qualifier.as_deref().filter(|q| !matches!(*q, "self" | "Self" | "this")) {
        if names(qualifier, target) {
            return Some((EdgeStatus::Verified, "qualified path".into()));
        }
        if others.iter().any(|d| names(qualifier, d)) {
            return None;
        }
    }
    if local_other && call.qualifier.is_none() {
        return None;
    }
    let imported = |symbol: &SymbolRecord| imports.iter().any(|i| names(i, symbol));
    if !call.method && imported(target) && !others.iter().any(imported) {
        return Some((EdgeStatus::Verified, "import".into()));
    }
    // The file imports the module of another definition: the call goes there.
    if !call.method && !imported(target) && others.iter().any(imported) {
        return None;
    }
    match others.len() {
        0 => Some((EdgeStatus::Inferred, "only definition".into())),
        n => Some((EdgeStatus::Candidate, format!("{} definitions", n + 1))),
    }
}

fn status_rank(status: EdgeStatus) -> u8 {
    match status {
        EdgeStatus::Verified => 0,
        EdgeStatus::Inferred => 1,
        _ => 2,
    }
}

impl QueryEngine {
    /// Returns the callers of a symbol, the best evidence first (at most `limit`).
    pub(crate) fn callers_of(&self, ctx: &QueryContext, target: &SymbolRecord, limit: usize) -> Result<Vec<Caller>> {
        let t = &self.config.tenant;
        let all = zcie_symbols::find_by_name(&self.store, t, &ctx.scope.workspace_id, &target.name, MAX_DEFINITIONS)?;
        let others: Vec<SymbolRecord> = all.into_iter().filter(|d| d.symbol_id != target.symbol_id && d.kind != SymbolKind::Module).collect();
        let graph = self.repo_graph(&ctx.scope.workspace_id)?;
        let mut files: Vec<(String, String)> = graph.users_of(&target.name, MAX_CALLER_FILES).into_iter().map(|(p, f)| (p.to_string(), f.to_string())).collect();
        if !files.iter().any(|(p, _)| *p == target.path) {
            files.push((target.path.clone(), target.file_id.to_string()));
        }
        let mut out = Vec::new();
        for (path, file_id) in files {
            let Some(stored) = self.store.get::<FileCallsValue>(Cf::Lookup, &layout::file_calls(t, &file_id))? else { continue };
            for call in stored.calls.iter().filter(|c| c.name == target.name) {
                if let Some((status, reason)) = resolve(call, &path, &stored.imports, target, &others) {
                    out.push(Caller { path: path.clone(), file_id: file_id.clone(), line: call.line, status, reason });
                }
            }
        }
        out.sort_by(|a, b| status_rank(a.status).cmp(&status_rank(b.status)).then_with(|| a.path.cmp(&b.path)).then(a.line.cmp(&b.line)));
        out.truncate(limit);
        Ok(out)
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;

    fn symbol(path: &str, qualified: &str) -> SymbolRecord {
        let mut s: SymbolRecord = serde_json::from_value(serde_json::json!({
            "v": 1, "symbol_id": "sym_01ARZ3NDEKTSV4RRFFQ69G5FAV", "scope": {"tenant_id": "local", "principal_id": "p", "workspace_id": "ws"},
            "file_id": "file_01ARZ3NDEKTSV4RRFFQ69G5FAV", "path": path, "language": "rust", "name": "apply",
            "qualified_name": qualified, "kind": "function", "signature": "fn apply()", "span": [1, 3], "file_hash": "b3:x", "parser_quality": "complete"
        })).expect("symbol");
        s.path = path.into();
        s
    }

    fn call(qualifier: Option<&str>, method: bool) -> CallSite {
        CallSite { name: "apply".into(), qualifier: qualifier.map(str::to_string), method, line: 5 }
    }

    #[test]
    fn calls_resolve_by_file_path_and_import() {
        let target = symbol("src/fees.rs", "apply");
        let other = symbol("src/tax.rs", "apply");
        let imports = vec!["crate::fees::apply".to_string()];
        assert_eq!(resolve(&call(None, false), "src/fees.rs", &[], &target, &[]).map(|r| r.0), Some(EdgeStatus::Verified));
        assert_eq!(resolve(&call(Some("fees"), false), "src/a.rs", &[], &target, std::slice::from_ref(&other)).map(|r| r.0), Some(EdgeStatus::Verified));
        assert_eq!(resolve(&call(Some("tax"), false), "src/a.rs", &[], &target, std::slice::from_ref(&other)), None);
        assert_eq!(resolve(&call(None, false), "src/a.rs", &imports, &target, std::slice::from_ref(&other)).map(|r| r.0), Some(EdgeStatus::Verified));
        assert_eq!(resolve(&call(None, false), "src/a.rs", &[], &target, std::slice::from_ref(&other)).map(|r| r.0), Some(EdgeStatus::Candidate));
        assert_eq!(resolve(&call(Some("x"), true), "src/a.rs", &[], &target, &[]).map(|r| r.0), Some(EdgeStatus::Inferred));
        assert_eq!(resolve(&call(None, false), "src/tax.rs", &[], &target, std::slice::from_ref(&other)), None, "a local definition wins");
        let tax_imports = vec!["crate::tax::apply".to_string()];
        assert_eq!(resolve(&call(None, false), "src/a.rs", &tax_imports, &target, std::slice::from_ref(&other)), None, "an import of the other module");
    }
}
