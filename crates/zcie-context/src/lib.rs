//! Minimal context: token estimates, the token firewall and cursors.
//! See `docs/09-query-and-context.md` sections 6 and 9.

pub mod budget;
pub mod capsule;
pub mod refs;

pub use budget::{MIN_BUDGET, clamp_budget, estimate_tokens, json_tokens};
pub use capsule::{Capsule, build_capsule, ref_only};
pub use refs::{make_cursor, parse_cursor};
