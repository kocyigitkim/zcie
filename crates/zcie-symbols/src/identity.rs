//! Stable symbol identity (`docs/05-data-model.md` section 5).

use zcie_store::{Batch, Cf, Store, layout};
use zcie_types::{SymbolId, SymbolKind};

use crate::error::Result;

/// Lookup value that maps a symbol identity to its ID.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SymbolIdentity {
    /// Value schema version.
    pub v: u32,
    /// The symbol ID.
    pub symbol_id: SymbolId,
}

/// Returns the stable ID for `(worktree, path, qualified name, kind)`. Makes a new ID if none exists.
pub fn symbol_id_for(
    store: &Store,
    batch: &mut Batch,
    tenant: &str,
    worktree_id: &str,
    path: &str,
    qualified_name: &str,
    kind: SymbolKind,
) -> Result<SymbolId> {
    let key = layout::symbol_identity(tenant, worktree_id, path, qualified_name, kind.name());
    if let Some(found) = batch.pending::<SymbolIdentity>(Cf::Lookup, &key) {
        return Ok(found.symbol_id);
    }
    if let Some(found) = store.get::<SymbolIdentity>(Cf::Lookup, &key)? {
        return Ok(found.symbol_id);
    }
    let symbol_id = SymbolId::new();
    batch.put(Cf::Lookup, key, &SymbolIdentity { v: 1, symbol_id: symbol_id.clone() })?;
    Ok(symbol_id)
}
