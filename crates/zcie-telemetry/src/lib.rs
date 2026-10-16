//! Logging setup, timers and counters for ZCIE.

pub mod error;
pub mod logging;
pub mod timer;

pub use error::{Error, Result};
pub use logging::init_logging;
pub use timer::{Counters, Timer};
