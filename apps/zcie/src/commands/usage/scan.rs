//! Session records from Claude Code transcripts (ADR-022, D-083). Each transcript file gives the
//! counters of one session; a file in `<session>/subagents/` adds to its parent session. The cache
//! keeps the byte offset of the last complete line of each file, so a scan reads only new lines.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::commands::import::{NOT_USER_TEXT, clean_text, message_text};

pub use super::session::{Day, Session};

/// Version of the cache format. A cache of another version is ignored and built again.
const CACHE_VERSION: u32 = 3;
/// First words of Bash commands that search files.
const SEARCH_COMMANDS: [&str; 5] = ["grep", "rg", "find", "fd", "ls"];
/// Shortest time between two writes of the cache.
const SAVE_INTERVAL: std::time::Duration = std::time::Duration::from_secs(60);

/// The scan state of one transcript file.
#[derive(Debug, Default, Serialize, Deserialize)]
struct FileState {
    /// Byte offset after the last complete line that was read.
    offset: u64,
    /// `message.id` of the last assistant record (its usage is counted once).
    last_message: Option<String>,
    /// Time of a typed message with no answer yet: the ZCIE capsule of that message comes after it.
    #[serde(default)]
    open_turn: Option<String>,
    /// Counters of the file.
    session: Session,
}

/// The cache of all files.
#[derive(Debug, Default, Serialize, Deserialize)]
struct Cache {
    version: u32,
    files: BTreeMap<String, FileState>,
}

/// The fields of a transcript record that the scan uses; the other fields are skipped.
#[derive(Debug, Deserialize)]
struct Record {
    #[serde(rename = "type", default)]
    kind: String,
    cwd: Option<String>,
    timestamp: Option<String>,
    entrypoint: Option<String>,
    message: Option<Value>,
    attachment: Option<Value>,
    #[serde(rename = "isMeta")]
    meta: Option<bool>,
    #[serde(rename = "isCompactSummary")]
    compact_summary: Option<bool>,
    #[serde(rename = "isSidechain")]
    sidechain: Option<bool>,
    #[serde(rename = "isVisibleInTranscriptOnly")]
    transcript_only: Option<bool>,
}

/// Returns the sessions of all transcripts in `dir`, by session ID. Updates the cache file when a
/// file changed, at most once in `SAVE_INTERVAL` (the menu bar app scans every few seconds; the
/// lines after the saved offsets are read again, which is fast).
pub fn scan(dir: &Path, cache_path: &Path) -> anyhow::Result<BTreeMap<String, Session>> {
    let mut old = load_cache(cache_path);
    let mut cache = Cache { version: CACHE_VERSION, files: BTreeMap::new() };
    let mut sessions: BTreeMap<String, Session> = BTreeMap::new();
    let mut changed = false;
    for (id, path) in transcript_files(dir) {
        let key = path.display().to_string();
        let known = old.files.remove(&key);
        changed |= known.is_none();
        let mut state = known.unwrap_or_default();
        let len = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        if len < state.offset {
            state = FileState::default();
        }
        if len > state.offset {
            read_new_lines(&path, &mut state)?;
            changed = true;
        }
        let entry = sessions.entry(id.clone()).or_default();
        entry.merge(&state.session);
        entry.id = id;
        if path.parent().is_some_and(|p| p.ends_with("subagents")) {
            entry.subagents += 1;
            entry.subagent_cost += state.session.weighted().round() as u64;
        }
        cache.files.insert(key, state);
    }
    // A removed transcript changes the cache too.
    if (changed || !old.files.is_empty()) && save_due(cache_path) {
        save_cache(cache_path, &cache);
    }
    Ok(sessions)
}

/// True when the cache file is missing or older than `SAVE_INTERVAL`.
fn save_due(path: &Path) -> bool {
    let modified = std::fs::metadata(path).and_then(|m| m.modified());
    modified.map_or(true, |time| time.elapsed().map_or(true, |age| age >= SAVE_INTERVAL))
}

fn load_cache(path: &Path) -> Cache {
    let cache: Cache = std::fs::read(path).ok().and_then(|bytes| serde_json::from_slice(&bytes).ok()).unwrap_or_default();
    if cache.version == CACHE_VERSION { cache } else { Cache::default() }
}

/// Writes the cache. A failure only makes the next scan slower.
fn save_cache(path: &Path, cache: &Cache) {
    if let Ok(bytes) = serde_json::to_vec(cache) {
        write_atomic(path, &bytes);
    }
}

/// Writes a file through a temporary file and a rename. Errors are ignored: the files of
/// `zcie usage` are caches.
pub fn write_atomic(path: &Path, bytes: &[u8]) {
    let temporary = path.with_extension("tmp");
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if std::fs::write(&temporary, bytes).is_ok() {
        let _ = std::fs::rename(&temporary, path);
    }
}

/// Lists the transcripts with their session ID: `<folder>/<session>.jsonl` and
/// `<folder>/<session>/subagents/*.jsonl`.
fn transcript_files(dir: &Path) -> Vec<(String, PathBuf)> {
    let mut out = Vec::new();
    let jsonl = |p: &Path| p.extension().is_some_and(|e| e == "jsonl");
    for folder in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        for entry in std::fs::read_dir(folder.path()).into_iter().flatten().flatten() {
            let path = entry.path();
            let name = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
            if jsonl(&path) {
                out.push((name, path));
            } else if path.is_dir() {
                let subagents = std::fs::read_dir(path.join("subagents")).into_iter().flatten().flatten();
                out.extend(subagents.map(|e| e.path()).filter(|p| jsonl(p)).map(|p| (name.clone(), p)));
            }
        }
    }
    // The main file of a session first: a subagent file is merged with its start time known.
    out.sort_by_key(|(id, path)| (id.clone(), path.components().count(), path.clone()));
    out
}

/// Reads the complete lines after the offset of a file. A last line with no newline is left for
/// the next scan (the client is still writing it).
fn read_new_lines(path: &Path, state: &mut FileState) -> anyhow::Result<()> {
    let mut file = std::fs::File::open(path)?;
    file.seek(SeekFrom::Start(state.offset))?;
    let mut reader = BufReader::new(file);
    let mut line = Vec::new();
    loop {
        line.clear();
        let read = reader.read_until(b'\n', &mut line)?;
        if read == 0 || line.last() != Some(&b'\n') {
            return Ok(());
        }
        state.offset += read as u64;
        if let Ok(record) = serde_json::from_slice::<Record>(&line) {
            apply(&record, state);
        }
    }
}

/// Adds one record to the counters.
fn apply(record: &Record, state: &mut FileState) {
    let session = &mut state.session;
    if let Some(cwd) = record.cwd.as_deref().filter(|_| session.cwd.is_empty()) {
        session.cwd = cwd.to_string();
    }
    session.automated |= record.entrypoint.as_deref() == Some("sdk-cli");
    let time = record.timestamp.clone().unwrap_or_default();
    match record.kind.as_str() {
        "assistant" => assistant(record, &time, state),
        "user" if is_typed(record) => {
            session.stamp(&time);
            session.turns += 1;
            day(session, &time).turns += 1;
            state.open_turn = Some(time);
        }
        "attachment" => attachment(record, &time, state),
        _ => {}
    }
}

/// The day entry of a time stamp.
fn day<'a>(session: &'a mut Session, time: &str) -> &'a mut Day {
    session.days.entry(time.get(..10).unwrap_or("unknown").to_string()).or_default()
}

/// Counts the usage of an assistant message once, and its tool calls.
fn assistant(record: &Record, time: &str, state: &mut FileState) {
    let Some(message) = &record.message else { return };
    state.open_turn = None;
    for part in message["content"].as_array().into_iter().flatten().filter(|p| p["type"] == "tool_use") {
        let name = part["name"].as_str().unwrap_or_default();
        if name.starts_with("mcp__zcie__") {
            start_zcie(state, time);
        }
        tool_call(name, &part["input"], &mut state.session);
    }
    let id = message["id"].as_str().map(str::to_string);
    if id.is_some() && id == state.last_message {
        return;
    }
    state.last_message = id;
    let usage = &message["usage"];
    let count = |field: &str| usage[field].as_u64().unwrap_or(0);
    let session = &mut state.session;
    session.stamp(time);
    let (input, write, read, output) = (count("input_tokens"), count("cache_creation_input_tokens"), count("cache_read_input_tokens"), count("output_tokens"));
    session.input += input;
    session.cache_write += write;
    session.cache_write_1h += usage["cache_creation"]["ephemeral_1h_input_tokens"].as_u64().unwrap_or(0);
    session.cache_read += read;
    session.output += output;
    day(session, time).tokens += input + write + read + output;
}

fn tool_call(name: &str, input: &Value, session: &mut Session) {
    match name {
        "Read" => session.reads += 1,
        "Grep" | "Glob" => session.searches += 1,
        "Bash" => {
            let first = input["command"].as_str().unwrap_or_default().split_whitespace().next().unwrap_or_default();
            if SEARCH_COMMANDS.contains(&first) {
                session.searches += 1;
            }
        }
        _ => {}
    }
}

/// A hook output of ZCIE marks the session; a context capsule is counted with its size.
fn attachment(record: &Record, time: &str, state: &mut FileState) {
    let Some(attachment) = &record.attachment else { return };
    let content = attachment["content"].as_str().unwrap_or_default();
    let command = attachment["command"].as_str().unwrap_or_default();
    if content.trim().is_empty() || !(command.contains(".zcie/hooks/") || content.starts_with("ZCIE ")) {
        return;
    }
    start_zcie(state, time);
    let session = &mut state.session;
    if content.starts_with("ZCIE context") {
        session.capsules += 1;
        session.capsule_chars += content.chars().count() as u64;
    }
}

/// Marks the session as a ZCIE session at its first ZCIE output. The counters until then become
/// the part before; a typed message with no answer yet (its capsule is this output) moves along.
fn start_zcie(state: &mut FileState, time: &str) {
    let session = &mut state.session;
    if session.zcie {
        return;
    }
    let mut prior = std::mem::take(session);
    session.cwd.clone_from(&prior.cwd);
    session.automated = prior.automated;
    session.zcie = true;
    session.zcie_since = time.to_string();
    if let Some(turn) = state.open_turn.take() {
        prior.turns = prior.turns.saturating_sub(1);
        day(&mut prior, &turn).turns = day(&mut prior, &turn).turns.saturating_sub(1);
        session.stamp(&turn);
        session.turns += 1;
        day(session, &turn).turns += 1;
    }
    if prior.turns > 0 || prior.tokens() > 0 {
        session.before = Some(Box::new(prior));
    }
}

/// True for a message that the user wrote (the rules of `zcie import claude`).
fn is_typed(record: &Record) -> bool {
    let flags = [record.meta, record.compact_summary, record.sidechain, record.transcript_only];
    if flags.contains(&Some(true)) {
        return false;
    }
    let Some(text) = record.message.as_ref().and_then(|m| message_text(&m["content"])) else { return false };
    let text = clean_text(&text);
    !text.is_empty() && !text.starts_with("! ") && !NOT_USER_TEXT.iter().any(|p| text.starts_with(p))
}
