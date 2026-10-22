//! Signal handling: SIGINT and SIGTERM stop the daemon, SIGHUP reloads `clients.toml`.
//! Windows has no SIGTERM or SIGHUP: the file `run/zcied.stop` stops the daemon, and a change of
//! `clients.toml` reloads it (ADR-027).

use std::path::PathBuf;
use std::sync::Arc;
#[cfg(not(unix))]
use std::time::Duration;

use zcie_api::Engine;

/// How often the Windows daemon checks the stop file and `clients.toml`.
#[cfg(not(unix))]
const POLL: Duration = Duration::from_millis(500);

/// Completes on SIGINT or SIGTERM.
#[cfg(unix)]
pub async fn stop_signal(_run_dir: PathBuf) {
    use tokio::signal::unix::{SignalKind, signal};
    let Ok(mut terminate) = signal(SignalKind::terminate()) else {
        let _ = tokio::signal::ctrl_c().await;
        return;
    };
    tokio::select! {
        _ = tokio::signal::ctrl_c() => tracing::info!("SIGINT received"),
        _ = terminate.recv() => tracing::info!("SIGTERM received"),
    }
}

/// Completes on Ctrl+C or when `run/zcied.stop` exists (written by `zcie daemon stop`).
#[cfg(not(unix))]
pub async fn stop_signal(run_dir: PathBuf) {
    let stop_file = run_dir.join("zcied.stop");
    let _ = std::fs::remove_file(&stop_file);
    let wait_for_file = async {
        while !stop_file.exists() {
            tokio::time::sleep(POLL).await;
        }
        let _ = std::fs::remove_file(&stop_file);
    };
    tokio::select! {
        _ = tokio::signal::ctrl_c() => tracing::info!("Ctrl+C received"),
        _ = wait_for_file => tracing::info!("stop file found"),
    }
}

fn reload(engine: &Engine, clients_path: &std::path::Path) {
    match zcie_security::ClientsFile::load(clients_path) {
        Ok(clients) => {
            engine.reload_clients(clients);
            tracing::info!("clients.toml reloaded");
        }
        Err(error) => tracing::warn!(%error, "clients.toml reload failed"),
    }
}

/// Reloads the API key file on each SIGHUP.
#[cfg(unix)]
pub fn spawn_reload_on_hangup(engine: Arc<Engine>, clients_path: PathBuf) {
    use tokio::signal::unix::{SignalKind, signal};
    tokio::spawn(async move {
        let Ok(mut hangup) = signal(SignalKind::hangup()) else { return };
        while hangup.recv().await.is_some() {
            reload(&engine, &clients_path);
        }
    });
}

/// Reloads the API key file when its modification time changes.
#[cfg(not(unix))]
pub fn spawn_reload_on_hangup(engine: Arc<Engine>, clients_path: PathBuf) {
    let modified = |path: &std::path::Path| std::fs::metadata(path).and_then(|m| m.modified()).ok();
    tokio::spawn(async move {
        let mut last = modified(&clients_path);
        loop {
            tokio::time::sleep(POLL).await;
            let now = modified(&clients_path);
            if now != last {
                last = now;
                reload(&engine, &clients_path);
            }
        }
    });
}
