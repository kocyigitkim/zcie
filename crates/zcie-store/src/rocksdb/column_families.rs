//! The nine column families (`docs/06-storage.md` section 3).

/// A RocksDB column family.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Cf {
    /// Principals, projects, repositories, counters.
    Registry,
    /// Immutable event envelopes.
    Events,
    /// Files, symbols, memory records, tasks, commits.
    Entities,
    /// Forward graph adjacency.
    EdgesOut,
    /// Reverse graph adjacency.
    EdgesIn,
    /// File states, watermarks.
    Revisions,
    /// Lookup maps.
    Lookup,
    /// Durable job records.
    Jobs,
    /// Tombstones and capture policies.
    Policies,
}

impl Cf {
    /// All column families.
    pub const ALL: [Cf; 9] = [
        Cf::Registry,
        Cf::Events,
        Cf::Entities,
        Cf::EdgesOut,
        Cf::EdgesIn,
        Cf::Revisions,
        Cf::Lookup,
        Cf::Jobs,
        Cf::Policies,
    ];

    /// Returns the RocksDB name of the column family.
    pub fn name(&self) -> &'static str {
        match self {
            Cf::Registry => "registry",
            Cf::Events => "events",
            Cf::Entities => "entities",
            Cf::EdgesOut => "edges_out",
            Cf::EdgesIn => "edges_in",
            Cf::Revisions => "revisions",
            Cf::Lookup => "lookup",
            Cf::Jobs => "jobs",
            Cf::Policies => "policies",
        }
    }
}
