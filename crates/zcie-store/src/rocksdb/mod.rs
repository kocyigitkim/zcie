//! RocksDB access: open, read, scan, batch write and snapshots (`docs/06-storage.md` sections 3 and 4).

pub mod column_families;

use std::path::Path;
use std::sync::Mutex;

use rocksdb::{BlockBasedOptions, Cache, ColumnFamily, ColumnFamilyDescriptor, DB, Direction, IteratorMode, Options, WriteBatch, WriteOptions};
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::codec::{decode, encode};
use crate::error::{Error, Result};
pub use column_families::Cf;

/// One key and its decoded value from a scan.
pub type ScanItem<T> = (Vec<u8>, T);

/// The canonical store. Only the daemon opens it (ADR-003).
pub struct Store {
    db: DB,
    /// Serializes watermark read-compare-write operations.
    pub(crate) watermark_lock: Mutex<()>,
}

/// A list of puts and deletes that the store writes atomically.
#[derive(Debug, Default)]
pub struct Batch {
    ops: Vec<(Cf, Vec<u8>, Option<Vec<u8>>)>,
}

impl Batch {
    /// Makes an empty batch.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a put of an encoded value.
    pub fn put<T: Serialize>(&mut self, cf: Cf, key: Vec<u8>, value: &T) -> Result<()> {
        self.ops.push((cf, key, Some(encode(value)?)));
        Ok(())
    }

    /// Adds a delete.
    pub fn delete(&mut self, cf: Cf, key: Vec<u8>) {
        self.ops.push((cf, key, None));
    }

    /// Returns true if the batch has no operations.
    pub fn is_empty(&self) -> bool {
        self.ops.is_empty()
    }

    /// Returns the number of operations.
    pub fn len(&self) -> usize {
        self.ops.len()
    }

    /// Moves all operations of `other` into this batch.
    pub fn append(&mut self, other: Batch) {
        self.ops.extend(other.ops);
    }

    /// Returns the value of the last put for a key in this batch, if any.
    pub fn pending<T: DeserializeOwned>(&self, cf: Cf, key: &[u8]) -> Option<T> {
        let found = self.ops.iter().rev().find(|(c, k, _)| *c == cf && k == key)?;
        found.2.as_ref().and_then(|bytes| decode(bytes).ok())
    }
}

/// Memory of the memtables of all column families together.
const TOTAL_WRITE_BUFFER_BYTES: usize = 64 * 1024 * 1024;
/// Memtable size of one column family.
const WRITE_BUFFER_BYTES: usize = 16 * 1024 * 1024;
/// Memtables of one column family before a write waits for a flush.
const MAX_WRITE_BUFFERS: i32 = 2;
/// Block cache shared by all column families.
const BLOCK_CACHE_BYTES: usize = 32 * 1024 * 1024;

/// Options of one column family: small memtables and the shared block cache.
fn column_family_options(cache: &Cache) -> Options {
    let mut options = Options::default();
    options.set_write_buffer_size(WRITE_BUFFER_BYTES);
    options.set_max_write_buffer_number(MAX_WRITE_BUFFERS);
    let mut table = BlockBasedOptions::default();
    table.set_block_cache(cache);
    options.set_block_based_table_factory(&table);
    options
}

impl Store {
    /// Opens the database with all column families. Creates missing parts.
    pub fn open(path: &Path) -> Result<Store> {
        let mut options = Options::default();
        options.create_if_missing(true);
        options.create_missing_column_families(true);
        // One memory limit for the memtables of all column families: a write burst (for example
        // the first index of many repositories) flushes to disk instead of growing the memory.
        options.set_db_write_buffer_size(TOTAL_WRITE_BUFFER_BYTES);
        let cache = Cache::new_lru_cache(BLOCK_CACHE_BYTES);
        let descriptors = Cf::ALL.iter().map(|cf| ColumnFamilyDescriptor::new(cf.name(), column_family_options(&cache)));
        let db = DB::open_cf_descriptors(&options, path, descriptors)?;
        Ok(Store { db, watermark_lock: Mutex::new(()) })
    }

    fn handle(&self, cf: Cf) -> Result<&ColumnFamily> {
        self.db.cf_handle(cf.name()).ok_or(Error::MissingCf(cf.name()))
    }

    /// Reads and decodes one value.
    pub fn get<T: DeserializeOwned>(&self, cf: Cf, key: &[u8]) -> Result<Option<T>> {
        match self.db.get_pinned_cf(self.handle(cf)?, key)? {
            Some(bytes) => Ok(Some(decode(&bytes)?)),
            None => Ok(None),
        }
    }

    /// Returns true if the key exists.
    pub fn exists(&self, cf: Cf, key: &[u8]) -> Result<bool> {
        Ok(self.db.get_pinned_cf(self.handle(cf)?, key)?.is_some())
    }

    /// Reads many values of one column family.
    pub fn multi_get<T: DeserializeOwned>(&self, cf: Cf, keys: &[Vec<u8>]) -> Result<Vec<Option<T>>> {
        let handle = self.handle(cf)?;
        let results = self.db.multi_get_cf(keys.iter().map(|k| (handle, k.as_slice())));
        let mut out = Vec::with_capacity(results.len());
        for result in results {
            out.push(match result? {
                Some(bytes) => Some(decode(&bytes)?),
                None => None,
            });
        }
        Ok(out)
    }

    /// Writes one value with a sync write.
    pub fn put_sync<T: Serialize>(&self, cf: Cf, key: &[u8], value: &T) -> Result<()> {
        let mut batch = Batch::new();
        batch.put(cf, key.to_vec(), value)?;
        self.write_batch(batch, true)
    }

    /// Writes a batch atomically. `sync` waits for the WAL fsync.
    pub fn write_batch(&self, batch: Batch, sync: bool) -> Result<()> {
        let mut write = WriteBatch::default();
        for (cf, key, value) in batch.ops {
            let handle = self.handle(cf)?;
            match value {
                Some(bytes) => write.put_cf(handle, key, bytes),
                None => write.delete_cf(handle, key),
            }
        }
        let mut options = WriteOptions::default();
        options.set_sync(sync);
        self.db.write_opt(write, &options)?;
        Ok(())
    }

    /// Scans keys that start with `prefix`. Returns at most `limit` items in key order.
    pub fn prefix_scan<T: DeserializeOwned>(&self, cf: Cf, prefix: &[u8], limit: usize) -> Result<Vec<ScanItem<T>>> {
        self.scan_from(cf, prefix, prefix, limit)
    }

    /// Scans keys that start with `prefix`, beginning at `start`.
    pub fn scan_from<T: DeserializeOwned>(
        &self,
        cf: Cf,
        prefix: &[u8],
        start: &[u8],
        limit: usize,
    ) -> Result<Vec<ScanItem<T>>> {
        let mode = IteratorMode::From(start, Direction::Forward);
        let mut out = Vec::new();
        for item in self.db.iterator_cf(self.handle(cf)?, mode) {
            let (key, value) = item?;
            if !key.starts_with(prefix) || out.len() >= limit {
                break;
            }
            out.push((key.to_vec(), decode(&value)?));
        }
        Ok(out)
    }

    /// Counts the keys that start with `prefix` (no value is decoded).
    pub fn count_prefix(&self, cf: Cf, prefix: &[u8]) -> Result<usize> {
        let mode = IteratorMode::From(prefix, Direction::Forward);
        let mut count = 0;
        for item in self.db.iterator_cf(self.handle(cf)?, mode) {
            let (key, _) = item?;
            if !key.starts_with(prefix) {
                break;
            }
            count += 1;
        }
        Ok(count)
    }

    /// Returns the keys that start with `prefix`, at most `limit`.
    pub fn prefix_keys(&self, cf: Cf, prefix: &[u8], limit: usize) -> Result<Vec<Vec<u8>>> {
        let mode = IteratorMode::From(prefix, Direction::Forward);
        let mut out = Vec::new();
        for item in self.db.iterator_cf(self.handle(cf)?, mode) {
            let (key, _) = item?;
            if !key.starts_with(prefix) || out.len() >= limit {
                break;
            }
            out.push(key.to_vec());
        }
        Ok(out)
    }

    /// Makes a snapshot for coherent multi-key reads.
    pub fn snapshot(&self) -> StoreSnapshot<'_> {
        StoreSnapshot { store: self, snapshot: self.db.snapshot() }
    }

    /// Flushes the memtables to disk.
    pub fn flush(&self) -> Result<()> {
        for cf in Cf::ALL {
            self.db.flush_cf(self.handle(cf)?)?;
        }
        Ok(())
    }

    /// Returns the estimated live data size in bytes.
    pub fn estimated_size(&self) -> u64 {
        Cf::ALL
            .iter()
            .filter_map(|cf| self.handle(*cf).ok())
            .filter_map(|h| self.db.property_int_value_cf(h, "rocksdb.estimate-live-data-size").ok().flatten())
            .sum()
    }
}

/// A coherent read view of the store.
pub struct StoreSnapshot<'a> {
    store: &'a Store,
    snapshot: rocksdb::SnapshotWithThreadMode<'a, DB>,
}

impl StoreSnapshot<'_> {
    /// Reads and decodes one value from the snapshot.
    pub fn get<T: DeserializeOwned>(&self, cf: Cf, key: &[u8]) -> Result<Option<T>> {
        match self.snapshot.get_cf(self.store.handle(cf)?, key)? {
            Some(bytes) => Ok(Some(decode(&bytes)?)),
            None => Ok(None),
        }
    }

    /// Scans keys that start with `prefix` in the snapshot.
    pub fn prefix_scan<T: DeserializeOwned>(&self, cf: Cf, prefix: &[u8], limit: usize) -> Result<Vec<ScanItem<T>>> {
        let mode = IteratorMode::From(prefix, Direction::Forward);
        let mut out = Vec::new();
        for item in self.snapshot.iterator_cf(self.store.handle(cf)?, mode) {
            let (key, value) = item?;
            if !key.starts_with(prefix) || out.len() >= limit {
                break;
            }
            out.push((key.to_vec(), decode(&value)?));
        }
        Ok(out)
    }
}
