//! Symbol records per file and name lookup.

use std::collections::HashSet;

use zcie_parse::ParsedFile;
use zcie_store::values::Marker;
use zcie_store::{Batch, Cf, Store, layout};
use zcie_types::{FileRecord, SymbolId, SymbolRecord};

use crate::error::Result;
use crate::identity::symbol_id_for;

/// The result of a symbol update for one file.
#[derive(Debug, Default)]
pub struct SymbolUpdate {
    /// Current symbols of the file.
    pub symbols: Vec<SymbolRecord>,
    /// Symbols that the new version does not have.
    pub removed: Vec<SymbolRecord>,
}

/// Maximum number of symbols of one file.
const MAX_SYMBOLS_PER_FILE: usize = 100_000;

/// Reads the current symbols of a file.
pub fn file_symbols(store: &Store, tenant: &str, file_id: &str) -> Result<Vec<SymbolRecord>> {
    let keys = store.prefix_keys(Cf::Lookup, &layout::file_symbols_prefix(tenant, file_id), MAX_SYMBOLS_PER_FILE)?;
    let entity_keys: Vec<Vec<u8>> = keys
        .iter()
        .map(|k| layout::entity(tenant, "sym", &zcie_store::keys::last_part(k)))
        .collect();
    let records = store.multi_get::<SymbolRecord>(Cf::Entities, &entity_keys)?;
    Ok(records.into_iter().flatten().collect())
}

/// Stores the symbols of a parsed file. Removes symbols that are not in the new version.
pub fn upsert_file_symbols(store: &Store, batch: &mut Batch, file: &FileRecord, parsed: &ParsedFile) -> Result<SymbolUpdate> {
    let tenant = file.scope.tenant_id.clone();
    let worktree_id = file.scope.worktree_id.clone().unwrap_or_default();
    let old = file_symbols(store, &tenant, file.file_id.as_str())?;
    let mut update = SymbolUpdate::default();
    let mut kept: HashSet<SymbolId> = HashSet::new();
    for parsed_symbol in &parsed.symbols {
        let symbol_id = symbol_id_for(
            store,
            batch,
            &tenant,
            &worktree_id,
            &file.path,
            &parsed_symbol.qualified_name,
            parsed_symbol.kind,
        )?;
        if !kept.insert(symbol_id.clone()) {
            continue;
        }
        let record = make_record(file, parsed, parsed_symbol, symbol_id);
        write_symbol(batch, &record)?;
        update.symbols.push(record);
    }
    for symbol in old.into_iter().filter(|s| !kept.contains(&s.symbol_id)) {
        delete_symbol(batch, &symbol);
        update.removed.push(symbol);
    }
    Ok(update)
}

/// Removes all symbols of a file. Returns the removed symbols.
pub fn remove_file_symbols(store: &Store, batch: &mut Batch, tenant: &str, file_id: &str) -> Result<Vec<SymbolRecord>> {
    let old = file_symbols(store, tenant, file_id)?;
    for symbol in &old {
        delete_symbol(batch, symbol);
    }
    Ok(old)
}

fn make_record(file: &FileRecord, parsed: &ParsedFile, symbol: &zcie_parse::ParsedSymbol, symbol_id: SymbolId) -> SymbolRecord {
    SymbolRecord {
        v: 1,
        symbol_id,
        scope: file.scope.clone(),
        file_id: file.file_id.clone(),
        path: file.path.clone(),
        language: file.language.clone().unwrap_or_default(),
        name: symbol.name.clone(),
        qualified_name: symbol.qualified_name.clone(),
        kind: symbol.kind,
        signature: symbol.signature.clone(),
        span: symbol.span,
        file_hash: file.content_hash.clone(),
        parser_quality: parsed.quality,
    }
}

fn write_symbol(batch: &mut Batch, record: &SymbolRecord) -> Result<()> {
    let tenant = &record.scope.tenant_id;
    let id = record.symbol_id.as_str();
    batch.put(Cf::Entities, layout::entity(tenant, "sym", id), record)?;
    let lower = record.name.to_lowercase();
    batch.put(Cf::Lookup, layout::symbol_name(tenant, &record.scope.workspace_id, &lower, id), &Marker { v: 1 })?;
    batch.put(Cf::Lookup, layout::file_symbol(tenant, record.file_id.as_str(), id), &Marker { v: 1 })?;
    Ok(())
}

fn delete_symbol(batch: &mut Batch, record: &SymbolRecord) {
    let tenant = &record.scope.tenant_id;
    let id = record.symbol_id.as_str();
    batch.delete(Cf::Entities, layout::entity(tenant, "sym", id));
    let lower = record.name.to_lowercase();
    batch.delete(Cf::Lookup, layout::symbol_name(tenant, &record.scope.workspace_id, &lower, id));
    batch.delete(Cf::Lookup, layout::file_symbol(tenant, record.file_id.as_str(), id));
    let worktree_id = record.scope.worktree_id.clone().unwrap_or_default();
    let identity = layout::symbol_identity(tenant, &worktree_id, &record.path, &record.qualified_name, record.kind.name());
    batch.delete(Cf::Lookup, identity);
}

/// Reads one symbol by ID.
pub fn get_symbol(store: &Store, tenant: &str, symbol_id: &str) -> Result<Option<SymbolRecord>> {
    Ok(store.get(Cf::Entities, &layout::entity(tenant, "sym", symbol_id))?)
}

/// Finds symbols by simple or qualified name (case-insensitive) in one workspace.
pub fn find_by_name(store: &Store, tenant: &str, workspace_id: &str, name: &str, limit: usize) -> Result<Vec<SymbolRecord>> {
    let lower = name.to_lowercase();
    let simple = lower.rsplit(['.', ':']).next().unwrap_or(&lower).to_string();
    let prefix = layout::symbol_name_prefix(tenant, workspace_id, &simple);
    let keys = store.prefix_keys(Cf::Lookup, &prefix, limit.saturating_mul(4).max(limit))?;
    let entity_keys: Vec<Vec<u8>> = keys
        .iter()
        .map(|k| layout::entity(tenant, "sym", &zcie_store::keys::last_part(k)))
        .collect();
    let qualified = simple != lower;
    let mut out: Vec<SymbolRecord> = store
        .multi_get::<SymbolRecord>(Cf::Entities, &entity_keys)?
        .into_iter()
        .flatten()
        .filter(|s| !qualified || s.qualified_name.to_lowercase().ends_with(&lower.replace("::", ".")) || s.qualified_name.to_lowercase().ends_with(&lower))
        .collect();
    out.truncate(limit);
    Ok(out)
}
