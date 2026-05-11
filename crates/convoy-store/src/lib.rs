//! Storage abstraction for Convoy. Two impls live here: in-memory and SQLite.

#![forbid(unsafe_code)]

use chrono::{DateTime, Utc};
use convoy_core::{
    Agent, FileLock, Message, MessageId, MessageKind, Nickname, Session, SessionId,
    WaitCondition,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub mod memory;
pub mod sqlite;

pub use memory::MemoryStore;
pub use sqlite::SqliteStore;

/// Storage failures.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    /// Lookup miss.
    #[error("not found")]
    NotFound,
    /// Lock was already held by another session.
    #[error("lock already held by {0}")]
    LockHeld(SessionId),
    /// SQL or I/O failure.
    #[error("storage backend: {0}")]
    Backend(#[from] anyhow::Error),
}

/// Args for registering a fresh session.
#[derive(Debug, Clone)]
pub struct RegisterArgs {
    /// Session id (caller-supplied so MCP can echo it back).
    pub id: SessionId,
    /// Agent kind.
    pub agent: Agent,
    /// OS pid.
    pub pid: u32,
    /// Initial nickname.
    pub nickname: Nickname,
    /// Branch when registered.
    pub branch: Option<String>,
    /// Worktree path.
    pub worktree_path: Option<PathBuf>,
}

/// Wait subscription record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WaitRecord {
    /// Wait id.
    pub id: String,
    /// Owner session.
    pub session_id: SessionId,
    /// Condition.
    pub condition: WaitCondition,
    /// Free-form hint.
    pub hint: Option<String>,
    /// Created.
    pub created_at: DateTime<Utc>,
    /// Expiry.
    pub expires_at: DateTime<Utc>,
    /// Set when satisfied or expired.
    pub satisfied_at: Option<DateTime<Utc>>,
    /// 'satisfied' | 'timeout' | 'cancelled' | null.
    pub outcome: Option<String>,
}

/// Single storage trait covering every persisted entity. Async to match
/// daemon's tokio runtime, even though SQLite calls are synchronous (we
/// run them inside `spawn_blocking` in the impl).
#[async_trait::async_trait]
pub trait Store: Send + Sync + 'static {
    // sessions
    async fn register_session(&self, args: RegisterArgs, now: DateTime<Utc>) -> Result<(), StoreError>;
    async fn get_session(&self, id: &SessionId) -> Result<Session, StoreError>;
    async fn list_active_sessions(&self) -> Result<Vec<Session>, StoreError>;
    async fn list_all_sessions(&self) -> Result<Vec<Session>, StoreError>;
    async fn touch_heartbeat(&self, id: &SessionId, now: DateTime<Utc>) -> Result<(), StoreError>;
    async fn touch_alive(&self, id: &SessionId, now: DateTime<Utc>) -> Result<(), StoreError>;
    async fn rename_session(&self, id: &SessionId, new: Nickname) -> Result<(), StoreError>;
    async fn set_branch(&self, id: &SessionId, branch: Option<String>) -> Result<(), StoreError>;
    async fn end_session(&self, id: &SessionId, now: DateTime<Utc>) -> Result<(), StoreError>;

    // messages
    async fn insert_message(&self, msg: Message) -> Result<(), StoreError>;
    async fn inbox(
        &self,
        for_session: &SessionId,
        unread_only: bool,
        limit: usize,
    ) -> Result<Vec<Message>, StoreError>;
    async fn mark_read(&self, ids: &[MessageId], now: DateTime<Utc>) -> Result<usize, StoreError>;
    async fn recent_messages(&self, limit: usize) -> Result<Vec<Message>, StoreError>;

    // locks
    async fn claim_file(&self, lock: FileLock) -> Result<(), StoreError>;
    async fn release_file(&self, abs_path: &std::path::Path, session: &SessionId) -> Result<(), StoreError>;
    async fn list_locks(&self) -> Result<Vec<FileLock>, StoreError>;
    async fn locks_held_by(&self, session: &SessionId) -> Result<Vec<FileLock>, StoreError>;
    async fn lock_for(&self, abs_path: &std::path::Path) -> Result<Option<FileLock>, StoreError>;
    async fn release_expired_locks(&self, now: DateTime<Utc>) -> Result<Vec<FileLock>, StoreError>;
    async fn release_locks_of(&self, session: &SessionId) -> Result<Vec<FileLock>, StoreError>;

    // status updates
    async fn push_status(&self, session: &SessionId, summary: String, now: DateTime<Utc>) -> Result<(), StoreError>;
    async fn latest_status(&self, session: &SessionId) -> Result<Option<String>, StoreError>;

    // waits
    async fn create_wait(&self, w: WaitRecord) -> Result<(), StoreError>;
    async fn cancel_wait(&self, id: &str) -> Result<(), StoreError>;
    async fn satisfy_wait(&self, id: &str, outcome: &str, now: DateTime<Utc>) -> Result<(), StoreError>;
    async fn pending_waits(&self) -> Result<Vec<WaitRecord>, StoreError>;
    async fn timeout_waits(&self, now: DateTime<Utc>) -> Result<Vec<WaitRecord>, StoreError>;

    // events / audit
    async fn log_event(
        &self,
        session: Option<&SessionId>,
        kind: &str,
        payload: serde_json::Value,
        now: DateTime<Utc>,
    ) -> Result<(), StoreError>;
}
