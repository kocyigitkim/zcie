//! Single-writer lock and PID file (`docs/adr/ADR-003-single-writer-daemon.md`).

use std::fs::{File, OpenOptions};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

use anyhow::{Context, bail};

/// The held daemon lock. Dropping it releases the lock and removes the PID file.
pub struct DaemonLock {
    _file: File,
    pid_path: PathBuf,
}

impl DaemonLock {
    /// Takes the exclusive lock on `run/daemon.lock` and writes `run/zcied.pid`.
    pub fn acquire(run_dir: &Path) -> anyhow::Result<DaemonLock> {
        std::fs::create_dir_all(run_dir)?;
        let lock_path = run_dir.join("daemon.lock");
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .mode(0o600)
            .open(&lock_path)
            .with_context(|| format!("cannot open {}", lock_path.display()))?;
        if file.try_lock().is_err() {
            bail!("another zcied holds {}; stop it first (zcie daemon stop)", lock_path.display());
        }
        let pid_path = run_dir.join("zcied.pid");
        std::fs::write(&pid_path, std::process::id().to_string())?;
        zcie_core::set_mode(&pid_path, 0o600)?;
        Ok(DaemonLock { _file: file, pid_path })
    }
}

impl Drop for DaemonLock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.pid_path);
    }
}
