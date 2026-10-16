//! `zcie daemon start | stop | status`.

use std::fs::OpenOptions;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Context, bail};
use clap::Subcommand;
use zcie_sdk::Client;

use super::print_json;

/// Time to wait for the daemon health check after start.
const START_WAIT: Duration = Duration::from_secs(10);
/// Interval between health checks.
const POLL_INTERVAL: Duration = Duration::from_millis(200);

/// Daemon actions.
#[derive(Debug, Subcommand)]
pub enum DaemonAction {
    /// Start `zcied`.
    Start {
        /// Run the daemon in this terminal and wait for it.
        #[arg(long)]
        foreground: bool,
    },
    /// Stop `zcied` with SIGTERM.
    Stop,
    /// Show the daemon health.
    Status,
}

/// Runs one daemon action.
pub fn run(action: DaemonAction, home: &Path, client: &Client) -> anyhow::Result<()> {
    match action {
        DaemonAction::Start { foreground } => start(home, client, foreground),
        DaemonAction::Stop => stop(home),
        DaemonAction::Status => print_json(&client.health()?),
    }
}

/// Returns the path of the `zcied` binary next to the `zcie` binary.
fn daemon_binary() -> anyhow::Result<PathBuf> {
    let exe = std::env::current_exe().context("cannot find the zcie binary path")?;
    let dir = exe.parent().context("zcie binary has no parent folder")?;
    let path = dir.join("zcied");
    if !path.exists() {
        bail!("zcied not found at {}", path.display());
    }
    Ok(path)
}

fn start(home: &Path, client: &Client, foreground: bool) -> anyhow::Result<()> {
    if client.health().is_ok() {
        println!("zcied is already running");
        return Ok(());
    }
    let binary = daemon_binary()?;
    let mut command = Command::new(binary);
    command.arg("--home").arg(home);
    if foreground {
        let status = command.status().context("cannot start zcied")?;
        if !status.success() {
            bail!("zcied stopped with {status}");
        }
        return Ok(());
    }
    let log_path = log_file(home)?;
    let log = OpenOptions::new().create(true).append(true).open(&log_path)?;
    command.stdin(Stdio::null()).stdout(log.try_clone()?).stderr(log);
    detach(&mut command);
    command.spawn().context("cannot start zcied")?;
    wait_for_health(client, &log_path)
}

fn log_file(home: &Path) -> anyhow::Result<PathBuf> {
    let dir = home.join("instances/local/telemetry/logs");
    std::fs::create_dir_all(&dir)?;
    Ok(dir.join("zcied.out"))
}

#[cfg(unix)]
fn detach(command: &mut Command) {
    use std::os::unix::process::CommandExt;
    command.process_group(0);
}

#[cfg(not(unix))]
fn detach(_command: &mut Command) {}

fn wait_for_health(client: &Client, log_path: &Path) -> anyhow::Result<()> {
    let started = Instant::now();
    while started.elapsed() < START_WAIT {
        if client.health().is_ok() {
            println!("zcied started (log: {})", log_path.display());
            return Ok(());
        }
        std::thread::sleep(POLL_INTERVAL);
    }
    bail!("zcied did not answer within {}s; see {}", START_WAIT.as_secs(), log_path.display())
}

fn stop(home: &Path) -> anyhow::Result<()> {
    let pid_path = home.join("run/zcied.pid");
    let pid = std::fs::read_to_string(&pid_path)
        .with_context(|| format!("cannot read {}", pid_path.display()))?;
    let pid = pid.trim();
    if pid.is_empty() || !pid.chars().all(|c| c.is_ascii_digit()) {
        bail!("invalid pid file {}", pid_path.display());
    }
    let status = Command::new("kill").arg("-TERM").arg(pid).status()?;
    if !status.success() {
        bail!("kill -TERM {pid} failed");
    }
    println!("sent SIGTERM to zcied (pid {pid})");
    Ok(())
}
