//! API keys, scope checks, secret redaction and exclusion globs (`docs/11-security.md`).

pub mod auth;
pub mod error;
pub mod exclude;
pub mod redact;
pub mod scope_check;

pub use auth::{
    AuthContext, ClientEntry, ClientsFile, Permission, authenticate, generate_key, hash_key,
};
pub use error::{Error, Result};
pub use exclude::ExcludeSet;
pub use redact::redact;
pub use scope_check::{can_read, can_write, require};
