//! Typed knowledge graph: edges in both directions, status rules and bounded traversal.
//! See `docs/08-memory-and-graph.md` sections 5 and 8.

pub mod edges;
pub mod error;
pub mod status;
pub mod traverse;

pub use edges::{delete_all_edges_of, delete_edge, delete_edges_from, edges_from, edges_to, new_edge, put_edge};
pub use error::{Error, Result};
pub use status::{effective_status, merged_status};
pub use traverse::{Direction, PathItem, TraverseOptions, neighbors};
