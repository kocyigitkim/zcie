//! The sessions of a project, its outliers and the confidence of its saving (ADR-024). A median
//! of two or three sessions is set by one large task: the report lists each session, marks the
//! outliers and leaves them out of the saving, and says how many sessions the saving uses.

use serde::Serialize;

use super::report::weighted_median;
use super::session::{Session, WEIGHTS};

/// Input tokens of one image: a rough estimate (an image's size varies).
const IMAGE_TOKENS: u64 = 3000;
/// An average context from this many tokens is long: its cache reads are a main cause.
const LONG_CONTEXT: u64 = 150_000;

/// A session costs this many times the turn-weighted median cost per turn of its group (with or with no ZCIE) in its project, or of the whole project when its group has fewer than `OUTLIER_MIN_SESSIONS` sessions, or more: an outlier.
pub const OUTLIER_FACTOR: f64 = 3.0;
/// Sessions that a group (with or with no ZCIE) of a project needs to use its own median; with fewer, the project median is used. The project needs this many sessions to have outliers at all.
pub const OUTLIER_MIN_SESSIONS: usize = 3;

/// One session in the report.
#[derive(Debug, Clone, Default, Serialize)]
pub struct SessionRow {
    /// The first characters of the session ID ("1fa50·pre" for the part before ZCIE).
    pub id: String,
    pub start: String,
    pub zcie: bool,
    pub turns: u64,
    /// Cost in input-token units, subagents included.
    pub cost: u64,
    pub cost_per_turn: f64,
    pub subagents: u64,
    /// Share of the cost that the subagents used (0..1).
    pub subagent_share: f64,
    /// Images in the main thread, and their estimated input tokens (`IMAGE_TOKENS` each).
    pub images: u64,
    pub image_tokens_est: u64,
    /// Characters of the output of Bash file reads (cat, sed, grep ...).
    pub bash_read_chars: u64,
    /// Average input + cache tokens of a main-thread call.
    pub avg_context: u64,
    /// Share of the weighted cost that output tokens use (0..1).
    pub output_share: f64,
    /// The largest cost driver: "subagents", "long context", "images", "bash reads", "output" or "-".
    pub cause: &'static str,
    /// Left out of the saving: its cost per turn is `OUTLIER_FACTOR` times the turn-weighted median of its group (with or with no ZCIE), or of the project when the group is small, or more.
    pub outlier: bool,
}

/// The largest cost driver of a session, in input-token units: the subagent cost, the cache reads
/// of a long context, the images, the Bash reads (4 characters per token) and the output.
pub fn main_cause(session: &Session) -> &'static str {
    let long = session.context_calls > 0 && session.context_sum / session.context_calls >= LONG_CONTEXT;
    let causes = [
        ("subagents", session.subagent_cost as f64),
        ("long context", if long { session.context_sum as f64 * WEIGHTS.2 } else { 0.0 }),
        ("images", (session.images * IMAGE_TOKENS) as f64),
        ("bash reads", session.bash_read_chars as f64 / 4.0),
        ("output", session.output as f64 * WEIGHTS.3),
    ];
    causes.iter().filter(|c| c.1 > 0.0).max_by(|a, b| a.1.total_cmp(&b.1)).map_or("-", |c| c.0)
}

fn per_turn(session: &Session) -> f64 {
    session.weighted() / session.turns.max(1) as f64
}

/// The turn-weighted median cost per turn of some sessions, 0 without sessions.
fn median<'a>(sessions: impl Iterator<Item = &'a Session>) -> f64 {
    weighted_median(sessions.map(|s| (per_turn(s), s.turns.max(1) as f64)).collect()).unwrap_or(0.0)
}

/// The rows of the sessions of a project, newest first, and the IDs of the outliers.
pub fn rows(zcie: &[Session], plain: &[Session]) -> (Vec<SessionRow>, Vec<String>) {
    let (middle_zcie, middle_plain) = (median(zcie.iter()), median(plain.iter()));
    let all: Vec<&Session> = zcie.iter().chain(plain).collect();
    let middle_all = median(zcie.iter().chain(plain));
    let outlier = |s: &Session| {
        let (group_len, own) = if s.zcie { (zcie.len(), middle_zcie) } else { (plain.len(), middle_plain) };
        let middle = if group_len >= OUTLIER_MIN_SESSIONS { own } else { middle_all };
        all.len() >= OUTLIER_MIN_SESSIONS && middle > 0.0 && per_turn(s) >= OUTLIER_FACTOR * middle
    };
    let mut rows: Vec<SessionRow> = all
        .iter()
        .map(|s| {
            let cost = s.weighted();
            SessionRow {
                id: s.id.chars().take(9).collect::<String>().trim_end_matches('-').to_string(),
                start: s.first.clone(),
                zcie: s.zcie,
                turns: s.turns,
                cost: cost.round() as u64,
                cost_per_turn: per_turn(s),
                subagents: s.subagents,
                subagent_share: if cost > 0.0 { (s.subagent_cost as f64 / cost).min(1.0) } else { 0.0 },
                images: s.images,
                image_tokens_est: s.images * IMAGE_TOKENS,
                bash_read_chars: s.bash_read_chars,
                avg_context: s.context_sum.checked_div(s.context_calls).unwrap_or(0),
                output_share: if cost > 0.0 { (s.output as f64 * WEIGHTS.3 / cost).min(1.0) } else { 0.0 },
                cause: main_cause(s),
                outlier: outlier(s),
            }
        })
        .collect();
    rows.sort_by(|a, b| b.start.cmp(&a.start));
    let ids = all.iter().filter(|s| outlier(s)).map(|s| s.id.clone()).collect();
    (rows, ids)
}

/// `low` with fewer than 3 sessions in a group or a baseline of all projects, `medium` with fewer
/// than 10, else `high`.
pub fn confidence(zcie: u64, plain: u64, rough: bool) -> &'static str {
    match zcie.min(plain) {
        _ if rough => "low",
        0..=2 => "low",
        3..=9 => "medium",
        _ => "high",
    }
}
