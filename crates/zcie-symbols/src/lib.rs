//! Stable symbol IDs, symbol records per file and name lookup.

pub mod error;
pub mod identity;
pub mod index;

pub use error::{Error, Result};
pub use identity::symbol_id_for;
pub use index::{SymbolUpdate, file_symbols, find_by_name, get_symbol, remove_file_symbols, upsert_file_symbols};
