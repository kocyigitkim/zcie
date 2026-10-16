//! L0 in-process cache. Keys contain the principal, the workspace and the watermarks,
//! so a cache hit never bypasses the scope or returns data older than the index state.

pub mod cache_key;
pub mod l0;

pub use cache_key::{CacheKeyParts, cache_key};
pub use l0::{L0Cache, PAGE_TTL, StoredPage};
