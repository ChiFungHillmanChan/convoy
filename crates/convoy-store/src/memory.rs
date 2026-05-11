//! In-memory store for tests.

use crate::*;

/// In-memory implementation. Not durable; for tests only.
#[derive(Default)]
pub struct MemoryStore;
