//! The current (not deleted) files of a worktree. A deleted file keeps its path lookup (its ID
//! comes back when the file comes back), so a count of path keys also counts deleted files.

use zcie_types::FileRecord;

use crate::error::Result;
use crate::values::FileIdValue;
use crate::{Cf, Store, layout};

/// Returns the relative paths of the current files of a worktree, at most `limit`.
pub fn current_paths(store: &Store, tenant: &str, worktree_id: &str, limit: usize) -> Result<Vec<String>> {
    Ok(current_files(store, tenant, worktree_id, limit)?.into_iter().map(|(path, _)| path).collect())
}

/// Returns (relative path, file ID) of the current files of a worktree, at most `limit`.
pub fn current_files(store: &Store, tenant: &str, worktree_id: &str, limit: usize) -> Result<Vec<(String, String)>> {
    let mut out = Vec::new();
    for (key, value) in store.prefix_scan::<FileIdValue>(Cf::Lookup, &layout::path_prefix(tenant, worktree_id), limit)? {
        let record: Option<FileRecord> = store.get(Cf::Entities, &layout::entity(tenant, "file", &value.file_id))?;
        if record.is_some_and(|r| !r.deleted) {
            out.push((crate::keys::last_part(&key), value.file_id));
        }
    }
    Ok(out)
}
