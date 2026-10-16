//! Engine bootstrap: configuration, runtime tree, recovery, workers, watchers and maintenance.
//! See `docs/06-storage.md` section 8 and `docs/15-configuration.md`.

pub mod clients;
pub mod config;
pub mod error;
pub mod maintenance;
pub mod startup;
pub mod tree;
pub mod watchers;

pub use config::{EngineConfig, default_home, expand_home};
pub use error::{Error, Result};
pub use startup::{RunningEngine, start_engine};
pub use tree::{InstancePaths, set_mode, write_private};
