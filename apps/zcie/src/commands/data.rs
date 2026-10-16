//! Data commands: repo, query, expand, link, forget, health.

use anyhow::{Context, bail};
use clap::{Args, Subcommand};
use zcie_sdk::Client;
use zcie_types::Relation;
use zcie_types::api::{ForgetRequest, ForgetSelector, LinkRequest};
use zcie_types::query::{ExpandLevel, QueryMode, QueryRequest};

use super::print_json;

/// Repository actions.
#[derive(Debug, Subcommand)]
pub enum RepoAction {
    /// Register a repository folder and start a full scan.
    Add {
        /// Path to the repository folder.
        path: String,
        /// Optional project name.
        #[arg(long)]
        project: Option<String>,
    },
}

/// Arguments of `zcie query`.
#[derive(Debug, Args)]
pub struct QueryArgs {
    /// Question, symbol name or ref.
    pub question: String,
    /// Query mode: auto, exact, search, timeline, memory.
    #[arg(long, default_value = "auto")]
    pub mode: String,
    /// Token budget.
    #[arg(long)]
    pub budget: Option<u32>,
    /// Maximum results.
    #[arg(long)]
    pub max_results: Option<u32>,
    /// Search all projects.
    #[arg(long)]
    pub global: bool,
}

/// Arguments of `zcie expand`.
#[derive(Debug, Args)]
pub struct ExpandArgs {
    /// Refs to expand.
    #[arg(required = true)]
    pub refs: Vec<String>,
    /// Level: evidence, source, audit.
    #[arg(long, default_value = "evidence")]
    pub level: String,
    /// Token budget.
    #[arg(long)]
    pub budget: Option<u32>,
}

/// Arguments of `zcie link`.
#[derive(Debug, Args)]
pub struct LinkArgs {
    /// Source ref.
    pub from: String,
    /// Relation, for example IMPLEMENTS.
    pub relation: String,
    /// Target ref.
    pub to: String,
    /// The event that supports the link.
    #[arg(long = "event")]
    pub source_event_id: String,
    /// Short note.
    #[arg(long)]
    pub note: Option<String>,
}

/// Arguments of `zcie forget`. Give exactly one selector.
#[derive(Debug, Args)]
pub struct ForgetArgs {
    /// Forget one entity or event ref.
    #[arg(long = "ref")]
    pub entity_ref: Option<String>,
    /// Forget all events of a session.
    #[arg(long)]
    pub session: Option<String>,
    /// Forget all events before this RFC 3339 time.
    #[arg(long)]
    pub before: Option<String>,
    /// Reason for the audit record.
    #[arg(long, default_value = "cli")]
    pub reason: String,
}

/// Runs a repository action.
pub fn run_repo(action: RepoAction, client: &Client) -> anyhow::Result<()> {
    match action {
        RepoAction::Add { path, project } => {
            let absolute = std::fs::canonicalize(&path).with_context(|| format!("cannot open {path}"))?;
            let path = absolute.to_string_lossy();
            print_json(&client.register_repository(&path, project.as_deref())?)
        }
    }
}

/// Runs `zcie query`.
pub fn run_query(args: QueryArgs, client: &Client) -> anyhow::Result<()> {
    let mode: QueryMode = parse_enum(&args.mode, "mode")?;
    let request = QueryRequest {
        workspace: client.workspace().to_string(),
        question: args.question,
        mode,
        token_budget: args.budget,
        max_results: args.max_results,
        scope: if args.global { zcie_types::QueryScope::Global } else { zcie_types::QueryScope::Project },
        cross_project: true,
        ..QueryRequest::default()
    };
    print_json(&client.query_request(request)?)
}

/// Runs `zcie expand`.
pub fn run_expand(args: ExpandArgs, client: &Client) -> anyhow::Result<()> {
    let level: ExpandLevel = parse_enum(&args.level, "level")?;
    print_json(&client.expand(&args.refs, level, args.budget)?)
}

/// Runs `zcie link` with user confirmation evidence.
pub fn run_link(args: LinkArgs, client: &Client) -> anyhow::Result<()> {
    let name = args.relation.to_uppercase();
    let Some(relation) = Relation::from_name(&name) else {
        bail!("unknown relation {name}");
    };
    let request = LinkRequest {
        workspace: client.workspace().to_string(),
        from: args.from,
        relation,
        to: args.to,
        evidence: "user_confirmation".to_string(),
        source_event_id: args.source_event_id,
        note: args.note,
    };
    print_json(&client.link(request)?)
}

/// Runs `zcie forget`.
pub fn run_forget(args: ForgetArgs, client: &Client) -> anyhow::Result<()> {
    let count = [&args.entity_ref, &args.session, &args.before].iter().filter(|v| v.is_some()).count();
    if count != 1 {
        bail!("give exactly one of --ref, --session, --before");
    }
    let request = ForgetRequest {
        workspace: client.workspace().to_string(),
        selector: ForgetSelector { entity_ref: args.entity_ref, session_id: args.session, before_utc: args.before },
        reason: args.reason,
    };
    print_json(&client.forget(request)?)
}

/// Runs `zcie health` and prints a short table.
pub fn run_health(client: &Client) -> anyhow::Result<()> {
    let report = client.health()?;
    println!("status        {}", report.status);
    println!("version       {}", report.version);
    println!("uptime_s      {}", report.uptime_s);
    if let Some(marks) = report.watermarks {
        println!("canonical_seq {}", marks.canonical_seq);
        println!("graph_seq     {}", marks.graph_seq);
        println!("search_seq    {}", marks.search_seq);
    }
    if let Some(queue) = report.queue {
        println!("queue         interactive={} background={} dead_letter={}", queue.interactive, queue.background, queue.dead_letter);
    }
    if let Some(bytes) = report.storage_bytes {
        println!("storage_bytes {bytes}");
    }
    Ok(())
}

/// Parses a snake_case enum value with serde.
fn parse_enum<T: serde::de::DeserializeOwned>(value: &str, name: &str) -> anyhow::Result<T> {
    serde_json::from_value(serde_json::Value::String(value.to_string()))
        .with_context(|| format!("invalid {name}: {value}"))
}
