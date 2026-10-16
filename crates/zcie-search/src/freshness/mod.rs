//! Freshness labels (`docs/09-query-and-context.md` section 7).

use zcie_types::{Freshness, Watermarks};

/// Returns the freshness label from the watermarks and the overlay state of the results.
///
/// `overlay_hits` is the number of results in the live overlay. `unconfirmed` is the number
/// of those results that the engine could not confirm from the current file.
pub fn freshness(watermarks: &Watermarks, overlay_hits: usize, unconfirmed: usize) -> Freshness {
    if unconfirmed > 0 {
        return Freshness::Stale;
    }
    let behind = watermarks.search_seq < watermarks.canonical_seq || watermarks.graph_seq < watermarks.canonical_seq;
    if behind || overlay_hits > 0 { Freshness::Partial } else { Freshness::Current }
}
