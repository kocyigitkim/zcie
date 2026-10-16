//! Signal handling: SIGINT and SIGTERM stop the daemon, SIGHUP reloads `clients.toml`.

use std::path::PathBuf;
use std::sync::Arc;

use tokio::signal::unix::{SignalKind, signal};
use zcie_api::Engine;

/// Completes on SIGINT or SIGTERM.
pub async fn stop_signal() {
    let Ok(mut terminate) = signal(SignalKind::terminate()) else {
        let _ = tokio::signal::ctrl_c().await;
        return;
    };
    tokio::select! {
        _ = tokio::signal::ctrl_c() => tracing::info!("SIGINT received"),
        _ = terminate.recv() => tracing::info!("SIGTERM received"),
    }
}

/// Reloads the API key file on each SIGHUP.
pub fn spawn_reload_on_hangup(engine: Arc<Engine>, clients_path: PathBuf) {
    tokio::spawn(async move {
        let Ok(mut hangup) = signal(SignalKind::hangup()) else { return };
        while hangup.recv().await.is_some() {
            match zcie_security::ClientsFile::load(&clients_path) {
                Ok(clients) => {
                    engine.reload_clients(clients);
                    tracing::info!("clients.toml reloaded");
                }
                Err(error) => tracing::warn!(%error, "clients.toml reload failed"),
            }
        }
    });
}
