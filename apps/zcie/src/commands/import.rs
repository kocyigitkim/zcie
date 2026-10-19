//! `zcie import claude`: reads Claude Code transcripts (`~/.claude/projects/*/*.jsonl`) and captures
//! each message that the user wrote as a `user.message` event of the project of its folder, with its
//! original time. The memory extraction then knows the ideas, decisions and rules of all earlier
//! conversations (ADR-013). The import is idempotent: the key is the message UUID.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::Context;
use clap::Args;
use serde_json::Value;
use zcie_sdk::Client;
use zcie_types::EventSource;
use zcie_types::api::CaptureRequest;
use zcie_types::event::EVENT_SCHEMA;

use super::project::find_project_id;

/// Key suffix of answer records in the format "Decision: … (question: …)" (version 2).
const ANSWER_KEY_SUFFIX: &str = ":answers-2";
/// Text starts of messages that the user did not write (tool and command output, notifications).
pub(crate) const NOT_USER_TEXT: [&str; 7] = ["<task-notification>", "<local-command", "<command-name>", "<command-message>", "Caveat:", "[Request interrupted", "<bash-"];

/// Options of `zcie import claude`.
#[derive(Debug, Args)]
pub struct ImportArgs {
    /// Transcript folder. Default: ~/.claude/projects.
    #[arg(long)]
    pub dir: Option<PathBuf>,
    /// Also import sessions that a program started (`claude -p`, entrypoint sdk-cli), for example benchmarks.
    #[arg(long)]
    pub include_automated: bool,
    /// Count only; capture nothing and make no project.
    #[arg(long)]
    pub dry_run: bool,
    /// Import only folders that already have a project (`.claude/zcie_id`). Default: make the project
    /// of each existing folder in the store (no file is written in the folder; its first Claude Code
    /// session finds the project by its root and writes `.claude/zcie_id`).
    #[arg(long)]
    pub existing_only: bool,
}

/// One user message of a transcript.
struct UserMessage {
    uuid: String,
    key: String,
    session: String,
    cwd: String,
    timestamp: Option<String>,
    text: String,
    /// `user.message`, or `assistant.message` for the text of an assistant answer.
    event_type: &'static str,
}

/// Longest assistant answer that the import stores (the first part; the rest is mostly code).
const MAX_ANSWER_CHARS: usize = 30_000;

/// Tries of one request when the connection fails (a daemon that just started, a short busy time).
const REQUEST_TRIES: u32 = 5;

/// Runs a request again after a connection error, with 1, 2, 4 and 8 s pauses. API errors
/// (a refused request) are returned at once.
fn with_retry<T>(mut request: impl FnMut() -> zcie_sdk::Result<T>) -> zcie_sdk::Result<T> {
    let mut tries = 1;
    loop {
        match request() {
            Err(zcie_sdk::Error::Http(_)) if tries < REQUEST_TRIES => {
                std::thread::sleep(std::time::Duration::from_secs(1 << (tries - 1)));
                tries += 1;
            }
            result => return result,
        }
    }
}

/// Captures one message. Returns false for an assistant answer that the daemon refuses
/// (`capture.assistant_messages = false`).
fn store(client: &Client, project: &str, message: &UserMessage) -> anyhow::Result<bool> {
    match with_retry(|| client.capture(capture_request(project, message))) {
        Ok(_) => Ok(true),
        Err(zcie_sdk::Error::Api { status: 403, .. }) if message.event_type == "assistant.message" => Ok(false),
        Err(error) => Err(error.into()),
    }
}

/// Runs `zcie import claude`.
pub fn run(args: ImportArgs, client: &Client) -> anyhow::Result<()> {
    let dir = match args.dir.clone() {
        Some(dir) => dir,
        None => PathBuf::from(std::env::var("HOME").context("HOME is not set")?).join(".claude/projects"),
    };
    let mut per_project: BTreeMap<String, usize> = BTreeMap::new();
    let mut skipped = 0usize;
    let mut folders: BTreeMap<String, Option<String>> = BTreeMap::new();
    for file in transcripts(&dir)? {
        for message in user_messages(&file, args.include_automated) {
            if !folders.contains_key(&message.cwd) {
                let project = project_of(client, &message.cwd, &args, &mut per_project)?;
                folders.insert(message.cwd.clone(), project);
            }
            match folders[&message.cwd].clone() {
                Some(project) => {
                    if args.dry_run || store(client, &project, &message)? {
                        *per_project.entry(project).or_default() += 1;
                    }
                }
                None => skipped += 1,
            }
        }
    }
    for (project, count) in &per_project {
        let name = client.project(project).map(|p| p.name).unwrap_or_else(|_| project.clone());
        println!("{:<30} {count} messages{}", name, if args.dry_run { " (dry run)" } else { "" });
    }
    println!("skipped {skipped} messages of folders with no project (.claude/zcie_id)");
    Ok(())
}

/// Returns the project of a folder: its `.claude/zcie_id`, or (unless `existing_only`) the project of
/// its Git root (or the folder), made in the store if needed. Temporary folders, the home folder
/// and missing folders have no project.
fn project_of(client: &Client, cwd: &str, args: &ImportArgs, known: &mut BTreeMap<String, usize>) -> anyhow::Result<Option<String>> {
    let folder = Path::new(cwd);
    if let Some((_, id)) = find_project_id(folder) {
        return Ok(Some(id));
    }
    let home = std::env::var("HOME").unwrap_or_default();
    let temporary = ["/tmp", "/private/tmp", "/var/folders", "/private/var"].iter().any(|t| cwd.starts_with(t));
    if args.existing_only || temporary || !folder.is_dir() || cwd == home || cwd == "/" {
        return Ok(None);
    }
    let root = git_root(folder).unwrap_or_else(|| folder.to_path_buf());
    if is_container(&root) {
        return Ok(None);
    }
    let name = root.file_name().map(|n| n.to_string_lossy().to_string());
    if args.dry_run {
        let label = format!("(new) {}", root.display());
        known.entry(label.clone()).or_default();
        return Ok(Some(label));
    }
    let request = zcie_types::api::EnsureProjectRequest { project_id: None, name, root: Some(root.display().to_string()), index: false };
    Ok(Some(with_retry(|| client.ensure_project(&request))?.project_id))
}

/// Returns the root of the main worktree of the Git repository of a folder, so that a linked
/// worktree (for example `.claude/worktrees/<name>`) belongs to the project of its repository.
pub(crate) fn git_root(folder: &Path) -> Option<PathBuf> {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(folder)
        .args(["rev-parse", "--path-format=absolute", "--git-common-dir"])
        .output()
        .ok()?;
    let common = PathBuf::from(String::from_utf8_lossy(&out.stdout).trim());
    (out.status.success() && common.ends_with(".git")).then(|| common.parent().map(Path::to_path_buf)).flatten()
}

/// True for a folder that is not a Git repository but has a Git repository in a direct subfolder
/// (for example `~/Desktop`): its conversations are not about one project.
fn is_container(root: &Path) -> bool {
    if root.join(".git").exists() {
        return false;
    }
    std::fs::read_dir(root).map(|entries| entries.flatten().any(|e| e.path().join(".git").exists())).unwrap_or(false)
}

fn transcripts(dir: &Path) -> anyhow::Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    for folder in std::fs::read_dir(dir).with_context(|| format!("cannot read {}", dir.display()))?.flatten() {
        let Ok(files) = std::fs::read_dir(folder.path()) else { continue };
        out.extend(files.flatten().map(|f| f.path()).filter(|p| p.extension().is_some_and(|e| e == "jsonl")));
    }
    out.sort();
    Ok(out)
}

/// Returns the messages that the user wrote in one transcript.
fn user_messages(file: &Path, include_automated: bool) -> Vec<UserMessage> {
    let Ok(text) = std::fs::read_to_string(file) else { return Vec::new() };
    let records: Vec<Value> = text.lines().filter_map(|l| serde_json::from_str(l).ok()).collect();
    let automated = records.iter().any(|r| r["entrypoint"].as_str() == Some("sdk-cli"));
    if automated && !include_automated {
        return Vec::new();
    }
    records.iter().filter_map(|r| user_message(r).or_else(|| assistant_message(r))).collect()
}

/// Returns the text parts of an assistant record (not its tool calls), as `assistant.message`.
fn assistant_message(record: &Value) -> Option<UserMessage> {
    if record["type"].as_str() != Some("assistant") || record["isSidechain"].as_bool() == Some(true) {
        return None;
    }
    let parts: Vec<&str> = record["message"]["content"].as_array()?.iter().filter(|p| p["type"] == "text").filter_map(|p| p["text"].as_str()).collect();
    let text = parts.join("\n").trim().to_string();
    let uuid = record["uuid"].as_str()?.to_string();
    let cwd = record["cwd"].as_str()?.to_string();
    (!text.is_empty()).then(|| UserMessage {
        key: format!("claude-transcript:{uuid}"),
        uuid,
        session: record["sessionId"].as_str().unwrap_or("unknown").to_string(),
        cwd,
        timestamp: record["timestamp"].as_str().map(str::to_string),
        text: zcie_types::truncate_chars(&text, MAX_ANSWER_CHARS),
        event_type: "assistant.message",
    })
}

fn user_message(record: &Value) -> Option<UserMessage> {
    let flags = ["isMeta", "isCompactSummary", "isSidechain", "isVisibleInTranscriptOnly"];
    if record["type"].as_str() != Some("user") || flags.iter().any(|f| record[*f].as_bool() == Some(true)) {
        return None;
    }
    let uuid = record["uuid"].as_str()?.to_string();
    let (text, key) = match answers_text(&record["toolUseResult"]) {
        Some(answers) => (answers, format!("claude-transcript:{uuid}{ANSWER_KEY_SUFFIX}")),
        None => (clean_text(&message_text(&record["message"]["content"])?), format!("claude-transcript:{uuid}")),
    };
    if text.is_empty() || text.starts_with("! ") || NOT_USER_TEXT.iter().any(|p| text.starts_with(p)) {
        return None;
    }
    Some(UserMessage {
        uuid,
        key,
        session: record["sessionId"].as_str().unwrap_or("unknown").to_string(),
        cwd: record["cwd"].as_str()?.to_string(),
        timestamp: record["timestamp"].as_str().map(str::to_string),
        text,
        event_type: "user.message",
    })
}

/// Returns the answers that the user chose for questions of the assistant (AskUserQuestion), as
/// "Decision: <answer> (question: <question>)" lines. These are decisions of the user.
fn answers_text(result: &Value) -> Option<String> {
    let answers = result.get("answers")?.as_object()?;
    // "Decision: <answer> (question: <question>)": the answer is a decision of the user, and the
    // text does not start as a question (stored questions are not shown as facts).
    let lines: Vec<String> = answers.iter().filter_map(|(q, a)| Some(format!("Decision: {} (question: {q})", a.as_str()?))).collect();
    (!lines.is_empty()).then(|| lines.join("\n"))
}

/// Returns the text of a message content: a string, or the text parts (no tool results).
pub(crate) fn message_text(content: &Value) -> Option<String> {
    if let Some(text) = content.as_str() {
        return Some(text.to_string());
    }
    let parts = content.as_array()?;
    if parts.iter().any(|p| p["type"].as_str() == Some("tool_result")) {
        return None;
    }
    Some(parts.iter().filter_map(|p| p["text"].as_str()).collect::<Vec<_>>().join("\n"))
}

/// Removes `<system-reminder>` blocks, which the client added to the message.
pub(crate) fn clean_text(text: &str) -> String {
    let mut out = text.to_string();
    while let (Some(start), Some(end)) = (out.find("<system-reminder>"), out.find("</system-reminder>")) {
        if end < start {
            break;
        }
        out.replace_range(start..end + "</system-reminder>".len(), "");
    }
    out.trim().to_string()
}

fn capture_request(project: &str, message: &UserMessage) -> CaptureRequest {
    CaptureRequest {
        schema: EVENT_SCHEMA.into(),
        idempotency_key: message.key.clone(),
        timestamp_utc: message.timestamp.clone(),
        workspace: project.to_string(),
        repository_id: None,
        source: EventSource { app: "claude-code".into(), session_id: message.session.clone(), turn_id: Some(message.uuid.clone()) },
        event_type: message.event_type.into(),
        payload: Some(message.text.clone()),
        git: None,
        classification: None,
    }
}
