//! `zcie project add` and `zcie project status`: add a project folder, index it, and show what
//! the index has and what still waits.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

use zcie_sdk::Client;
use zcie_types::api::ProjectStatus;

use super::project::{find_project_id, init, project_root};

/// Interval between two progress lines.
const POLL: Duration = Duration::from_secs(2);

/// Adds a project folder and, with `wait`, shows the index progress until it is complete. Git
/// repositories inside the folder are separate projects; `nested` adds them too.
pub fn add(client: &Client, path: Option<PathBuf>, name: Option<String>, wait: bool, nested: bool) -> anyhow::Result<()> {
    let root = project_root(path)?;
    let id = init(client, Some(root.clone()), name, true)?;
    let repositories = nested_repositories(&root);
    report_skipped(&root, &repositories, nested);
    if wait {
        wait_until_ready(client, &id)?;
    } else {
        println!("Indexing goes on in the background. Check it with: zcie project status {id}");
    }
    if nested {
        for repository in repositories {
            println!("
== {}", repository.display());
            add(client, Some(repository), None, wait, false)?;
        }
    }
    Ok(())
}

/// Returns the Git repositories in the first two folder levels under `root` (not `root`).
fn nested_repositories(root: &Path) -> Vec<PathBuf> {
    let subfolders = |dir: &Path| -> Vec<PathBuf> {
        let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
        let mut out: Vec<PathBuf> = entries.filter_map(Result::ok).map(|e| e.path()).filter(|p| p.is_dir() && !p.file_name().is_some_and(|n| n.to_string_lossy().starts_with('.') || n == "node_modules")).collect();
        out.sort();
        out
    };
    let mut found = Vec::new();
    for first in subfolders(root) {
        if first.join(".git").exists() {
            found.push(first);
            continue;
        }
        found.extend(subfolders(&first).into_iter().filter(|second| second.join(".git").exists()));
    }
    found
}

/// Says which top folders the index skips: Git repositories (separate projects) and `.gitignore` folders.
fn report_skipped(root: &Path, repositories: &[PathBuf], nested: bool) {
    let relative = |p: &Path| p.strip_prefix(root).unwrap_or(p).display().to_string();
    for repository in repositories {
        let hint = if nested { "added as its own project below".to_string() } else { format!("add it with: zcie project add {}", repository.display()) };
        println!("  skipped {}/ (a separate Git repository; {hint})", relative(repository));
    }
    let Ok(entries) = std::fs::read_dir(root) else { return };
    let mut ignored: Vec<String> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.is_dir() && !repositories.contains(p) && git_ignores(root, p))
        .map(|p| relative(&p))
        .collect();
    ignored.sort();
    if !ignored.is_empty() {
        println!("  skipped {} (.gitignore)", ignored.iter().map(|d| format!("{d}/")).collect::<Vec<_>>().join(", "));
    }
}

fn git_ignores(root: &Path, path: &Path) -> bool {
    std::process::Command::new("git").arg("-C").arg(root).args(["check-ignore", "-q"]).arg(path).stderr(std::process::Stdio::null()).status().is_ok_and(|s| s.success())
}

/// Shows the index progress of a project until no job of it waits.
fn wait_until_ready(client: &Client, id: &str) -> anyhow::Result<()> {
    println!("Indexing (Ctrl+C stops the wait only; the index goes on in the background)…");
    let mut last = String::new();
    loop {
        let state = client.project_status(id)?;
        let line = progress(&state);
        if line != last {
            print!("\r{line:<90}");
            let _ = std::io::stdout().flush();
            last = line;
        }
        if state.ready {
            println!();
            break;
        }
        std::thread::sleep(POLL);
    }
    print_status(&client.project_status(id)?);
    println!("\nAsk about it: zcie query --project {id} \"where is ... defined\", or open Claude Code in the folder.");
    Ok(())
}

fn progress(state: &ProjectStatus) -> String {
    let phase = if state.ready { "complete" } else { "indexing" };
    format!("{phase}: {} files, {} symbols, {} commits, {} jobs waiting", state.files, state.symbols, state.commits, state.pending_jobs)
}

/// Prints the index state of a project: an ID, a name, a folder, or the current project.
pub fn status(client: &Client, project: Option<String>) -> anyhow::Result<()> {
    let target = match project {
        Some(value) if Path::new(&value).is_dir() => folder_project(Path::new(&value))?,
        Some(value) => value,
        None => client.workspace().to_string(),
    };
    print_status(&client.project_status(&target)?);
    Ok(())
}

fn folder_project(folder: &Path) -> anyhow::Result<String> {
    let folder = folder.canonicalize()?;
    match find_project_id(&folder) {
        Some((_, id)) => Ok(id),
        None => anyhow::bail!("{} has no project. Add it with: zcie project add {}", folder.display(), folder.display()),
    }
}

fn print_status(state: &ProjectStatus) {
    let p = &state.project;
    println!("project   {} ({})", p.name, p.project_id);
    println!("folders   {}", if p.roots.is_empty() { "-".to_string() } else { p.roots.join(", ") });
    println!("files     {}", state.files);
    println!("symbols   {}", state.symbols);
    println!("commits   {}", state.commits);
    println!("memory    {} records", state.memories);
    match state.ready {
        _ if !state.indexed => println!("index     off: memory only (index the files with: zcie project add <folder>)"),
        true => println!("index     complete"),
        false => println!("index     {} jobs waiting (zcie project status shows the progress)", state.pending_jobs),
    }
}
