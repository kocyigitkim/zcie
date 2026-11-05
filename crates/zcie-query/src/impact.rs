//! Change impact and file history of the first symbol of a response (ADR-031, D-105, D-106).

use std::collections::HashSet;

use zcie_store::values::CommitIdValue;
use zcie_store::{Cf, layout};
use zcie_types::{
    CommandRunPayload, CommitRecord, EdgeStatus, EventType, FileChangePayload, MemoryRecord, QueryResponse, SymbolKind, SymbolRecord, truncate_chars,
};
use zcie_types::evidence::QueryIntent;

use crate::error::Result;
use crate::expand_batch::is_test_path;
use crate::planner::{QueryContext, QueryEngine};
use crate::relevance::{item_coverage, stems};

/// Most direct callers that are listed.
const MAX_CALLERS: usize = 5;
/// Most tests that are listed.
const MAX_TESTS: usize = 5;
/// Most direct callers whose own callers are read for tests (the second step).
const MAX_STEP_CALLERS: usize = 5;
/// Files with the name of the target that one caller resolution reads, at most.
const CALLER_FILES: usize = 40;
/// Most commits that are listed.
const MAX_COMMITS: usize = 3;
/// Most commit records of one repository that are read.
const MAX_SCAN_COMMITS: usize = 5_000;
/// Characters of a commit message line.
const MESSAGE_CHARS: usize = 100;
/// Share of the question terms that an earlier request must contain (ADR-032).
const MIN_TASK_COVERAGE: f32 = 0.6;
/// Characters of the earlier request in a similar task line.
const TASK_CHARS: usize = 100;
/// Most changed paths of a similar earlier task that are listed.
const MAX_TASK_PATHS: usize = 5;
/// Most passed commands of a similar earlier task that are listed.
const MAX_TASK_COMMANDS: usize = 2;
/// Characters of a passed command.
const COMMAND_CHARS: usize = 80;
/// Events of the earlier turn that are read.
const MAX_TURN_EVENTS: usize = 500;

impl QueryEngine {
    /// Adds the impact lines (callers and tests for `edit`, commits for `edit`, `explain` and `trace`).
    pub(crate) fn add_impact(&self, ctx: &QueryContext, question: &str, response: &mut QueryResponse) -> Result<()> {
        let edit = response.intent == Some(QueryIntent::Edit);
        if !edit && !matches!(response.intent, Some(QueryIntent::Explain | QueryIntent::Trace)) {
            return Ok(());
        }
        if edit {
            self.add_similar_task(ctx, question, response)?;
        }
        let first = response.results.iter().find(|r| r.kind == "symbol" && !r.path.as_deref().is_some_and(is_test_path));
        let Some(id) = first.and_then(|r| r.entity_ref.strip_prefix("sym:")) else { return Ok(()) };
        let t = &self.config.tenant;
        let Some(symbol) = self.store.get::<SymbolRecord>(Cf::Entities, &layout::entity(t, "sym", id))? else { return Ok(()) };
        if edit {
            self.add_callers_and_tests(ctx, &symbol, &mut response.impact)?;
        }
        self.add_commits(&symbol, &mut response.impact)?;
        Ok(())
    }

    /// Adds the line of an earlier request that repeats the question, with the files its turn changed
    /// and the commands it passed (ADR-032, D-110). Adds nothing when that turn changed no file.
    fn add_similar_task(&self, ctx: &QueryContext, question: &str, response: &mut QueryResponse) -> Result<()> {
        let terms = stems(question);
        let Some(memory) = response.results.iter().find(|r| {
            r.kind == "memory" && r.evidence_status.as_deref().is_some_and(|e| e.starts_with("user_requirement")) && item_coverage(&terms, question, r) >= MIN_TASK_COVERAGE
        }) else {
            return Ok(());
        };
        let t = &self.config.tenant;
        let Some(id) = memory.entity_ref.strip_prefix("mem:") else { return Ok(()) };
        let Some(record) = self.store.get::<MemoryRecord>(Cf::Entities, &layout::entity(t, "mem", id))? else { return Ok(()) };
        let Some(source) = record.source_event_ids.first() else { return Ok(()) };
        let Some(event) = zcie_store::events::get_event_by_id(&self.store, t, &source.0)? else { return Ok(()) };
        let Some(turn) = event.source.turn_id.as_deref() else { return Ok(()) };
        let session = event.source.session_id.as_str();
        let text = memory.snippet.as_deref().unwrap_or(&memory.title);
        let request = truncate_chars(&text.split_whitespace().collect::<Vec<_>>().join(" "), TASK_CHARS);

        // A root and an event path can name the same folder through a link (`/var` and `/private/var` on macOS).
        let roots: Vec<String> = zcie_memory::worktrees_of_workspace(&self.store, t, &ctx.scope.workspace_id)?
            .into_iter()
            .flat_map(|w| {
                let real = std::fs::canonicalize(&w.root_path).ok().map(|p| p.to_string_lossy().into_owned());
                std::iter::once(w.root_path).chain(real)
            })
            .collect();
        let (mut paths, mut commands) = (Vec::new(), Vec::new());
        for key in self.store.prefix_keys(Cf::Lookup, &layout::turn_prefix(t, session, turn), MAX_TURN_EVENTS)? {
            let Some(event) = zcie_store::events::get_event_by_id(&self.store, t, &zcie_store::keys::last_part(&key))? else { continue };
            if event.forgotten {
                continue;
            }
            let Ok(Some(payload)) = zcie_store::events::payload_text(&self.objects, &event) else { continue };
            match event.event_type {
                EventType::FileChanged => {
                    let Ok(change) = serde_json::from_str::<FileChangePayload>(&payload) else { continue };
                    let real = std::fs::canonicalize(&change.path).ok().map(|p| p.to_string_lossy().into_owned());
                    let forms = std::iter::once(change.path.as_str()).chain(real.as_deref());
                    let path = forms.clone().find_map(|f| roots.iter().find_map(|root| f.strip_prefix(&format!("{root}/")))).unwrap_or(change.path.as_str());
                    if paths.len() < MAX_TASK_PATHS && !paths.iter().any(|p: &String| p == path) {
                        paths.push(path.to_string());
                    }
                }
                EventType::CommandRun => {
                    let Ok(run) = serde_json::from_str::<CommandRunPayload>(&payload) else { continue };
                    let line = truncate_chars(run.command.lines().next().unwrap_or_default(), COMMAND_CHARS);
                    if run.exit_code == Some(0) && !line.is_empty() && commands.len() < MAX_TASK_COMMANDS && !commands.contains(&line) {
                        commands.push(line);
                    }
                }
                _ => {}
            }
        }
        if paths.is_empty() {
            return Ok(());
        }
        let mut line = format!("similar earlier task: \"{request}\" changed {}", paths.join(", "));
        if !commands.is_empty() {
            line.push_str(&format!("; passed: {}", commands.join(" | ")));
        }
        response.impact.push(line);
        Ok(())
    }

    fn add_callers_and_tests(&self, ctx: &QueryContext, symbol: &SymbolRecord, out: &mut Vec<String>) -> Result<()> {
        let inside = |path: &str, line: u32, s: &SymbolRecord| path == s.path && (s.span[0]..=s.span[1]).contains(&line);
        let (mut callers, mut tests, mut seen) = (Vec::new(), Vec::new(), HashSet::new());
        let mut steps = 0;
        for caller in self.callers_of(ctx, symbol, CALLER_FILES)?.into_iter().filter(|c| !inside(&c.path, c.line, symbol)) {
            let text = self.line_of(&caller.file_id, caller.line)?.unwrap_or_default();
            if is_test_path(&caller.path) {
                if tests.len() < MAX_TESTS && seen.insert((caller.path.clone(), caller.line)) {
                    tests.push(format!("test {}:{} {text} [{}: {}]", caller.path, caller.line, caller.status.name(), caller.reason));
                }
                continue;
            }
            if callers.len() < MAX_CALLERS {
                callers.push(format!("caller {}:{} {text} [{}: {}]", caller.path, caller.line, caller.status.name(), caller.reason));
            }
            // Second step: tests that call the function around this call.
            if steps >= MAX_STEP_CALLERS || tests.len() >= MAX_TESTS || caller.status == EdgeStatus::Candidate {
                continue;
            }
            steps += 1;
            let around = zcie_symbols::file_symbols(&self.store, &self.config.tenant, &caller.file_id)?
                .into_iter()
                .filter(|s| s.kind != SymbolKind::Module && inside(&caller.path, caller.line, s))
                .min_by_key(|s| s.span[1] - s.span[0]);
            let Some(around) = around else { continue };
            for test in self.callers_of(ctx, &around, CALLER_FILES)?.into_iter().filter(|c| is_test_path(&c.path) && !inside(&c.path, c.line, &around)) {
                if tests.len() < MAX_TESTS && seen.insert((test.path.clone(), test.line)) {
                    let text = self.line_of(&test.file_id, test.line)?.unwrap_or_default();
                    tests.push(format!("test {}:{} {text} [via {}]", test.path, test.line, around.name));
                }
            }
        }
        out.extend(callers);
        out.extend(tests);
        Ok(())
    }

    /// The newest indexed commits that changed the file of the symbol.
    fn add_commits(&self, symbol: &SymbolRecord, out: &mut Vec<String>) -> Result<()> {
        let Some(repository) = symbol.scope.repository_id.as_deref() else { return Ok(()) };
        let t = &self.config.tenant;
        let ids = self.store.prefix_scan::<CommitIdValue>(Cf::Lookup, &layout::commits_prefix(t, repository), MAX_SCAN_COMMITS)?;
        let keys: Vec<Vec<u8>> = ids.iter().map(|(_, v)| layout::entity(t, "commit", &v.commit_id)).collect();
        let mut commits: Vec<CommitRecord> = self
            .store
            .multi_get::<CommitRecord>(Cf::Entities, &keys)?
            .into_iter()
            .flatten()
            .filter(|c| c.changed_paths.iter().any(|p| p.split_once(' ').is_some_and(|(_, path)| path == symbol.path || path.ends_with(&format!(" {}", symbol.path)))))
            .collect();
        commits.sort_by(|a, b| b.author_utc.cmp(&a.author_utc));
        for commit in commits.into_iter().take(MAX_COMMITS) {
            let message = truncate_chars(commit.message.lines().next().unwrap_or_default(), MESSAGE_CHARS);
            let date: String = commit.author_utc.chars().take(10).collect();
            out.push(format!("commit {} {date} {message}", commit.sha.chars().take(7).collect::<String>()));
        }
        Ok(())
    }
}
