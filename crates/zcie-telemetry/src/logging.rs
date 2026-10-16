//! Log setup with `tracing-subscriber`.

use std::fs::{File, OpenOptions};
use std::path::Path;
use std::sync::{Arc, Mutex};

use tracing_subscriber::EnvFilter;
use tracing_subscriber::fmt::writer::{BoxMakeWriter, MakeWriterExt};

/// Name of the log file in the log folder.
pub const LOG_FILE_NAME: &str = "zcied.log";

/// Starts logging to stderr, and to `log_dir/zcied.log` if `log_dir` is set.
///
/// `RUST_LOG` replaces `level` if it is set. A second call does nothing.
pub fn init_logging(level: &str, log_dir: Option<&Path>) {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(format!("{level},tantivy=warn")));
    let writer = match log_dir.and_then(open_log_file) {
        Some(file) => {
            let shared = Arc::new(Mutex::new(file));
            let file_writer = move || SharedFile(Arc::clone(&shared));
            BoxMakeWriter::new(std::io::stderr.and(file_writer))
        }
        None => BoxMakeWriter::new(std::io::stderr),
    };
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(writer)
        .with_ansi(false)
        .try_init();
}

fn open_log_file(dir: &Path) -> Option<File> {
    std::fs::create_dir_all(dir).ok()?;
    OpenOptions::new().create(true).append(true).open(dir.join(LOG_FILE_NAME)).ok()
}

/// A log file shared between threads.
struct SharedFile(Arc<Mutex<File>>);

impl std::io::Write for SharedFile {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match self.0.lock() {
            Ok(mut file) => file.write(buf),
            Err(_) => Ok(buf.len()),
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        match self.0.lock() {
            Ok(mut file) => file.flush(),
            Err(_) => Ok(()),
        }
    }
}
