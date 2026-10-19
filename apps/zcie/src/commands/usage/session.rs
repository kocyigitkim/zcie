//! The counters of one Claude Code session (ADR-022) and their cost weight.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Price of each token kind relative to one input token (all current Claude models): a cache
/// write of 5 minutes 1.25, of 1 hour 2, a cache read 0.1, an output token 5.
pub const WEIGHTS: (f64, f64, f64, f64) = (1.25, 2.0, 0.1, 5.0);

/// Tokens and turns of one UTC day.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Day {
    /// Input + cache write + cache read + output tokens.
    pub tokens: u64,
    /// Messages that the user wrote.
    pub turns: u64,
}

/// Counters of one session.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Session {
    /// Session ID (the transcript file name).
    #[serde(default)]
    pub id: String,
    /// Folder of the session.
    pub cwd: String,
    /// A program started the session (`entrypoint` `sdk-cli`).
    pub automated: bool,
    /// A ZCIE hook gave output, or a ZCIE MCP tool was called.
    pub zcie: bool,
    /// First time stamp (RFC 3339, UTC).
    pub first: String,
    /// Last time stamp.
    pub last: String,
    /// Messages that the user wrote.
    pub turns: u64,
    /// Input tokens.
    pub input: u64,
    /// Cache write tokens.
    pub cache_write: u64,
    /// The part of `cache_write` with a 1-hour lifetime.
    #[serde(default)]
    pub cache_write_1h: u64,
    /// Cache read tokens.
    pub cache_read: u64,
    /// Output tokens.
    pub output: u64,
    /// Read tool calls.
    pub reads: u64,
    /// Grep, Glob and search Bash commands.
    pub searches: u64,
    /// ZCIE context capsules.
    pub capsules: u64,
    /// Characters of the capsules.
    pub capsule_chars: u64,
    /// Tokens and turns of each day ("YYYY-MM-DD").
    pub days: BTreeMap<String, Day>,
    /// Subagent transcripts of the session, and their cost in input-token units (in the counters).
    #[serde(default)]
    pub subagents: u64,
    #[serde(default)]
    pub subagent_cost: u64,
    /// Time of the first ZCIE output; empty with none.
    #[serde(default)]
    pub zcie_since: String,
    /// The part before the first ZCIE output, when ZCIE joined a running session (a resumed
    /// session). The report counts it as a session with no ZCIE.
    #[serde(default)]
    pub before: Option<Box<Session>>,
}

impl Session {
    /// All tokens: input, cache write, cache read and output.
    pub fn tokens(&self) -> u64 {
        self.input + self.cache_write + self.cache_read + self.output
    }

    /// The cost in input-token units: each token kind with its price weight (`WEIGHTS`).
    pub fn weighted(&self) -> f64 {
        let (write_5m, write_1h, read, output) = WEIGHTS;
        let short = self.cache_write.saturating_sub(self.cache_write_1h);
        self.input as f64 + short as f64 * write_5m + self.cache_write_1h as f64 * write_1h + self.cache_read as f64 * read + self.output as f64 * output
    }

    /// Adds another file of the same session (a subagent file). A subagent file that ended before
    /// ZCIE joined the session adds to the part before.
    pub fn merge(&mut self, other: &Session) {
        let early = !other.zcie && !self.zcie_since.is_empty() && !other.last.is_empty() && other.last < self.zcie_since;
        if early {
            self.before.get_or_insert_with(Box::default).add(other);
            return;
        }
        if let Some(before) = &other.before {
            self.before.get_or_insert_with(Box::default).add(before);
        }
        if self.zcie_since.is_empty() {
            self.zcie_since.clone_from(&other.zcie_since);
        }
        self.add(other);
    }

    /// Adds the counters of another part.
    fn add(&mut self, other: &Session) {
        if self.cwd.is_empty() {
            self.cwd.clone_from(&other.cwd);
        }
        self.automated |= other.automated;
        self.zcie |= other.zcie;
        self.stamp(&other.first);
        self.stamp(&other.last);
        let pairs = [
            (&mut self.turns, other.turns),
            (&mut self.input, other.input),
            (&mut self.cache_write, other.cache_write),
            (&mut self.cache_write_1h, other.cache_write_1h),
            (&mut self.cache_read, other.cache_read),
            (&mut self.output, other.output),
            (&mut self.reads, other.reads),
            (&mut self.searches, other.searches),
            (&mut self.capsules, other.capsules),
            (&mut self.capsule_chars, other.capsule_chars),
        ];
        for (own, add) in pairs {
            *own += add;
        }
        for (date, day) in &other.days {
            let own = self.days.entry(date.clone()).or_default();
            own.tokens += day.tokens;
            own.turns += day.turns;
        }
    }

    /// Extends the time range with one time stamp.
    pub(crate) fn stamp(&mut self, time: &str) {
        if time.is_empty() {
            return;
        }
        if self.first.is_empty() || time < self.first.as_str() {
            self.first = time.to_string();
        }
        if time > self.last.as_str() {
            self.last = time.to_string();
        }
    }
}
