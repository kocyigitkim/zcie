//! Core API of ZCIE: the `Engine` struct with all operations. No transport code.
//! See `docs/10-api-and-gateways.md` section 2.

pub mod engine;
pub mod error;
pub mod forget;
pub mod read_ops;
pub mod write_ops;

pub use engine::{ApiConfig, Engine, EngineParts, WorktreeHook};
pub use error::{ApiError, ApiResult};
