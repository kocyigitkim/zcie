//! Shared test helpers of the ZCIE test phase (`docs/13-test-plan.md`).
//!
//! Tests may use `unwrap` and `expect` (rule C-07 applies to production code only).
#![allow(clippy::unwrap_used, clippy::expect_used)]

pub mod engine;
pub mod fixture;
pub mod process;

pub use engine::{TestEngine, admin, read_only_auth, workspace_auth};
pub use fixture::{commit_all, fixture_repo, git};
pub use process::{DaemonProcess, free_port, mcp_session, workspace_root};
