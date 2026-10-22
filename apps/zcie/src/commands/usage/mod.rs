//! `zcie usage`: the token savings of ZCIE for each project, from the Claude Code transcripts and
//! the hook statistics (ADR-022). `--json` gives the full report for the menu bar app.

pub mod honest;
pub mod index;
pub mod report;
pub mod scan;
pub mod session;
#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests;

use std::collections::BTreeMap;
use std::io::BufRead;
use std::path::{Path, PathBuf};

use anyhow::Context;
use clap::Args;

use self::report::{Project, Report, Root, Stat};
use super::import::git_root;
use super::project::find_project_id;

/// Options of `zcie usage`.
#[derive(Debug, Args)]
pub struct UsageArgs {
    /// Print the full report as JSON.
    #[arg(long)]
    pub json: bool,
    /// Days of the window.
    #[arg(long, default_value_t = 90)]
    pub days: u32,
    /// Transcript folder. Default: ~/.claude/projects.
    #[arg(long)]
    pub dir: Option<PathBuf>,
    /// Read no project folder to find project roots: use the roots that earlier runs and the
    /// SessionStart hook recorded. The menu bar app uses it, because macOS asks before a program
    /// reads Desktop, Documents or Downloads.
    #[arg(long)]
    pub no_disk: bool,
}

/// Runs `zcie usage`.
pub fn run(args: &UsageArgs, home: &Path, client: &zcie_sdk::Client) -> anyhow::Result<()> {
    let dir = match &args.dir {
        Some(dir) => dir.clone(),
        None => PathBuf::from(crate::commands::user_home().context("HOME is not set")?).join(".claude/projects"),
    };
    let mut report = make_report(&dir, home, args.days, !args.no_disk)?;
    report.index = Some(index::build(home, client, &report.since));
    if args.json {
        return super::print_json(&report);
    }
    print_table(&report);
    Ok(())
}

/// Scans the transcripts and the hook statistics and builds the report of the window. With
/// `disk`, an unknown folder gets its project root from the disk.
pub fn make_report(dir: &Path, home: &Path, window_days: u32, disk: bool) -> anyhow::Result<Report> {
    let sessions = scan::scan(dir, &home.join("run").join("usage-cache.json"))?;
    let stats = read_stats(&home.join("stats").join("hooks.jsonl"));
    let roots_path = home.join("run").join("usage-roots.json");
    let mut roots = load_roots(&roots_path, &stats);
    let recorded = roots.clone();
    let now = time::OffsetDateTime::now_utc();
    let since = (now - time::Duration::days(i64::from(window_days))).date();
    let since = format!("{:04}-{:02}-{:02}", since.year(), u8::from(since.month()), since.day());
    let mut resolve = |cwd: &str| resolve_root(&mut roots, cwd, disk);
    let mut report = report::build(sessions.into_values().collect(), stats, &since, &mut resolve);
    // Written only when a root was added or changed.
    if (roots != recorded || !roots_path.exists())
        && let Ok(bytes) = serde_json::to_vec(&roots)
    {
        scan::write_atomic(&roots_path, &bytes);
    }
    report.window_days = window_days;
    report.generated_at = now.format(&time::format_description::well_known::Rfc3339).unwrap_or_default();
    Ok(report)
}

/// The recorded roots, and the roots that the SessionStart hook recorded (the last one wins).
fn load_roots(path: &Path, stats: &[Stat]) -> BTreeMap<String, Root> {
    let mut roots: BTreeMap<String, Root> = std::fs::read(path).ok().and_then(|bytes| serde_json::from_slice(&bytes).ok()).unwrap_or_default();
    for stat in stats.iter().filter(|s| s.kind == "session" && !s.root.is_empty() && !s.cwd.is_empty()) {
        let id = Some(stat.project_id.clone()).filter(|id| !id.is_empty());
        roots.insert(stat.cwd.clone(), Root { path: stat.root.clone(), id });
    }
    roots
}

/// The root of a folder: a recorded root; else, with `disk`, the root on disk (then recorded);
/// else the folder, and a linked worktree (`.claude/worktrees/<name>`) goes to its repository.
fn resolve_root(roots: &mut BTreeMap<String, Root>, cwd: &str, disk: bool) -> Root {
    if let Some(root) = roots.get(cwd) {
        return root.clone();
    }
    if disk {
        let root = root_of(cwd);
        roots.insert(cwd.to_string(), root.clone());
        return root;
    }
    let path = cwd.find("/.claude/worktrees/").map_or(cwd, |end| &cwd[..end]);
    Root { path: path.to_string(), id: None }
}

/// The project of a folder: the folder with `.claude/zcie_id`, else the Git root, else the folder.
fn root_of(cwd: &str) -> Root {
    let folder = Path::new(cwd);
    if let Some((root, id)) = find_project_id(folder) {
        return Root { path: root.display().to_string(), id: Some(id) };
    }
    let root = git_root(folder).unwrap_or_else(|| folder.to_path_buf());
    let id = find_project_id(&root).map(|(_, id)| id);
    Root { path: root.display().to_string(), id }
}

/// Reads the hook statistics. A missing file or a bad line gives no statistics.
fn read_stats(path: &Path) -> Vec<Stat> {
    let Ok(file) = std::fs::File::open(path) else { return Vec::new() };
    std::io::BufReader::new(file).lines().map_while(Result::ok).filter_map(|line| serde_json::from_str(&line).ok()).collect()
}

/// "1.3B", "1.2M", "34.5k" or "812".
fn short(value: f64) -> String {
    let size = value.abs();
    if size >= 1e9 {
        format!("{:.1}B", value / 1e9)
    } else if size >= 1e6 {
        format!("{:.1}M", value / 1e6)
    } else if size >= 1e4 {
        format!("{:.1}k", value / 1e3)
    } else {
        format!("{value:.0}")
    }
}

fn saving_text(project: &Project) -> (String, String) {
    match (project.saving.percent, project.saving.tokens) {
        // A saving is fewer tokens: "-42%". More tokens with ZCIE: "+7%".
        (Some(percent), Some(tokens)) => (format!("{}{:.0}%", if percent >= 0.0 { "-" } else { "+" }, percent.abs()), short(tokens as f64)),
        _ => ("-".into(), project.saving.note.clone()),
    }
}

fn print_table(report: &Report) {
    println!("ZCIE token savings, last {} days (since {}), observed", report.window_days, report.since);
    println!("{:<28} {:>6} {:>6} {:>12} {:>12} {:>7}  saved tokens", "project", "zcie", "plain", "tok/turn z", "tok/turn p", "saving");
    let per_turn = |value: Option<f64>| value.map(short).unwrap_or_else(|| "-".into());
    for project in &report.projects {
        let (percent, saved) = saving_text(project);
        println!(
            "{:<28} {:>6} {:>6} {:>12} {:>12} {:>7}  {saved}",
            project.name.chars().take(28).collect::<String>(),
            project.zcie.sessions,
            project.plain.sessions,
            per_turn(project.zcie.tokens_per_turn),
            per_turn(project.plain.tokens_per_turn),
            percent,
        );
    }
    println!("saved tokens (projects with a value): {}", short(report.saved_tokens as f64));
    println!("sessions: {} with ZCIE, {} with no ZCIE", report.zcie_sessions, report.plain_sessions);
    println!("{}", report.method);
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod text_tests {
    use super::*;

    #[test]
    fn saving_text_shows_a_saving_as_minus() {
        let mut project = Project::default();
        project.saving.percent = Some(42.4);
        project.saving.tokens = Some(1_250_000);
        assert_eq!(saving_text(&project), ("-42%".to_string(), "1.2M".to_string()));
        project.saving.percent = Some(-7.0);
        project.saving.tokens = Some(-30_000);
        assert_eq!(saving_text(&project), ("+7%".to_string(), "-30.0k".to_string()));
    }
}
