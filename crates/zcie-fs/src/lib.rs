//! File discovery, hashing, binary detection and the file watcher.

pub mod binary;
pub mod discover;
pub mod error;
pub mod hash;
pub mod watcher;

pub use binary::{BINARY_PROBE_BYTES, is_binary, is_document, language_for_path};
pub use discover::{DiscoveredFile, discover_files, files_from_list, relative_path};
pub use error::{Error, Result};
pub use hash::{hash_bytes, hash_file, read_head};
pub use watcher::{ChangeKind, FileChange, FileWatcher};
