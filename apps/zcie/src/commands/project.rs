//! `zcie project`: central management of project memory. A project is identified by the ID
//! in `<root>/.claude/zcie_id` (ADR-009).

use std::path::{Path, PathBuf};

use anyhow::Context;
use clap::Subcommand;
use zcie_sdk::Client;
use zcie_types::api::EnsureProjectRequest;

use super::print_json;

/// Relative path of the project ID file.
pub const ID_FILE: &str = ".claude/zcie_id";

/// Project actions.
#[derive(Debug, Subcommand)]
pub enum ProjectAction {
    /// Make or connect the project of a folder and write `.claude/zcie_id` (memory only; `add`
    /// also indexes the files).
    Init {
        /// Project folder. Default: the Git root of the current folder, or the current folder.
        #[arg(long)]
        dir: Option<PathBuf>,
        /// Project name. Default: the folder name.
        #[arg(long)]
        name: Option<String>,
    },
    /// Add a project folder: make the project, write `.claude/zcie_id`, index the folder and show
    /// the progress until the index is complete.
    Add {
        /// Project folder. Default: the current folder (its Git root if it is in a repository).
        path: Option<PathBuf>,
        /// Project name. Default: the folder name.
        #[arg(long)]
        name: Option<String>,
        /// Do not wait for the index; it goes on in the background.
        #[arg(long)]
        no_wait: bool,
        /// Also add the Git repositories inside the folder (each as its own project).
        #[arg(long)]
        nested: bool,
    },
    /// Show the index state of a project (ID, name or folder). Default: the current project.
    Status {
        /// Project ID, name or folder.
        project: Option<String>,
    },
    /// List all projects.
    List,
    /// Show one project (ID or name). Default: the current project.
    Show {
        /// Project ID or name.
        project: Option<String>,
    },
    /// Rename a project. The ID does not change.
    Rename {
        /// Project ID or name.
        project: String,
        /// New name.
        name: String,
    },
    /// Forget all memory of a project and remove it (admin key).
    Delete {
        /// Project ID or name.
        project: String,
    },
    /// Write `.claude/zcie_id` into the root folders of all registered projects that do not have one.
    Migrate,
}

/// Runs one project action.
pub fn run(action: ProjectAction, client: &Client) -> anyhow::Result<()> {
    match action {
        ProjectAction::Init { dir, name } => init(client, dir, name, false).map(|_| ()),
        ProjectAction::Add { path, name, no_wait, nested } => super::project_add::add(client, path, name, !no_wait, nested),
        ProjectAction::Status { project } => super::project_add::status(client, project),
        ProjectAction::List => list(client),
        ProjectAction::Show { project } => print_json(&client.project(project.as_deref().unwrap_or(client.workspace()))?),
        ProjectAction::Rename { project, name } => print_json(&client.rename_project(&project, &name)?),
        ProjectAction::Delete { project } => print_json(&client.delete_project(&project)?),
        ProjectAction::Migrate => migrate(client),
    }
}

/// Finds `.claude/zcie_id` in `start` or a parent folder. Returns the project root and the ID.
/// The search stops at a Git repository root: a repository inside another project folder is its
/// own project.
pub fn find_project_id(start: &Path) -> Option<(PathBuf, String)> {
    let mut folder = Some(start);
    while let Some(current) = folder {
        if let Ok(text) = std::fs::read_to_string(current.join(ID_FILE)) {
            let id = text.lines().next().unwrap_or_default().trim().to_string();
            if zcie_types::ids::is_project_id(&id) {
                return Some((current.to_path_buf(), id));
            }
        }
        if current.join(".git").exists() {
            return None;
        }
        folder = current.parent();
    }
    None
}

/// Writes the project ID file of a root folder.
pub fn write_project_id(root: &Path, id: &str) -> anyhow::Result<()> {
    let path = root.join(ID_FILE);
    std::fs::create_dir_all(path.parent().context("no parent folder")?)?;
    std::fs::write(&path, format!("{id}\n"))?;
    Ok(())
}

/// Returns the project root of a folder: the root of its ID file, its Git root, or the folder.
pub fn project_root(dir: Option<PathBuf>) -> anyhow::Result<PathBuf> {
    let start = match dir {
        Some(dir) => dir,
        None => std::env::current_dir()?,
    };
    let start = start.canonicalize().with_context(|| format!("{} does not exist", start.display()))?;
    if let Some((root, _)) = find_project_id(&start) {
        return Ok(root);
    }
    let git = std::process::Command::new("git").arg("-C").arg(&start).args(["rev-parse", "--show-toplevel"]).output();
    match git {
        Ok(out) if out.status.success() => Ok(PathBuf::from(String::from_utf8_lossy(&out.stdout).trim())),
        _ => Ok(start),
    }
}

/// Makes or connects the project of a folder and writes its ID file. With `index`, the files of
/// the folder are indexed and watched from now on (ADR-015). Returns the project ID.
pub fn init(client: &Client, dir: Option<PathBuf>, name: Option<String>, index: bool) -> anyhow::Result<String> {
    let root = project_root(dir)?;
    let existing = find_project_id(&root).map(|(_, id)| id);
    let folder = root.file_name().map(|n| n.to_string_lossy().to_string());
    let request = EnsureProjectRequest {
        project_id: existing,
        name: name.or(folder),
        root: Some(root.display().to_string()),
        index,
    };
    let info = client.ensure_project(&request)?;
    write_project_id(&root, &info.project_id)?;
    println!("project {} ({}) -> {}", info.name, info.project_id, root.join(ID_FILE).display());
    Ok(info.project_id)
}

fn list(client: &Client) -> anyhow::Result<()> {
    let projects = client.list_projects()?.projects;
    println!("{:<30} {:<32} ROOTS", "NAME", "ID");
    for project in projects {
        println!("{:<30} {:<32} {}", project.name, project.project_id, project.roots.join(", "));
    }
    Ok(())
}

fn migrate(client: &Client) -> anyhow::Result<()> {
    for project in client.list_projects()?.projects {
        for root in &project.roots {
            let root = Path::new(root);
            if !root.is_dir() {
                println!("skip   {} (folder is missing: {})", project.name, root.display());
                continue;
            }
            match find_project_id(root) {
                Some((_, id)) if id == project.project_id => println!("ok     {} {}", project.name, root.display()),
                Some((_, id)) => println!("skip   {} {} has another ID {id}", project.name, root.display()),
                None => {
                    write_project_id(root, &project.project_id)?;
                    println!("wrote  {} {}", project.name, root.join(ID_FILE).display());
                }
            }
        }
    }
    Ok(())
}
