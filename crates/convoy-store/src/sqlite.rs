//! SQLite-backed store.

use crate::*;

/// Persistent implementation using rusqlite + r2d2 pool.
pub struct SqliteStore;
