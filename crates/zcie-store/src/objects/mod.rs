//! Content-addressed blob store (`docs/06-storage.md` sections 5 and 11).

use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use crate::error::{Error, Result};
use crate::layout;
use crate::rocksdb::{Cf, Store};

/// Zstd compression level for blobs.
const ZSTD_LEVEL: i32 = 3;

/// Folder of compressed blobs: `objects/<hex[0..2]>/<hex[2..4]>/<hex>.zst`.
pub struct ObjectStore {
    root: PathBuf,
}

impl ObjectStore {
    /// Makes an object store at `root`. Creates the folder.
    pub fn new(root: PathBuf) -> Result<ObjectStore> {
        fs::create_dir_all(&root)?;
        Ok(ObjectStore { root })
    }

    /// Returns the BLAKE3 content hash `b3:<hex>` of bytes.
    pub fn hash(bytes: &[u8]) -> String {
        format!("b3:{}", blake3::hash(bytes).to_hex())
    }

    fn path_for(&self, hash: &str) -> Result<PathBuf> {
        let hex = hash.strip_prefix("b3:").unwrap_or(hash);
        if hex.len() != 64 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(Error::Invalid(format!("bad content hash: {hash}")));
        }
        Ok(self.root.join(&hex[0..2]).join(&hex[2..4]).join(format!("{hex}.zst")))
    }

    /// Returns true if the blob exists.
    pub fn exists(&self, hash: &str) -> bool {
        self.path_for(hash).map(|p| p.exists()).unwrap_or(false)
    }

    /// Stores bytes durably and returns the content hash. Writes a blob only once.
    pub fn put(&self, bytes: &[u8]) -> Result<String> {
        let hash = Self::hash(bytes);
        let final_path = self.path_for(&hash)?;
        if final_path.exists() {
            return Ok(hash);
        }
        let folder = final_path.parent().ok_or_else(|| Error::Invalid(hash.clone()))?;
        fs::create_dir_all(folder)?;
        let temp_path = folder.join(format!("{}.tmp-{}", hash.trim_start_matches("b3:"), ulid::Ulid::generate()));
        let compressed = zstd::encode_all(bytes, ZSTD_LEVEL)?;
        let mut file = File::create(&temp_path)?;
        file.write_all(&compressed)?;
        file.sync_all()?;
        fs::rename(&temp_path, &final_path)?;
        File::open(folder)?.sync_all()?;
        Ok(hash)
    }

    /// Reads a blob and checks its hash.
    pub fn get(&self, hash: &str) -> Result<Vec<u8>> {
        let path = self.path_for(hash)?;
        if !path.exists() {
            return Err(Error::NotFound(hash.to_string()));
        }
        let bytes = zstd::decode_all(File::open(path)?)?;
        if Self::hash(&bytes) != hash {
            return Err(Error::HashMismatch(hash.to_string()));
        }
        Ok(bytes)
    }

    /// Reads a blob as UTF-8 text (lossy).
    pub fn get_text(&self, hash: &str) -> Result<String> {
        Ok(String::from_utf8_lossy(&self.get(hash)?).into_owned())
    }

    /// Deletes a blob. A missing blob is not an error.
    pub fn delete(&self, hash: &str) -> Result<()> {
        let path = self.path_for(hash)?;
        if path.exists() {
            fs::remove_file(path)?;
        }
        Ok(())
    }

    /// Deletes temporary files of interrupted writes (recovery step 6).
    pub fn remove_temp_files(&self) -> Result<u64> {
        let mut removed = 0;
        for file in self.all_files()? {
            let name = file.file_name().and_then(|n| n.to_str()).unwrap_or_default();
            if name.contains(".tmp-") {
                fs::remove_file(&file)?;
                removed += 1;
            }
        }
        Ok(removed)
    }

    /// Deletes blobs with no `lookup` reference that are older than `min_age`.
    pub fn sweep_orphans(&self, store: &Store, tenant: &str, min_age: Duration) -> Result<u64> {
        let mut removed = 0;
        for file in self.all_files()? {
            let name = file.file_name().and_then(|n| n.to_str()).unwrap_or_default();
            let Some(hex) = name.strip_suffix(".zst") else { continue };
            let hash = format!("b3:{hex}");
            if store.exists(Cf::Lookup, &layout::blob(tenant, &hash))? || !is_older_than(&file, min_age) {
                continue;
            }
            fs::remove_file(&file)?;
            removed += 1;
        }
        Ok(removed)
    }

    /// Returns the total size of all blobs in bytes.
    pub fn total_bytes(&self) -> u64 {
        self.all_files()
            .unwrap_or_default()
            .iter()
            .filter_map(|f| f.metadata().ok())
            .map(|m| m.len())
            .sum()
    }

    fn all_files(&self) -> Result<Vec<PathBuf>> {
        let mut out = Vec::new();
        for level1 in read_dirs(&self.root)? {
            for level2 in read_dirs(&level1)? {
                for entry in fs::read_dir(&level2)? {
                    let path = entry?.path();
                    if path.is_file() {
                        out.push(path);
                    }
                }
            }
        }
        Ok(out)
    }
}

fn read_dirs(path: &Path) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    for entry in fs::read_dir(path)? {
        let path = entry?.path();
        if path.is_dir() {
            out.push(path);
        }
    }
    Ok(out)
}

fn is_older_than(path: &Path, min_age: Duration) -> bool {
    let modified = path.metadata().and_then(|m| m.modified()).unwrap_or_else(|_| SystemTime::now());
    SystemTime::now().duration_since(modified).map(|age| age >= min_age).unwrap_or(false)
}
