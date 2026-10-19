//! Activity log of the daemon (ADR-023): one JSON line for each index read (query, expand) and
//! each index write (capture, job) in `<home>/stats/activity.jsonl`. `zcie usage` and the menu
//! bar app read it. A full file (`MAX_BYTES`) moves to `activity.1.jsonl`. A failed write is
//! logged once and never stops the operation.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use serde_json::Value;

/// Size at which the log moves to `activity.1.jsonl`.
pub const MAX_BYTES: u64 = 20 * 1024 * 1024;

/// The open log file and its size.
#[derive(Debug)]
struct Open {
    file: File,
    size: u64,
}

/// An append-only JSON lines file.
#[derive(Debug)]
pub struct ActivityLog {
    path: PathBuf,
    open: Mutex<Option<Open>>,
    warned: AtomicBool,
}

impl ActivityLog {
    /// A log at `path`. The file is opened at the first record.
    pub fn new(path: &Path) -> ActivityLog {
        ActivityLog { path: path.to_path_buf(), open: Mutex::new(None), warned: AtomicBool::new(false) }
    }

    /// The path of the log.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Appends one record. Adds the time `ts` (RFC 3339, UTC) when it is missing.
    pub fn record(&self, mut value: Value) {
        if let Some(object) = value.as_object_mut() {
            object.entry("ts").or_insert_with(|| Value::String(now_rfc3339()));
        }
        let mut line = value.to_string();
        line.push('\n');
        let mut guard = self.open.lock().unwrap_or_else(|e| e.into_inner());
        if let Err(error) = self.append(&mut guard, line.as_bytes()) {
            *guard = None;
            if !self.warned.swap(true, Ordering::Relaxed) {
                tracing::warn!(%error, path = %self.path.display(), "activity log write failed");
            }
        }
    }

    fn append(&self, open: &mut Option<Open>, bytes: &[u8]) -> std::io::Result<()> {
        if open.as_ref().is_some_and(|o| o.size + bytes.len() as u64 > MAX_BYTES) {
            *open = None;
            std::fs::rename(&self.path, self.path.with_extension("1.jsonl"))?;
        }
        if open.is_none() {
            if let Some(parent) = self.path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let file = OpenOptions::new().create(true).append(true).open(&self.path)?;
            let size = file.metadata()?.len();
            *open = Some(Open { file, size });
        }
        if let Some(o) = open.as_mut() {
            o.file.write_all(bytes)?;
            o.size += bytes.len() as u64;
        }
        Ok(())
    }
}

/// The current time as RFC 3339 UTC with milliseconds ("2026-10-19T08:00:00.123Z").
fn now_rfc3339() -> String {
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
    let (secs, millis) = (now.as_secs(), now.subsec_millis());
    let (days, rest) = (secs / 86_400, secs % 86_400);
    let (y, m, d) = civil_from_days(days as i64);
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}.{millis:03}Z", rest / 3600, rest % 3600 / 60, rest % 60)
}

/// Year, month and day of a day count since 1970-01-01 (H. Hinnant's civil_from_days).
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn records_are_lines_with_a_time_and_the_log_rotates() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(20_745), (2026, 10, 19));
        let dir = std::env::temp_dir().join(format!("zcie-activity-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let log = ActivityLog::new(&dir.join("activity.jsonl"));
        log.record(serde_json::json!({"op": "query"}));
        log.record(serde_json::json!({"op": "job", "ts": "x"}));
        let text = std::fs::read_to_string(log.path()).expect("log");
        let lines: Vec<Value> = text.lines().map(|l| serde_json::from_str(l).expect("json")).collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[0]["ts"].as_str().expect("ts").ends_with('Z'));
        assert_eq!(lines[1]["ts"], "x");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
