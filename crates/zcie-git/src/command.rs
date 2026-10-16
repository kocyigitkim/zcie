//! Runs the `git` program.

use std::io::ErrorKind;
use std::path::Path;
use std::process::Command;

use crate::error::{Error, Result};

/// Runs `git -C <root> <args>`. Returns the standard output.
pub fn run_git(root: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("LC_ALL", "C")
        .output()
        .map_err(|e| match e.kind() {
            ErrorKind::NotFound => Error::GitUnavailable,
            _ => Error::Io(e),
        })?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(Error::Command(stderr));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}
