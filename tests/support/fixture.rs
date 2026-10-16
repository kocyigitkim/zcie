//! The fixture repository (`docs/13-test-plan.md` section 4.1).

use std::path::{Path, PathBuf};
use std::process::Command;

/// Runs `git` in `dir` and panics on failure. Returns stdout.
pub fn git(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_AUTHOR_NAME", "Test")
        .env("GIT_AUTHOR_EMAIL", "test@example.com")
        .env("GIT_COMMITTER_NAME", "Test")
        .env("GIT_COMMITTER_EMAIL", "test@example.com")
        .output()
        .expect("git runs");
    assert!(output.status.success(), "git {args:?} failed: {}", String::from_utf8_lossy(&output.stderr));
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

/// Commits all changes with a message. Returns the SHA.
pub fn commit_all(dir: &Path, message: &str) -> String {
    git(dir, &["add", "-A"]);
    git(dir, &["commit", "-q", "-m", message]);
    git(dir, &["rev-parse", "HEAD"])
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}

/// Copies the fixture repository into `parent/sample-repo`, runs `git init` and commits.
///
/// With `with_git = false` the folder has no `.git`.
pub fn fixture_repo(parent: &Path, with_git: bool) -> PathBuf {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures").join("sample-repo");
    let target = parent.join("sample-repo");
    copy_dir(&source, &target);
    std::fs::write(target.join(".env"), "API_KEY=abc\n").unwrap();
    std::fs::create_dir_all(target.join("assets")).unwrap();
    std::fs::write(target.join("assets").join("icon.bin"), b"BIN\x00\x01\x02\x03\x04\x05\x06\x07\x08\x09\x0a\x0b").unwrap();
    if with_git {
        git(&target, &["init", "-q", "-b", "main"]);
        commit_all(&target, "initial");
    }
    target
}
