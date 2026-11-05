//! `zcie memory export` and `zcie memory import`: share the decisions of a project through a file (ADR-032).

use std::path::PathBuf;

use anyhow::Context;
use clap::Subcommand;
use serde_json::{Value, json};
use zcie_sdk::Client;
use zcie_types::EventSource;
use zcie_types::api::{CaptureRequest, ForgetRequest, ForgetSelector};
use zcie_types::event::EVENT_SCHEMA;
use zcie_types::query::{QueryMode, QueryRequest, ResultItem};

use super::project::find_project_id;

/// Memory actions.
#[derive(Debug, Subcommand)]
pub enum MemoryAction {
    /// Write the active decisions and preferences of the project as JSON lines.
    Export {
        /// File to write (default: `.zcie/shared-memory.jsonl` in the project folder).
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Forget the junk memory records: subagent reports stored as memory, and decisions that are
    /// code or report lines. A dry run unless `--apply`.
    Prune {
        /// Forget the records (default: only list them).
        #[arg(long)]
        apply: bool,
        /// Every registered project, not only the current one.
        #[arg(long)]
        all_projects: bool,
    },
    /// Send each line of a file as a user message, so the extraction makes the records again.
    Import {
        /// File to read (default: `.zcie/shared-memory.jsonl` in the project folder).
        file: Option<PathBuf>,
    },
}

/// Runs one memory action.
pub fn run(action: MemoryAction, client: &Client) -> anyhow::Result<()> {
    match action {
        MemoryAction::Export { out } => export(out.map_or_else(default_file, Ok)?, client),
        MemoryAction::Import { file } => import(file.map_or_else(default_file, Ok)?, client),
        MemoryAction::Prune { apply, all_projects } => prune(apply, all_projects, client),
    }
}

/// Returns `.zcie/shared-memory.jsonl` in the project folder of the current folder.
fn default_file() -> anyhow::Result<PathBuf> {
    let cwd = std::env::current_dir()?;
    let root = find_project_id(&cwd).map(|(root, _)| root).unwrap_or(cwd);
    Ok(root.join(".zcie").join("shared-memory.jsonl"))
}

/// The memory records of the workspace (up to 500).
fn query_memory(client: &Client) -> anyhow::Result<Vec<ResultItem>> {
    let request = QueryRequest {
        workspace: client.workspace().to_string(),
        question: "list all decisions and rules".into(),
        mode: QueryMode::Memory,
        token_budget: Some(20000),
        max_results: Some(500),
        ..QueryRequest::default()
    };
    Ok(client.query_request(request)?.results)
}

fn export(path: PathBuf, client: &Client) -> anyhow::Result<()> {
    let mut body = String::new();
    let mut count = 0;
    for item in &query_memory(client)? {
        let Some(evidence) = item.evidence_status.as_deref() else { continue };
        let keep = item.kind == "memory"
            && matches!(item.status.as_deref(), Some("active" | "verified"))
            && (evidence.starts_with("design_decision") || evidence.starts_with("preference"));
        if !keep {
            continue;
        }
        let kind = evidence.split(':').next().unwrap_or(evidence);
        let text = item.snippet.as_deref().unwrap_or(&item.title);
        body.push_str(&json!({"ref": item.entity_ref, "kind": kind, "text": text}).to_string());
        body.push('\n');
        count += 1;
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).with_context(|| format!("cannot create {}", parent.display()))?;
    }
    std::fs::write(&path, body).with_context(|| format!("cannot write {}", path.display()))?;
    println!("{count} decisions exported to {}", path.display());
    Ok(())
}

/// Subagent reports that were stored as user memory: their text starts like this.
const SUBAGENT_PREFIXES: [&str; 3] = ["<agent-message", "Another Claude session sent a message", "[Subagent hand-back]"];

/// Why a memory record is junk, or None. Conservative: a subagent report, or a decision that is
/// code (a code fence) or a report line (40 % or more of its characters are not letters or spaces).
fn junk_reason(kind: &str, text: &str) -> Option<&'static str> {
    let text = text.trim();
    if SUBAGENT_PREFIXES.iter().any(|p| text.starts_with(p)) {
        return Some("subagent report");
    }
    if !matches!(kind, "decision" | "design_decision") {
        return None;
    }
    if text.contains("```") {
        return Some("code in a decision");
    }
    let chars = text.chars().count();
    let letters = text.chars().filter(|c| c.is_alphabetic() || c.is_whitespace()).count();
    (chars > 0 && (chars - letters) * 100 >= chars * 40).then_some("mostly symbols")
}

/// The junk memory records of the workspace of the client: entity ref, first characters, reason.
fn find_junk(client: &Client) -> anyhow::Result<Vec<(String, String, &'static str)>> {
    let mut junk = Vec::new();
    for item in query_memory(client)?.iter().filter(|item| item.kind == "memory") {
        let kind = item.evidence_status.as_deref().unwrap_or_default().split(':').next().unwrap_or_default();
        let text = item.snippet.as_deref().unwrap_or(&item.title);
        if let Some(reason) = junk_reason(kind, text) {
            junk.push((item.entity_ref.clone(), text.trim().chars().take(80).collect::<String>().replace('\n', " "), reason));
        }
    }
    Ok(junk)
}

/// Lists the junk memory records of the current project, or of every project with junk; with
/// `apply`, forgets them.
fn prune(apply: bool, all_projects: bool, client: &Client) -> anyhow::Result<()> {
    let scopes = if all_projects {
        client.list_projects()?.projects.into_iter().map(|p| (p.name, p.project_id)).collect()
    } else {
        vec![(String::new(), client.workspace().to_string())]
    };
    let mut total = 0;
    for (name, workspace) in scopes {
        let scoped = client.with_workspace(&workspace);
        let junk = find_junk(&scoped)?;
        if junk.is_empty() {
            continue;
        }
        if all_projects {
            println!("{name} ({workspace})");
        }
        println!("{} junk memory records", junk.len());
        for (entity_ref, text, reason) in junk.iter().take(10) {
            println!("  {entity_ref}  {text}  ({reason})");
        }
        if apply {
            for (entity_ref, _, _) in &junk {
                let selector = ForgetSelector { entity_ref: Some(entity_ref.clone()), ..ForgetSelector::default() };
                scoped.forget(ForgetRequest { selector, reason: "zcie memory prune".into(), ..ForgetRequest::default() })?;
            }
            println!("{} forgotten", junk.len());
        }
        total += junk.len();
    }
    if all_projects {
        println!("{total} junk memory records in all projects");
    }
    if !apply {
        println!("dry run: run with --apply to forget them");
    }
    Ok(())
}

fn import(path: PathBuf, client: &Client) -> anyhow::Result<()> {
    let text = std::fs::read_to_string(&path).with_context(|| format!("cannot read {}", path.display()))?;
    let (mut imported, mut present) = (0, 0);
    for (index, line) in text.lines().enumerate() {
        let Ok(value) = serde_json::from_str::<Value>(line) else { continue };
        let (Some(entity_ref), Some(body)) = (value["ref"].as_str(), value["text"].as_str()) else { continue };
        if entity_ref.is_empty() || body.trim().is_empty() {
            continue;
        }
        let request = CaptureRequest {
            schema: EVENT_SCHEMA.to_string(),
            idempotency_key: format!("share:{entity_ref}"),
            timestamp_utc: None,
            workspace: client.workspace().to_string(),
            repository_id: None,
            source: EventSource { app: "zcie-share".into(), session_id: "zcie-share".into(), turn_id: Some((index + 1).to_string()) },
            event_type: "user.message".into(),
            payload: Some(body.to_string()),
            git: None,
            classification: None,
        };
        if client.capture(request)?.duplicate {
            present += 1;
        } else {
            imported += 1;
        }
    }
    println!("{imported} imported, {present} already present");
    Ok(())
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::junk_reason;

    #[test]
    fn junk_reason_keeps_decisions_and_prunes_reports_and_code() {
        assert_eq!(junk_reason("decision", "Use Postgres for the ledger."), None);
        assert_eq!(junk_reason("preference", "```rust\nfn x() {}\n```"), None, "only decisions are checked for code");
        assert_eq!(junk_reason("memory", "  <agent-message from=\"a\">done</agent-message>"), Some("subagent report"));
        assert_eq!(junk_reason("design_decision", "[Subagent hand-back] tests pass"), Some("subagent report"));
        assert_eq!(junk_reason("design_decision", "Use:\n```\nlet x = 1;\n```"), Some("code in a decision"));
        assert_eq!(junk_reason("decision", "fn f() { x[0] = 1; y->z(); }"), Some("mostly symbols"));
    }
}
