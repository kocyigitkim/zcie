//! Index section of `zcie usage` (ADR-023): the daemon state, the index of each project, and the
//! reads (query, expand) and writes (capture, job) of the activity log of the daemon
//! (`<home>/stats/activity.jsonl`). The log is read incrementally: the cache keeps the byte offset
//! and the totals of each project and day.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::Path;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Upper bounds (ms) of the latency buckets; the last bucket has no bound.
const BUCKETS: [f64; 12] = [1.0, 2.0, 5.0, 10.0, 20.0, 50.0, 100.0, 200.0, 500.0, 1000.0, 2000.0, 5000.0];
/// Operations that the report lists, newest first.
const RECENT: usize = 200;
/// Version of the cache format.
const CACHE_VERSION: u32 = 1;
/// Age at which the daemon state is asked again.
const STATUS_TTL: Duration = Duration::from_secs(30);

/// Reads of one project.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Reads {
    pub queries: u64,
    pub expands: u64,
    /// Queries with no result.
    pub empty: u64,
    /// Queries answered from the L0 cache.
    pub cached: u64,
    /// Tokens that the reads returned (estimate of the daemon).
    pub tokens: u64,
    pub results: u64,
    pub ms_total: f64,
    /// Count for each latency bucket (`BUCKETS`, then one more).
    pub buckets: Vec<u64>,
}

/// Writes of one project.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Writes {
    pub captures: u64,
    pub capture_bytes: u64,
    pub duplicates: u64,
    pub events: BTreeMap<String, u64>,
    pub jobs: u64,
    pub jobs_failed: u64,
    pub job_kinds: BTreeMap<String, u64>,
    /// Store keys written by jobs.
    pub keys: u64,
    pub search_ops: u64,
    pub job_ms: f64,
}

/// Reads and writes of one project and day.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Activity {
    pub reads: Reads,
    pub writes: Writes,
    pub last: String,
}

/// The daemon state.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Daemon {
    pub version: String,
    pub uptime_s: u64,
    pub storage_bytes: u64,
    pub canonical_seq: u64,
    pub graph_seq: u64,
    pub search_seq: u64,
    pub queue_interactive: u64,
    pub queue_background: u64,
    pub dead_letter: u64,
}

/// The index of one project.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct IndexProject {
    pub project_id: String,
    pub name: String,
    pub roots: Vec<String>,
    pub files: u64,
    pub symbols: u64,
    pub commits: u64,
    pub memories: u64,
    pub pending_jobs: u64,
    /// Files are indexed and watched (`zcie project add`); else memory only.
    pub indexed: bool,
    pub activity: Activity,
    /// Median and 95th percentile read latency (upper bucket bound, ms).
    pub read_ms_p50: Option<f64>,
    pub read_ms_p95: Option<f64>,
}

/// The index section of the report.
#[derive(Debug, Clone, Default, Serialize)]
pub struct IndexReport {
    /// None when the daemon does not answer.
    pub daemon: Option<Daemon>,
    pub daemon_error: Option<String>,
    pub projects: Vec<IndexProject>,
    /// The last operations, newest first (lines of the activity log).
    pub recent: Vec<Value>,
    pub log: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Cache {
    version: u32,
    offset: u64,
    /// Project ID → day → activity.
    days: BTreeMap<String, BTreeMap<String, Activity>>,
    recent: Vec<Value>,
    status_at: u64,
    daemon: Option<Daemon>,
    #[serde(default)]
    daemon_error: Option<String>,
    status: Vec<IndexProject>,
}

/// Builds the index section: the activity since `since` ("YYYY-MM-DD") and the daemon state.
pub fn build(home: &Path, client: &zcie_sdk::Client, since: &str) -> IndexReport {
    let log = home.join("stats").join("activity.jsonl");
    let cache_path = home.join("run").join("usage-index.json");
    let mut cache: Cache = std::fs::read(&cache_path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
    if cache.version != CACHE_VERSION {
        cache = Cache { version: CACHE_VERSION, ..Cache::default() };
    }
    let mut changed = read_log(&log, &mut cache);
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
    if now.saturating_sub(cache.status_at) >= STATUS_TTL.as_secs() {
        match daemon_state(client) {
            Ok((daemon, status)) => (cache.daemon, cache.status, cache.daemon_error) = (Some(daemon), status, None),
            Err(e) => (cache.daemon, cache.daemon_error) = (None, Some(e.to_string())),
        }
        cache.status_at = now;
        changed = true;
    }
    if changed && let Ok(bytes) = serde_json::to_vec(&cache) {
        super::scan::write_atomic(&cache_path, &bytes);
    }
    report(cache, since, &log)
}

/// Reads the new complete lines of the log. A shorter file (moved to `activity.1.jsonl`) is read
/// from the start: its earlier lines are already counted.
fn read_log(path: &Path, cache: &mut Cache) -> bool {
    let len = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    if len < cache.offset {
        cache.offset = 0;
    }
    if len == cache.offset {
        return false;
    }
    let Ok(mut file) = std::fs::File::open(path) else { return false };
    if file.seek(SeekFrom::Start(cache.offset)).is_err() {
        return false;
    }
    let mut reader = BufReader::new(file);
    let mut line = Vec::new();
    while let Ok(read) = reader.read_until(b'\n', &mut line) {
        if read == 0 || line.last() != Some(&b'\n') {
            break;
        }
        cache.offset += read as u64;
        if let Ok(value) = serde_json::from_slice::<Value>(&line) {
            add(cache, value);
        }
        line.clear();
    }
    if cache.recent.len() > RECENT {
        cache.recent.drain(..cache.recent.len() - RECENT);
    }
    true
}

/// Adds one log line to the totals of its project and day.
fn add(cache: &mut Cache, value: Value) {
    let text = |key: &str| value[key].as_str().unwrap_or_default().to_string();
    let number = |key: &str| value[key].as_u64().unwrap_or(0);
    let (ws, ts) = (text("ws"), text("ts"));
    let day = ts.get(..10).unwrap_or("unknown").to_string();
    let entry = cache.days.entry(ws).or_default().entry(day).or_default();
    entry.last.clone_from(&ts);
    let ms = value["ms"].as_f64().unwrap_or(0.0);
    match text("op").as_str() {
        "query" | "expand" => {
            let reads = &mut entry.reads;
            if text("op") == "query" {
                reads.queries += 1;
                reads.empty += u64::from(number("results") == 0);
                reads.cached += u64::from(value["cached"].as_bool() == Some(true));
                reads.results += number("results");
            } else {
                reads.expands += 1;
                reads.results += number("items");
            }
            reads.tokens += number("tokens");
            reads.ms_total += ms;
            reads.buckets.resize(BUCKETS.len() + 1, 0);
            reads.buckets[BUCKETS.iter().position(|b| ms <= *b).unwrap_or(BUCKETS.len())] += 1;
        }
        "capture" => {
            let writes = &mut entry.writes;
            writes.captures += 1;
            writes.capture_bytes += number("bytes");
            writes.duplicates += u64::from(value["duplicate"].as_bool() == Some(true));
            *writes.events.entry(text("event_type")).or_default() += 1;
        }
        "job" => {
            let writes = &mut entry.writes;
            writes.jobs += 1;
            writes.jobs_failed += u64::from(value["ok"].as_bool() == Some(false));
            *writes.job_kinds.entry(text("kind")).or_default() += 1;
            writes.keys += number("keys");
            writes.search_ops += number("search_ops");
            writes.job_ms += ms;
        }
        _ => {}
    }
    cache.recent.push(value);
}

/// Asks the daemon for its health, its projects and the index state of each project.
fn daemon_state(client: &zcie_sdk::Client) -> anyhow::Result<(Daemon, Vec<IndexProject>)> {
    let health = client.health()?;
    let (marks, queue) = (health.watermarks.unwrap_or_default(), health.queue.unwrap_or_default());
    let daemon = Daemon {
        version: health.version,
        uptime_s: health.uptime_s,
        storage_bytes: health.storage_bytes.unwrap_or(0),
        canonical_seq: marks.canonical_seq,
        graph_seq: marks.graph_seq,
        search_seq: marks.search_seq,
        queue_interactive: queue.interactive,
        queue_background: queue.background,
        dead_letter: queue.dead_letter,
    };
    let mut projects = Vec::new();
    for info in client.list_projects()?.projects {
        let Ok(status) = client.project_status(&info.project_id) else { continue };
        projects.push(IndexProject {
            project_id: info.project_id,
            name: info.name,
            roots: info.roots,
            files: status.files,
            symbols: status.symbols,
            commits: status.commits,
            memories: status.memories,
            pending_jobs: status.pending_jobs,
            indexed: status.indexed,
            ..IndexProject::default()
        });
    }
    Ok((daemon, projects))
}

/// The report: the projects of the daemon with their activity in the window, the active first.
fn report(cache: Cache, since: &str, log: &Path) -> IndexReport {
    let mut projects = cache.status;
    for (ws, days) in &cache.days {
        if !projects.iter().any(|p| &p.project_id == ws) && !ws.is_empty() {
            projects.push(IndexProject { project_id: ws.clone(), name: ws.clone(), ..IndexProject::default() });
        }
        let Some(project) = projects.iter_mut().find(|p| &p.project_id == ws) else { continue };
        for activity in days.iter().filter(|(day, _)| day.as_str() >= since).map(|(_, a)| a) {
            merge(&mut project.activity, activity);
        }
        let buckets = &project.activity.reads.buckets;
        project.read_ms_p50 = percentile(buckets, 0.50);
        project.read_ms_p95 = percentile(buckets, 0.95);
    }
    projects.sort_by(|a, b| b.activity.last.cmp(&a.activity.last).then(b.files.cmp(&a.files)));
    let recent = cache.recent.into_iter().rev().filter(|v| v["ts"].as_str().unwrap_or_default() >= since).collect();
    IndexReport { daemon: cache.daemon, daemon_error: cache.daemon_error, projects, recent, log: log.display().to_string() }
}

fn merge(total: &mut Activity, day: &Activity) {
    let (r, d) = (&mut total.reads, &day.reads);
    r.queries += d.queries;
    r.expands += d.expands;
    r.empty += d.empty;
    r.cached += d.cached;
    r.tokens += d.tokens;
    r.results += d.results;
    r.ms_total += d.ms_total;
    r.buckets.resize(BUCKETS.len() + 1, 0);
    for (own, add) in r.buckets.iter_mut().zip(&d.buckets) {
        *own += add;
    }
    let (w, d) = (&mut total.writes, &day.writes);
    w.captures += d.captures;
    w.capture_bytes += d.capture_bytes;
    w.duplicates += d.duplicates;
    w.jobs += d.jobs;
    w.jobs_failed += d.jobs_failed;
    w.keys += d.keys;
    w.search_ops += d.search_ops;
    w.job_ms += d.job_ms;
    for (event, count) in &d.events {
        *w.events.entry(event.clone()).or_default() += count;
    }
    for (kind, count) in &d.job_kinds {
        *w.job_kinds.entry(kind.clone()).or_default() += count;
    }
    if day.last > total.last {
        total.last.clone_from(&day.last);
    }
}

/// The upper bound of the bucket that holds the `share` percentile; none with no read.
pub fn percentile(buckets: &[u64], share: f64) -> Option<f64> {
    let total: u64 = buckets.iter().sum();
    if total == 0 {
        return None;
    }
    let target = (total as f64 * share).ceil() as u64;
    let mut seen = 0;
    for (i, count) in buckets.iter().enumerate() {
        seen += count;
        if seen >= target {
            return Some(BUCKETS.get(i).copied().unwrap_or(f64::INFINITY));
        }
    }
    None
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;

    /// TS-23-02: the log is read incrementally and summed by project and day; a moved log
    /// keeps its earlier totals.
    #[test]
    fn ts_23_02_the_activity_log_gives_reads_and_writes_of_each_project() {
        let dir = tempfile::tempdir().expect("tmp");
        let path = dir.path().join("activity.jsonl");
        let lines = [
            r#"{"ts":"2026-10-19T08:00:00Z","op":"query","ws":"ws_a","ms":3.0,"results":2,"tokens":40,"cached":false}"#,
            r#"{"ts":"2026-10-19T08:00:01Z","op":"query","ws":"ws_a","ms":80.0,"results":0,"tokens":10,"cached":true}"#,
            r#"{"ts":"2026-10-19T08:00:02Z","op":"expand","ws":"ws_a","ms":1.0,"items":3,"tokens":90}"#,
            r#"{"ts":"2026-10-19T08:00:03Z","op":"capture","ws":"ws_a","ms":0.5,"event_type":"user.message","bytes":120,"duplicate":false}"#,
            r#"{"ts":"2026-10-19T08:00:04Z","op":"job","ws":"ws_a","ms":12.0,"kind":"IndexFile","ok":true,"keys":7,"search_ops":2}"#,
            r#"{"ts":"2026-10-19T08:00:05Z","op":"job","ws":"ws_b","ms":1.0,"kind":"ScanRepository","ok":false}"#,
        ];
        std::fs::write(&path, lines[..4].join("\n") + "\n" + &lines[4][..10]).expect("write");
        let mut cache = Cache::default();
        assert!(read_log(&path, &mut cache));
        std::fs::write(&path, lines.join("\n") + "\n").expect("write");
        assert!(read_log(&path, &mut cache), "the completed line and the next one");
        assert!(!read_log(&path, &mut cache), "no new line");
        let report = report(cache, "2026-10-01", &path);
        let a = report.projects.iter().find(|p| p.project_id == "ws_a").expect("ws_a");
        let (r, w) = (&a.activity.reads, &a.activity.writes);
        assert_eq!((r.queries, r.expands, r.empty, r.cached, r.tokens, r.results), (2, 1, 1, 1, 140, 5));
        assert_eq!((w.captures, w.capture_bytes, w.jobs, w.keys, w.search_ops), (1, 120, 1, 7, 2));
        assert_eq!((a.read_ms_p50, a.read_ms_p95), (Some(5.0), Some(100.0)), "buckets 1, 5 and 100 ms");
        let b = report.projects.iter().find(|p| p.project_id == "ws_b").expect("ws_b");
        assert_eq!((b.activity.writes.jobs, b.activity.writes.jobs_failed), (1, 1));
        assert_eq!(report.recent.len(), 6);
        assert_eq!(report.recent[0]["op"], "job", "newest first");
    }
}
