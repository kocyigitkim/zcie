//! "Tell me about this project" questions (ADR-016): the answer is a project card from the index,
//! not the best word matches. The card has the root folders, the top-level files and folders, the
//! languages, the projects inside the folder, the start of the README and the newest decisions.

use std::collections::BTreeMap;

use zcie_store::values::FileIdValue;
use zcie_store::{Cf, layout};
use zcie_types::{FileRecord, QueryClass, QueryResponse, Watermarks, WorkspaceRecord, WorktreeRecord, truncate_chars};

use crate::error::Result;
use crate::planner::{QueryContext, QueryEngine};
use crate::routes::Candidate;

/// Phrases that ask about the project as a whole (lowercase).
const OVERVIEW_PHRASES: [&str; 18] = [
    "bu proje hakkında", "proje hakkında bilgi", "projeyi anlat", "projeyi tanıt", "bu proje ne", "proje ne yapıyor",
    "projenin amacı", "projeyi özetle", "proje özeti", "bu projede neler var",
    "what is this project", "about this project", "describe this project", "explain this project",
    "project overview", "overview of this project", "what does this project do", "summarize this project",
];
/// Characters of the README in the card.
const README_CHARS: usize = 1_500;
/// Top-level entries in the card.
const MAX_ENTRIES: usize = 30;
/// Paths of one folder that the card reads.
const MAX_PATHS: usize = 50_000;
/// Decisions in the card.
const MAX_DECISIONS: usize = 5;
/// Token budget of the repository map in a project card.
const OVERVIEW_MAP_BUDGET: u32 = 300;
/// Token budget of a project card (ADR-017, D-056).
const OVERVIEW_BUDGET: u32 = 900;
/// Note of an overview answer.
pub const OVERVIEW_NOTE: &str = "project overview";

/// True for a question about the project as a whole.
pub fn asks_for_overview(question: &str) -> bool {
    let lower = question.to_lowercase();
    OVERVIEW_PHRASES.iter().any(|p| lower.contains(p))
}

/// Returns "name (N files)" for the top-level entries of a list of relative paths, and the
/// languages by file extension.
fn layout_of(paths: &[String]) -> (Vec<String>, Vec<String>) {
    let mut top: BTreeMap<String, usize> = BTreeMap::new();
    let mut extensions: BTreeMap<String, usize> = BTreeMap::new();
    for path in paths {
        match path.split_once('/') {
            Some((folder, _)) => *top.entry(format!("{folder}/")).or_default() += 1,
            None => *top.entry(path.clone()).or_default() += 0,
        }
        if let Some((_, ext)) = path.rsplit_once('.').filter(|(_, e)| e.len() <= 5 && !e.contains('/')) {
            *extensions.entry(ext.to_lowercase()).or_default() += 1;
        }
    }
    let entries = top.into_iter().take(MAX_ENTRIES).map(|(name, n)| if n > 0 { format!("{name} ({n} files)") } else { name }).collect();
    let mut languages: Vec<(String, usize)> = extensions.into_iter().collect();
    languages.sort_by_key(|l| std::cmp::Reverse(l.1));
    (entries, languages.into_iter().take(6).map(|(e, n)| format!("{e} {n}")).collect())
}

impl QueryEngine {
    fn worktrees(&self) -> Result<Vec<WorktreeRecord>> {
        let prefix = layout::registry_prefix(&self.config.tenant, "worktree");
        Ok(self.store.prefix_scan::<WorktreeRecord>(Cf::Registry, &prefix, 10_000)?.into_iter().map(|(_, w)| w).collect())
    }

    /// Returns the projects whose root folders are inside `root`, with their file counts.
    fn inner_projects(&self, workspace: &str, root: &str, all: &[WorktreeRecord]) -> Result<Vec<String>> {
        let mut out = Vec::new();
        for inner in all.iter().filter(|w| w.workspace_id != workspace && w.root_path.starts_with(&format!("{root}/"))) {
            let key = layout::registry(&self.config.tenant, "workspace", &inner.workspace_id);
            let name = self.store.get::<WorkspaceRecord>(Cf::Registry, &key)?.map(|w| w.name).unwrap_or_default();
            let files = zcie_store::files::current_paths(&self.store, &self.config.tenant, &inner.worktree_id, MAX_PATHS)?.len();
            let relative = inner.root_path.trim_start_matches(root).trim_start_matches('/');
            let state = if inner.indexed { format!("{files} files indexed") } else { "not indexed".to_string() };
            out.push(format!("{relative}/ = project {name} ({state})"));
        }
        Ok(out)
    }

    /// Returns the README file of a worktree and its first characters.
    fn readme(&self, worktree: &WorktreeRecord) -> Result<Option<(String, String)>> {
        for name in ["README.md", "readme.md", "README", "README.txt", "CLAUDE.md"] {
            let key = layout::path(&self.config.tenant, &worktree.worktree_id, name);
            let Some(id) = self.store.get::<FileIdValue>(Cf::Lookup, &key)? else { continue };
            let record: Option<FileRecord> = self.store.get(Cf::Entities, &layout::entity(&self.config.tenant, "file", &id.file_id))?;
            // The stored content only: a query never reads project folders (on macOS the first read of a
            // launchd daemon in a protected folder can wait for a privacy prompt).
            if let Some(text) = record.filter(|r| !r.deleted).and_then(|r| self.objects.get_text(&r.content_hash).ok()) {
                return Ok(Some((format!("file:{}", id.file_id), truncate_chars(&text, README_CHARS))));
            }
        }
        Ok(None)
    }

    /// Returns the card text of one root folder and its README candidate.
    fn root_card(&self, ctx: &QueryContext, worktree: &WorktreeRecord, all: &[WorktreeRecord]) -> Result<(String, Option<Candidate>)> {
        let paths = zcie_store::files::current_paths(&self.store, &self.config.tenant, &worktree.worktree_id, MAX_PATHS)?;
        let (entries, languages) = layout_of(&paths);
        let mut text = format!("Folder {} ({} files indexed{}).", worktree.root_path, paths.len(), if worktree.indexed { "" } else { "; not indexed: add it with zcie project add" });
        if !entries.is_empty() {
            text.push_str(&format!(" Top level: {}.", entries.join(", ")));
        }
        if !languages.is_empty() {
            text.push_str(&format!(" File types: {}.", languages.join(", ")));
        }
        let inner = self.inner_projects(&worktree.workspace_id, &worktree.root_path, all)?;
        if !inner.is_empty() {
            text.push_str(&format!(" Projects inside: {}.", inner.join("; ")));
        }
        let readme = match self.readme(worktree)? {
            Some((entity_ref, head)) => self.candidate(ctx, &entity_ref, 1.0, None)?.map(|mut c| {
                c.loaded.item.snippet = Some(head);
                c
            }),
            None => None,
        };
        Ok((text, readme))
    }

    /// Answers an overview question with the project card. Returns `None` for other questions.
    pub(crate) fn overview(&self, ctx: &QueryContext, (class, question): (QueryClass, &str), watermarks: Watermarks, budget: u32) -> Result<Option<QueryResponse>> {
        if !asks_for_overview(question) {
            return Ok(None);
        }
        let all = self.worktrees()?;
        let mut card = Vec::new();
        let mut candidates = Vec::new();
        for worktree in all.iter().filter(|w| w.workspace_id == ctx.scope.workspace_id) {
            let (text, readme) = self.root_card(ctx, worktree, &all)?;
            card.push(text);
            candidates.extend(readme);
        }
        for decision in zcie_memory::decisions_of_workspace(&self.store, &self.config.tenant, &ctx.scope.workspace_id)?.into_iter().rev().take(MAX_DECISIONS) {
            candidates.extend(self.candidate(ctx, &format!("mem:{}", decision.memory_id), 0.9, None)?);
        }
        let mut answer = format!("Project card from the local index (ZCIE). {}", card.join(" "));
        let map = self.map_text(ctx, "", OVERVIEW_MAP_BUDGET)?;
        if !map.is_empty() {
            answer.push_str(&format!("\nMost used files and their main symbols:\n{map}"));
        }
        let budget = if self.config.context.capsule_rules { budget.min(OVERVIEW_BUDGET) } else { budget };
        let mut response = self.compose(ctx, (class, Some(OVERVIEW_NOTE.to_string()), question), candidates, watermarks, 0, budget, MAX_DECISIONS + 2);
        response.answer = Some(answer);
        response.answer_kind = zcie_types::AnswerKind::Local;
        Ok(Some(response))
    }
}
