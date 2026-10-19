//! Savings of each project (ADR-022, D-084): the median tokens per turn of the sessions with ZCIE
//! against the sessions with no ZCIE of the same project. The value is an observation of the real
//! work, not a controlled measurement: the tasks of the two groups are not the same.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::scan::Session;

/// Folders of temporary sessions (benchmarks, scratch folders): never a project.
const TEMPORARY: [&str; 4] = ["/tmp", "/private/tmp", "/var/folders", "/private/var"];
/// The method note of the report.
pub const METHOD: &str = "Observed on your sessions: the median cost per turn in input-token units (input ×1, cache write ×1.25 or ×2, cache read ×0.1, output ×5, the price ratios of Claude) of the sessions with ZCIE against the sessions with no ZCIE of the same project. A project with no session with no ZCIE is compared with the sessions with no ZCIE of all projects; that value is marked rough (≈). A session that costs 3 times the median per turn of its project or more (one large task, for example many subagents) is an outlier: it is listed but left out of the saving. The confidence is low with fewer than 3 sessions in a group, medium with fewer than 10. Each value shows the number of sessions it uses; a value from few sessions changes much. The tasks of the groups are not the same, so each value is an estimate. The controlled measurement is the isolated benchmark (ADR-019).";

/// The project of a folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Root {
    /// Root folder of the project.
    pub path: String,
    /// Project ID of `.claude/zcie_id`, if any.
    pub id: Option<String>,
}

/// One line of `<zcie home>/stats/hooks.jsonl` (D-085).
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Stat {
    /// Time (RFC 3339, UTC).
    #[serde(default)]
    pub ts: String,
    /// `read_guard` or `gate`.
    #[serde(default)]
    pub kind: String,
    /// Folder of the session.
    #[serde(default)]
    pub cwd: String,
    /// Read guard: lines of the file.
    #[serde(default)]
    pub lines: u64,
    /// Read guard: lines that the Read got.
    #[serde(default)]
    pub read: u64,
    /// Gate: `pass`, `block`, `skipped` or `fail`.
    #[serde(default)]
    pub result: String,
    /// Session start: the project root of the folder.
    #[serde(default)]
    pub root: String,
    /// Session start: the project ID, or empty.
    #[serde(default)]
    pub project_id: String,
}

/// The sessions of one group (with ZCIE, or with no ZCIE).
#[derive(Debug, Clone, Default, Serialize)]
pub struct Group {
    pub sessions: u64,
    pub turns: u64,
    pub tokens: u64,
    /// The parts of `tokens`. Cache reads cost about a tenth of new input.
    pub input: u64,
    pub cache_write: u64,
    pub cache_read: u64,
    pub output: u64,
    /// Cost in input-token units (`session::WEIGHTS`): what the tokens cost, not how many they are.
    pub weighted: f64,
    /// Median of the sessions; none with no session.
    pub tokens_per_turn: Option<f64>,
    /// Median cost per turn in input-token units. The saving uses this value.
    pub weighted_per_turn: Option<f64>,
    pub reads_per_turn: Option<f64>,
    pub searches_per_turn: Option<f64>,
    pub capsules: u64,
    pub capsule_chars: u64,
}

/// The saving of a project.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Saving {
    /// 1 − median cost with ZCIE / median cost with no ZCIE, in percent. Negative: ZCIE cost more.
    pub percent: Option<f64>,
    /// Turns with ZCIE × (median with no ZCIE − median with ZCIE), in input-token units.
    pub tokens: Option<i64>,
    /// Why there is no value, "observed", or the rough basis.
    pub note: String,
    /// `low`, `medium` or `high` (`honest::confidence`); empty with no value.
    pub confidence: String,
    /// Outlier sessions left out of the saving.
    pub excluded: u64,
    /// `project` (sessions with no ZCIE of the project), `all projects` (rough: the project has
    /// none), or empty with no value.
    pub basis: String,
}

/// Read guard actions.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Guard {
    pub events: u64,
    pub file_lines: u64,
    pub lines_read: u64,
}

/// Verification gate results.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Gate {
    pub pass: u64,
    pub block: u64,
    pub skipped: u64,
    pub fail: u64,
}

/// Tokens and turns of one day of a project.
#[derive(Debug, Clone, Default, Serialize)]
pub struct DayRow {
    pub date: String,
    pub zcie_tokens: u64,
    pub plain_tokens: u64,
    pub zcie_turns: u64,
    pub plain_turns: u64,
}

/// The report of one project.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Project {
    pub name: String,
    pub root: String,
    pub project_id: Option<String>,
    pub last_active: String,
    pub zcie: Group,
    pub plain: Group,
    pub saving: Saving,
    pub guard: Guard,
    pub gate: Gate,
    pub days: Vec<DayRow>,
    /// Each session, newest first (ADR-024).
    pub sessions: Vec<super::honest::SessionRow>,
}

/// Sessions that a program started (`claude -p`, for example the summaries of a plugin): not in
/// the saving, but they cost tokens.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Automated {
    pub sessions: u64,
    pub cost: u64,
}

/// The full report.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Report {
    pub generated_at: String,
    pub window_days: u32,
    /// First day of the window ("YYYY-MM-DD").
    pub since: String,
    pub method: String,
    /// Sum of the saved tokens of the projects with a saving against their own sessions.
    pub saved_tokens: i64,
    /// Sum of the rough saved tokens (basis `all projects`).
    pub rough_saved_tokens: i64,
    pub zcie_sessions: u64,
    pub plain_sessions: u64,
    /// Projects, the last activity first.
    pub projects: Vec<Project>,
    pub automated: Automated,
    /// The index, the daemon, and the reads and writes of the activity log (ADR-023).
    pub index: Option<super::index::IndexReport>,
}

/// The sessions and statistics of one project, before the report.
#[derive(Default)]
struct Bucket {
    zcie: Vec<Session>,
    plain: Vec<Session>,
    stats: Vec<Stat>,
}

/// Builds the report. `resolve` gives the project of a folder.
pub fn build(sessions: Vec<Session>, stats: Vec<Stat>, since: &str, resolve: &mut dyn FnMut(&str) -> Root) -> Report {
    let mut buckets: BTreeMap<String, (Root, Bucket)> = BTreeMap::new();
    let usable = |s: &Session| !s.automated && s.turns > 0 && s.last.as_str() >= since && !s.cwd.is_empty() && !TEMPORARY.iter().any(|t| s.cwd.starts_with(t));
    let mut automated = Automated::default();
    for session in sessions.iter().filter(|s| s.automated && s.last.as_str() >= since) {
        automated.sessions += 1;
        automated.cost += session.weighted().round() as u64;
    }
    for session in sessions.into_iter().flat_map(split).filter(usable) {
        let root = resolve(&session.cwd);
        let (_, bucket) = buckets.entry(root.path.clone()).or_insert_with(|| (root, Bucket::default()));
        if session.zcie { bucket.zcie.push(session) } else { bucket.plain.push(session) }
    }
    for stat in stats.into_iter().filter(|s| s.ts.as_str() >= since && !s.cwd.is_empty()) {
        let root = resolve(&stat.cwd);
        if let Some((_, bucket)) = buckets.get_mut(&root.path) {
            bucket.stats.push(stat);
        }
    }
    let global = baseline(buckets.values().map(|(_, b)| b));
    let mut projects: Vec<Project> = buckets.into_values().map(|(root, bucket)| project(root, &bucket, since, global)).collect();
    projects.sort_by(|a, b| b.last_active.cmp(&a.last_active));
    unique_names(&mut projects);
    Report {
        window_days: 0,
        since: since.to_string(),
        method: METHOD.to_string(),
        saved_tokens: saved(&projects, "project"),
        rough_saved_tokens: saved(&projects, "all projects"),
        zcie_sessions: projects.iter().map(|p| p.zcie.sessions).sum(),
        plain_sessions: projects.iter().map(|p| p.plain.sessions).sum(),
        automated,
        projects,
        ..Report::default()
    }
}

/// A session that ZCIE joined later gives two sessions: the part before (no ZCIE) and the rest.
fn split(mut session: Session) -> Vec<Session> {
    match session.before.take() {
        Some(before) if session.zcie => {
            let mut before = *before;
            before.cwd.clone_from(&session.cwd);
            // The part before ZCIE: the same ID with a mark, so that its row is not empty.
            before.id = format!("{}·pre", session.id.chars().take(5).collect::<String>());
            before.automated = session.automated;
            before.zcie = false;
            vec![before, session]
        }
        _ => vec![session],
    }
}

fn saved(projects: &[Project], basis: &str) -> i64 {
    projects.iter().filter(|p| p.saving.basis == basis).filter_map(|p| p.saving.tokens).sum()
}

/// The median tokens per turn of all sessions with no ZCIE, for a project with no baseline of its
/// own. None with no such session.
#[derive(Debug, Clone, Copy)]
struct Baseline {
    per_turn: f64,
    sessions: u64,
}

fn baseline<'a>(buckets: impl Iterator<Item = &'a Bucket>) -> Option<Baseline> {
    let values: Vec<f64> = buckets.flat_map(|b| &b.plain).map(|s| s.weighted() / s.turns as f64).collect();
    let sessions = values.len() as u64;
    median(values).map(|per_turn| Baseline { per_turn, sessions })
}

fn project(root: Root, bucket: &Bucket, since: &str, global: Option<Baseline>) -> Project {
    let (zcie, plain) = (group(&bucket.zcie), group(&bucket.plain));
    let (sessions, outliers) = super::honest::rows(&bucket.zcie, &bucket.plain);
    let kept = |list: &[Session]| list.iter().filter(|s| !outliers.contains(&s.id)).cloned().collect::<Vec<_>>();
    let (kept_zcie, kept_plain) = (group(&kept(&bucket.zcie)), group(&kept(&bucket.plain)));
    let mut saving = saving(&kept_zcie, &kept_plain, global);
    saving.excluded = outliers.len() as u64;
    if saving.percent.is_some() {
        saving.confidence = super::honest::confidence(kept_zcie.sessions, kept_plain.sessions, saving.basis != "project").into();
    }
    let name = root.path.rsplit('/').find(|p| !p.is_empty()).unwrap_or("/").to_string();
    let last_active = bucket.zcie.iter().chain(&bucket.plain).map(|s| s.last.clone()).max().unwrap_or_default();
    Project {
        name,
        saving,
        sessions,
        guard: guard(&bucket.stats),
        gate: gate(&bucket.stats),
        days: days(bucket, since),
        root: root.path,
        project_id: root.id,
        last_active,
        zcie,
        plain,
    }
}

/// Gives projects with the same folder name the name "<parent>/<name>".
fn unique_names(projects: &mut [Project]) {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for project in projects.iter() {
        *counts.entry(project.name.clone()).or_default() += 1;
    }
    for project in projects.iter_mut().filter(|p| counts[&p.name] > 1) {
        let parts: Vec<&str> = project.root.rsplit('/').filter(|p| !p.is_empty()).take(2).collect();
        project.name = parts.into_iter().rev().collect::<Vec<_>>().join("/");
    }
}

/// Median of a list; none for an empty list.
pub fn median(mut values: Vec<f64>) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    values.sort_by(f64::total_cmp);
    let middle = values.len() / 2;
    Some(if values.len().is_multiple_of(2) { (values[middle - 1] + values[middle]) / 2.0 } else { values[middle] })
}

fn group(sessions: &[Session]) -> Group {
    let per_turn = |value: fn(&Session) -> u64| median(sessions.iter().map(|s| value(s) as f64 / s.turns as f64).collect());
    Group {
        sessions: sessions.len() as u64,
        turns: sessions.iter().map(|s| s.turns).sum(),
        tokens: sessions.iter().map(Session::tokens).sum(),
        input: sessions.iter().map(|s| s.input).sum(),
        cache_write: sessions.iter().map(|s| s.cache_write).sum(),
        cache_read: sessions.iter().map(|s| s.cache_read).sum(),
        output: sessions.iter().map(|s| s.output).sum(),
        tokens_per_turn: per_turn(Session::tokens),
        weighted: sessions.iter().map(Session::weighted).sum(),
        weighted_per_turn: median(sessions.iter().map(|s| s.weighted() / s.turns as f64).collect()),
        reads_per_turn: per_turn(|s| s.reads),
        searches_per_turn: per_turn(|s| s.searches),
        capsules: sessions.iter().map(|s| s.capsules).sum(),
        capsule_chars: sessions.iter().map(|s| s.capsule_chars).sum(),
    }
}

fn saving(zcie: &Group, plain: &Group, global: Option<Baseline>) -> Saving {
    if zcie.sessions == 0 {
        return Saving { note: "no sessions with ZCIE".into(), ..Saving::default() };
    }
    if plain.sessions > 0 {
        let text = format!("observed: {} with ZCIE against {} with no ZCIE", count(zcie.sessions), count(plain.sessions));
        return compare(zcie, plain.weighted_per_turn, "project", text);
    }
    match global {
        Some(base) => {
            let text = format!("rough: {} with ZCIE against {} with no ZCIE in all projects", count(zcie.sessions), count(base.sessions));
            compare(zcie, Some(base.per_turn), "all projects", text)
        }
        None => Saving { note: "no sessions with no ZCIE in any project".into(), ..Saving::default() },
    }
}

/// "1 session", "3 sessions".
fn count(sessions: u64) -> String {
    format!("{sessions} session{}", if sessions == 1 { "" } else { "s" })
}

/// The saving of the ZCIE sessions against a median with no ZCIE.
fn compare(zcie: &Group, without: Option<f64>, basis: &str, note: String) -> Saving {
    match (zcie.weighted_per_turn, without.filter(|w| *w > 0.0)) {
        (Some(with), Some(without)) => Saving {
            percent: Some((1.0 - with / without) * 100.0),
            tokens: Some(((without - with) * zcie.turns as f64).round() as i64),
            note,
            basis: basis.into(),
            ..Saving::default()
        },
        _ => Saving { note: "no tokens with no ZCIE".into(), ..Saving::default() },
    }
}

fn guard(stats: &[Stat]) -> Guard {
    let events: Vec<&Stat> = stats.iter().filter(|s| s.kind == "read_guard").collect();
    Guard { events: events.len() as u64, file_lines: events.iter().map(|s| s.lines).sum(), lines_read: events.iter().map(|s| s.read).sum() }
}

fn gate(stats: &[Stat]) -> Gate {
    let count = |result: &str| stats.iter().filter(|s| s.kind == "gate" && s.result == result).count() as u64;
    Gate { pass: count("pass"), block: count("block"), skipped: count("skipped"), fail: count("fail") }
}

/// Tokens and turns of each day in the window, by group.
fn days(bucket: &Bucket, since: &str) -> Vec<DayRow> {
    let mut rows: BTreeMap<String, DayRow> = BTreeMap::new();
    for (zcie, sessions) in [(true, &bucket.zcie), (false, &bucket.plain)] {
        for (date, day) in sessions.iter().flat_map(|s| &s.days).filter(|(d, _)| d.as_str() >= since) {
            let row = rows.entry(date.clone()).or_insert_with(|| DayRow { date: date.clone(), ..DayRow::default() });
            let (tokens, turns) = if zcie { (&mut row.zcie_tokens, &mut row.zcie_turns) } else { (&mut row.plain_tokens, &mut row.plain_turns) };
            *tokens += day.tokens;
            *turns += day.turns;
        }
    }
    rows.into_values().collect()
}
