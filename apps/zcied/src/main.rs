//! `zcied`: the ZCIE daemon. It owns the database and serves the HTTP API.

mod lock;
mod signals;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, bail};
use clap::Parser;
use zcie_core::{EngineConfig, InstancePaths, default_home, start_engine};

/// Command-line arguments of `zcied`.
#[derive(Debug, Parser)]
#[command(name = "zcied", version, about = "ZeroContext Intelligence Engine daemon")]
struct Args {
    /// ZCIE home folder. Default `~/.zcie`.
    #[arg(long, env = "ZCIE_HOME")]
    home: Option<PathBuf>,
    /// Configuration file. Default `<home>/config/engine.toml`.
    #[arg(long)]
    config: Option<PathBuf>,
    /// Instance folder. Overrides `storage.data_dir`.
    #[arg(long)]
    data_dir: Option<PathBuf>,
    /// Run in the foreground. The daemon always runs in the foreground; `zcie daemon start` detaches it.
    #[arg(long)]
    foreground: bool,
    /// Log level (`error`, `warn`, `info`, `debug`, `trace`).
    #[arg(long, default_value = "info")]
    log_level: String,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let home = args.home.clone().unwrap_or_else(default_home);
    let config_path = args.config.clone().unwrap_or_else(|| home.join("config").join("engine.toml"));
    let config = EngineConfig::load(&config_path)?;
    let data = args.data_dir.clone().unwrap_or_else(|| config.data_dir(&home));
    let paths = InstancePaths { home, data };
    let addr: SocketAddr = config.server.http_addr.parse().context("server.http_addr is not an address")?;
    if !addr.ip().is_loopback() && !config.server.allow_remote {
        bail!("server.http_addr {addr} is not a loopback address; set server.allow_remote = true to permit it");
    }
    std::fs::create_dir_all(paths.logs())?;
    zcie_telemetry::init_logging(&args.log_level, Some(&paths.logs()));
    let _lock = lock::DaemonLock::acquire(&paths.run_dir())?;
    let running = start_engine(paths.clone(), config.clone())?;
    if let Some(key) = &running.new_admin_key {
        announce_admin_key(&paths, key)?;
    }
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().build()?;
    let engine = Arc::clone(&running.engine);
    let max_payload = config.ingest.max_payload_bytes;
    let result = runtime.block_on(async move {
        signals::spawn_reload_on_hangup(Arc::clone(&engine), paths.clients_toml());
        zcie_gateway_http::serve(engine, addr, max_payload, signals::stop_signal(paths.run_dir())).await
    });
    tracing::info!("HTTP gateway stopped; shutting down");
    running.shutdown();
    runtime.shutdown_timeout(std::time::Duration::from_secs(2));
    result.context("HTTP gateway failed")
}

/// Prints a new admin key once and stores it in `config/admin.key` (mode 0600) for the local CLI.
fn announce_admin_key(paths: &InstancePaths, key: &str) -> anyhow::Result<()> {
    let key_file = paths.home.join("config").join("admin.key");
    zcie_core::write_private(&key_file, key.as_bytes())?;
    println!("ZCIE admin API key (shown once): {key}");
    println!("The key is also stored in {} (mode 0600).", key_file.display());
    Ok(())
}
