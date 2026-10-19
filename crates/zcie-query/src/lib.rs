//! Read path of ZCIE: classification, retrieval routes, ranking, freshness, local answers,
//! the token firewall and expand. No language model is called. See `docs/09-query-and-context.md`.

mod calls;
pub mod classify;
pub mod compress;
pub mod capsule_rules;
mod dedupe;
mod followups;
pub mod synonyms;
pub mod error;
pub mod evidence;
pub mod expand;
mod expand_batch;
pub mod freshness;
pub mod fusion;
mod graph_rank;
pub mod intent;
pub mod items;
pub mod listing;
pub mod locate;
pub mod overview;
mod pagination;
pub mod planner;
pub mod relevance;
pub mod repomap;
pub mod routes;
mod trace;

pub use classify::{classify, normalize};
pub use error::{Error, Result};
pub use items::{Loaded, load_ref};
pub use planner::{QueryConfig, QueryContext, QueryEngine};
