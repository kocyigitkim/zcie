//! The sessions of a project, its outliers and the confidence of its saving (ADR-024). A median
//! of two or three sessions is set by one large task: the report lists each session, marks the
//! outliers and leaves them out of the saving, and says how many sessions the saving uses.

use serde::Serialize;

use super::report::median;
use super::session::Session;

/// A session costs this many times the median cost per turn of its project, or more: an outlier.
pub const OUTLIER_FACTOR: f64 = 3.0;
/// Sessions that a project needs before a session can be an outlier.
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
    /// Left out of the saving: its cost per turn is `OUTLIER_FACTOR` times the project median or more.
    pub outlier: bool,
}

fn per_turn(session: &Session) -> f64 {
    session.weighted() / session.turns.max(1) as f64
}

/// The rows of the sessions of a project, newest first, and the IDs of the outliers.
pub fn rows(zcie: &[Session], plain: &[Session]) -> (Vec<SessionRow>, Vec<String>) {
    let all: Vec<&Session> = zcie.iter().chain(plain).collect();
    let middle = median(all.iter().map(|s| per_turn(s)).collect()).unwrap_or(0.0);
    let outlier = |s: &Session| all.len() >= OUTLIER_MIN_SESSIONS && middle > 0.0 && per_turn(s) >= OUTLIER_FACTOR * middle;
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
