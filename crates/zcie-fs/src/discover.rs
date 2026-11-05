//! File discovery with ignore rules and exclusion globs.

use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use zcie_security::ExcludeSet;

use crate::error::{Error, Result};

/// One discovered file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredFile {
    /// Path relative to the root, with `/` separators.
    pub rel_path: String,
    /// Absolute path.
    pub abs_path: PathBuf,
    /// Size in bytes.
    pub size: u64,
    /// Modification time in nanoseconds since the Unix epoch; 0 if unknown (ADR-032).
    pub mtime_ns: u64,
}

/// Returns the modification time of a file in nanoseconds since the Unix epoch; 0 if unknown.
pub fn mtime_ns(meta: &std::fs::Metadata) -> u64 {
    meta.modified().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map_or(0, |d| d.as_nanos() as u64)
}

/// Returns the path of `abs` relative to `root`, with `/` separators.
pub fn relative_path(root: &Path, abs: &Path) -> Option<String> {
    let rel = abs.strip_prefix(root).ok()?;
    let parts: Vec<String> = rel
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect();
    if parts.is_empty() { None } else { Some(parts.join("/")) }
}

/// Walks a folder. Applies `.gitignore` rules and the exclusion globs. Sorted by path.
pub fn discover_files(root: &Path, exclude: &ExcludeSet) -> Result<Vec<DiscoveredFile>> {
    let walker = ignore::WalkBuilder::new(root)
        .hidden(false)
        .git_ignore(true)
        .require_git(false)
        .follow_links(false)
        .build();
    let mut files = Vec::new();
    for entry in walker {
        let entry = entry.map_err(|e| Error::Walk(e.to_string()))?;
        if !entry.file_type().is_some_and(|t| t.is_file()) {
            continue;
        }
        let Some(rel_path) = relative_path(root, entry.path()) else { continue };
        if exclude.is_excluded(&rel_path) {
            continue;
        }
        let meta = entry.metadata().ok();
        let (size, mtime_ns) = meta.as_ref().map_or((0, 0), |m| (m.len(), mtime_ns(m)));
        files.push(DiscoveredFile { rel_path, abs_path: entry.path().to_path_buf(), size, mtime_ns });
    }
    files.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));
    Ok(files)
}

/// Stats a list of relative paths. Skips missing, excluded and outside-root files.
pub fn files_from_list(root: &Path, rel_paths: &[String], exclude: &ExcludeSet) -> Vec<DiscoveredFile> {
    let canonical_root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let mut files = Vec::new();
    for rel_path in rel_paths {
        if exclude.is_excluded(rel_path) {
            continue;
        }
        let abs_path = root.join(rel_path);
        if let Some(meta) = regular_file_meta(&canonical_root, &abs_path) {
            files.push(DiscoveredFile { rel_path: rel_path.clone(), abs_path, size: meta.len(), mtime_ns: mtime_ns(&meta) });
        }
    }
    files.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));
    files
}

fn regular_file_meta(canonical_root: &Path, abs_path: &Path) -> Option<std::fs::Metadata> {
    let link_meta = std::fs::symlink_metadata(abs_path).ok()?;
    if link_meta.file_type().is_symlink() {
        let target = abs_path.canonicalize().ok()?;
        if !target.starts_with(canonical_root) {
            return None;
        }
    }
    let meta = std::fs::metadata(abs_path).ok()?;
    if meta.is_file() { Some(meta) } else { None }
}
