//! `zcie daemon start | stop | status`.

use std::fs::OpenOptions;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Context, bail};
use clap::Subcommand;
use zcie_sdk::Client;

use super::print_json;

/// Time to wait for the daemon health check after start (a large store opens in a few seconds).
const START_WAIT: Duration = Duration::from_secs(60);
/// Maximum wait for a clean stop (the engine flushes the search index; a long file job ends first).
const STOP_WAIT: Duration = Duration::from_secs(90);
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
    /// Stop and start `zcied` (after an update or a change of `engine.toml`).
    Restart,
    /// Show the daemon health.
    Status,
    /// Start the daemon at login and after a crash (macOS launchd agent). Stops a running daemon first.
    Install {
        /// Print the agent file only.
        #[arg(long)]
        dry_run: bool,
    },
    /// Remove the launchd agent. The daemon stops.
    Uninstall,
}

/// Runs one daemon action.
pub fn run(action: DaemonAction, home: &Path, client: &Client) -> anyhow::Result<()> {
    match action {
        DaemonAction::Start { foreground } => start(home, client, foreground),
        DaemonAction::Stop => stop(home),
        DaemonAction::Restart => restart(home, client),
        DaemonAction::Status => print_json(&client.health()?),
        DaemonAction::Install { dry_run } => {
            if !dry_run && client.health().is_ok() {
                stop(home)?;
            }
            super::service::install(&daemon_binary()?, home, dry_run)
        }
        DaemonAction::Uninstall => super::service::uninstall(home),
    }
}

/// Restarts the daemon: through launchd if its agent is loaded, else with stop and start.
pub fn restart(home: &Path, client: &Client) -> anyhow::Result<()> {
    if super::service::is_loaded(home) {
        super::service::kickstart(&daemon_binary()?, home)?;
        return wait_for_health(client, &log_file(home)?);
    }
    if client.health().is_ok() {
        stop(home)?;
    }
    start(home, client, false)
}

/// Returns the path of the `zcied` binary next to the `zcie` binary.
pub fn daemon_binary() -> anyhow::Result<PathBuf> {
    let exe = std::env::current_exe().context("cannot find the zcie binary path")?;
    let dir = exe.parent().context("zcie binary has no parent folder")?;
    let path = dir.join(format!("zcied{}", std::env::consts::EXE_SUFFIX));
    if !path.exists() {
        bail!("zcied not found at {}", path.display());
    }
    Ok(path)
}

/// Starts the daemon in the background (or in this terminal) and waits for its health check.
pub fn start(home: &Path, client: &Client, foreground: bool) -> anyhow::Result<()> {
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

/// Returns the daemon log file (and makes its folder).
pub fn log_file(home: &Path) -> anyhow::Result<PathBuf> {
    let dir = home.join("instances/local/telemetry/logs");
    std::fs::create_dir_all(&dir)?;
    Ok(dir.join("zcied.out"))
}

#[cfg(unix)]
fn detach(command: &mut Command) {
    use std::os::unix::process::CommandExt;
    command.process_group(0);
}

#[cfg(windows)]
fn detach(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    // DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW
    command.creation_flags(0x0000_0008 | 0x0000_0200 | 0x0800_0000);
}

#[cfg(not(any(unix, windows)))]
fn detach(_command: &mut Command) {}

/// True if the process `pid` runs.
#[cfg(unix)]
fn alive(pid: &str) -> bool {
    Command::new("kill").arg("-0").arg(pid).stderr(Stdio::null()).status().is_ok_and(|s| s.success())
}

/// True if the process `pid` runs.
#[cfg(not(unix))]
fn alive(pid: &str) -> bool {
    Command::new("tasklist")
        .args(["/FI", &format!("PID eq {pid}"), "/NH", "/FO", "CSV"])
        .output()
        .is_ok_and(|out| String::from_utf8_lossy(&out.stdout).contains(&format!("\"{pid}\"")))
}

/// Asks the daemon to stop: SIGTERM.
#[cfg(unix)]
fn request_stop(_home: &Path, pid: &str) -> anyhow::Result<()> {
    let status = Command::new("kill").arg("-TERM").arg(pid).status()?;
    if !status.success() {
        bail!("kill -TERM {pid} failed");
    }
    println!("sent SIGTERM to zcied (pid {pid})");
    Ok(())
}

/// Asks the daemon to stop: the stop file (ADR-027).
#[cfg(not(unix))]
fn request_stop(home: &Path, pid: &str) -> anyhow::Result<()> {
    std::fs::write(home.join("run/zcied.stop"), pid)?;
    println!("asked zcied to stop (pid {pid})");
    Ok(())
}

/// True if the process of `<home>/run/zcied.pid` runs.
pub fn is_running(home: &Path) -> bool {
    let Ok(pid) = std::fs::read_to_string(home.join("run/zcied.pid")) else { return false };
    let pid = pid.trim();
    !pid.is_empty() && pid.chars().all(|c| c.is_ascii_digit()) && alive(pid)
}

/// Waits until the daemon answers its health check and prints where its log is.
pub fn wait_for_health(client: &Client, log_path: &Path) -> anyhow::Result<()> {
    wait_healthy(client, log_path)?;
    println!("zcied started (log: {})", log_path.display());
    Ok(())
}

/// Waits until the daemon answers its health check.
pub fn wait_healthy(client: &Client, log_path: &Path) -> anyhow::Result<()> {
    let started = Instant::now();
    while started.elapsed() < START_WAIT {
        if client.health().is_ok() {
            return Ok(());
        }
        std::thread::sleep(POLL_INTERVAL);
    }
    bail!("zcied did not answer within {}s; see {}", START_WAIT.as_secs(), log_path.display())
}

/// Stops the daemon and waits until the process ends.
pub fn stop(home: &Path) -> anyhow::Result<()> {
    let pid_path = home.join("run/zcied.pid");
    let pid = std::fs::read_to_string(&pid_path)
        .with_context(|| format!("cannot read {}", pid_path.display()))?;
    let pid = pid.trim();
    if pid.is_empty() || !pid.chars().all(|c| c.is_ascii_digit()) {
        bail!("invalid pid file {}", pid_path.display());
    }
    request_stop(home, pid)?;
    wait_for_exit(pid)
}

/// Waits until the process ends, so that a `daemon start` right after `stop` gets the lock.
fn wait_for_exit(pid: &str) -> anyhow::Result<()> {
    let started = Instant::now();
    while started.elapsed() < STOP_WAIT {
        if !alive(pid) {
            println!("zcied stopped");
            return Ok(());
        }
        std::thread::sleep(POLL_INTERVAL);
    }
    bail!("zcied (pid {pid}) did not stop within {}s", STOP_WAIT.as_secs())
}
