//! Canonical storage of ZCIE: RocksDB column families, key layout, object store,
//! watermarks and recovery helpers. See `docs/06-storage.md`.

pub mod codec;
pub mod error;
pub mod events;
pub mod files;
pub mod keys;
pub mod layout;
pub mod objects;
pub mod recovery;
pub mod rocksdb;
pub mod values;
pub mod watermarks;

pub use codec::{NumberValue, decode, encode};
pub use error::{Error, Result};
pub use keys::{key, prefix, seq_part};
pub use objects::ObjectStore;
pub use rocksdb::{Batch, Cf, Store, StoreSnapshot};
