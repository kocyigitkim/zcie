//! Core API of ZCIE: the `Engine` struct with all operations. No transport code.
//! See `docs/10-api-and-gateways.md` section 2.

pub mod engine;
pub mod error;
pub mod forget;
pub mod multi;
pub mod project_status;
pub mod projects;
pub mod read_ops;
pub mod write_ops;

pub use engine::{ApiConfig, Engine, EngineParts, WorktreeHook};
pub use error::{ApiError, ApiResult};
/// Term coverage (ADR-009). It lives in `zcie-query`; the path `zcie_api::relevance` stays for clients.
pub use zcie_query::relevance;
