//! Cache keys (`docs/09-query-and-context.md` section 8).

/// All parts that make a cache key. A change of any part gives a new key.
#[derive(Debug, Clone, Default)]
pub struct CacheKeyParts<'a> {
    /// Tenant ID.
    pub tenant_id: &'a str,
    /// Principal ID.
    pub principal_id: &'a str,
    /// Workspace ID.
    pub workspace_id: &'a str,
    /// Repository ID or empty.
    pub repository_id: &'a str,
    /// Policy version of the auth context.
    pub policy_version: u64,
    /// Normalized question.
    pub question: &'a str,
    /// Query mode name.
    pub mode: &'a str,
    /// Token budget.
    pub token_budget: u32,
    /// Maximum results.
    pub max_results: u32,
    /// Canonical watermark at query time.
    pub canonical_seq: u64,
    /// Graph watermark at query time.
    pub graph_seq: u64,
    /// Sequence of the last file change of the workspace (`lookup: t/code_seq/<ws>`): a file
    /// that an editor changed makes no event, so the canonical watermark does not move (D-078).
    pub code_seq: u64,
}

/// Returns the BLAKE3 hex hash of all key parts.
pub fn cache_key(parts: &CacheKeyParts<'_>) -> String {
    let mut hasher = blake3::Hasher::new();
    for text in [parts.tenant_id, parts.principal_id, parts.workspace_id, parts.repository_id, parts.question, parts.mode] {
        hasher.update(text.as_bytes());
        hasher.update(&[0]);
    }
    for number in [parts.policy_version, u64::from(parts.token_budget), u64::from(parts.max_results), parts.canonical_seq, parts.graph_seq, parts.code_seq] {
        hasher.update(&number.to_le_bytes());
    }
    hasher.finalize().to_hex().to_string()
}
