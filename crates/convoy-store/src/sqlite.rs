//! SQLite-backed store.

use crate::*;
use anyhow::anyhow;
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::OptionalExtension;
use std::path::Path;

type SqlitePool = Pool<SqliteConnectionManager>;

/// Persistent implementation using rusqlite + r2d2 pool.
pub struct SqliteStore {
    pool: SqlitePool,
}

impl SqliteStore {
    /// Open (or create) a SQLite database at `path`, run migrations, return store.
    pub fn open(path: &Path) -> Result<Self, StoreError> {
        let manager = SqliteConnectionManager::file(path);
        let pool = Pool::builder()
            .max_size(4)
            .build(manager)
            .map_err(|e| StoreError::Backend(anyhow!(e)))?;
        let store = Self { pool };
        store.migrate()?;
        Ok(store)
    }

    fn migrate(&self) -> Result<(), StoreError> {
        let conn = self.pool.get().map_err(|e| StoreError::Backend(anyhow!(e)))?;
        let sql = include_str!("../migrations/0001_init.sql");
        conn.execute_batch(sql)
            .map_err(|e| StoreError::Backend(anyhow!(e)))?;
        // Set version if not already set
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM schema_version", [], |r| r.get(0))
            .map_err(|e| StoreError::Backend(anyhow!(e)))?;
        if count == 0 {
            conn.execute("INSERT INTO schema_version(version) VALUES(1)", [])
                .map_err(|e| StoreError::Backend(anyhow!(e)))?;
        }
        Ok(())
    }

    /// Return the stored schema version (1 after first migration).
    pub fn version(&self) -> Result<i64, StoreError> {
        let conn = self.pool.get().map_err(|e| StoreError::Backend(anyhow!(e)))?;
        conn.query_row("SELECT version FROM schema_version", [], |r| r.get(0))
            .map_err(|e| StoreError::Backend(anyhow!(e)))
    }
}

// ---------------------------------------------------------------------------
// Row-mapper helpers
// ---------------------------------------------------------------------------

fn row_to_session(row: &rusqlite::Row<'_>) -> rusqlite::Result<Session> {
    let id_str: String = row.get(0)?;
    let agent_str: String = row.get(1)?;
    let pid: u32 = row.get(2)?;
    let branch: Option<String> = row.get(3)?;
    let worktree_path: Option<String> = row.get(4)?;
    let nickname_str: String = row.get(5)?;
    let started_at_str: String = row.get(6)?;
    let last_heartbeat_str: String = row.get(7)?;
    let last_seen_alive_str: String = row.get(8)?;
    let ended_at_str: Option<String> = row.get(9)?;

    Ok(Session {
        id: SessionId::from_string_unchecked(id_str),
        agent: Agent::from_tag(&agent_str).unwrap_or(Agent::ClaudeCode),
        pid,
        branch,
        worktree_path: worktree_path.map(std::path::PathBuf::from),
        nickname: Nickname::new(&nickname_str).unwrap_or_else(|_| Nickname::new("unknown").unwrap()),
        started_at: started_at_str.parse().unwrap(),
        last_heartbeat: last_heartbeat_str.parse().unwrap(),
        last_seen_alive: last_seen_alive_str.parse().unwrap(),
        ended_at: ended_at_str.map(|s| s.parse().unwrap()),
    })
}

fn row_to_message(row: &rusqlite::Row<'_>) -> rusqlite::Result<Message> {
    let id_str: String = row.get(0)?;
    let from_str: String = row.get(1)?;
    let to_str: Option<String> = row.get(2)?;
    let kind_str: String = row.get(3)?;
    let reply_str: Option<String> = row.get(4)?;
    let body: String = row.get(5)?;
    let created_at_str: String = row.get(6)?;
    let read_at_str: Option<String> = row.get(7)?;

    Ok(Message {
        id: MessageId::from_string_unchecked(id_str),
        from: SessionId::from_string_unchecked(from_str),
        to: to_str.map(SessionId::from_string_unchecked),
        kind: MessageKind::from_tag(&kind_str).unwrap_or(MessageKind::Info),
        in_reply_to: reply_str.map(MessageId::from_string_unchecked),
        body,
        created_at: created_at_str.parse().unwrap(),
        read_at: read_at_str.map(|s| s.parse().unwrap()),
    })
}

fn row_to_lock(row: &rusqlite::Row<'_>) -> rusqlite::Result<FileLock> {
    let abs_path_str: String = row.get(0)?;
    let session_str: String = row.get(1)?;
    let reason: Option<String> = row.get(2)?;
    let claimed_at_str: String = row.get(3)?;
    let expires_at_str: String = row.get(4)?;

    Ok(FileLock {
        abs_path: std::path::PathBuf::from(abs_path_str),
        session_id: SessionId::from_string_unchecked(session_str),
        reason,
        claimed_at: claimed_at_str.parse().unwrap(),
        expires_at: expires_at_str.parse().unwrap(),
    })
}

fn row_to_wait(row: &rusqlite::Row<'_>) -> rusqlite::Result<WaitRecord> {
    let id: String = row.get(0)?;
    let session_str: String = row.get(1)?;
    let condition_json: String = row.get(2)?;
    let hint: Option<String> = row.get(3)?;
    let created_at_str: String = row.get(4)?;
    let expires_at_str: String = row.get(5)?;
    let satisfied_at_str: Option<String> = row.get(6)?;
    let outcome: Option<String> = row.get(7)?;

    Ok(WaitRecord {
        id,
        session_id: SessionId::from_string_unchecked(session_str),
        condition: serde_json::from_str(&condition_json).unwrap(),
        hint,
        created_at: created_at_str.parse().unwrap(),
        expires_at: expires_at_str.parse().unwrap(),
        satisfied_at: satisfied_at_str.map(|s| s.parse().unwrap()),
        outcome,
    })
}

// ---------------------------------------------------------------------------
// Store impl
// ---------------------------------------------------------------------------

#[async_trait::async_trait]
impl Store for SqliteStore {
    // sessions ---------------------------------------------------------------

    async fn register_session(&self, args: RegisterArgs, now: DateTime<Utc>) -> Result<(), StoreError> {
        let pool = self.pool.clone();
        tokio::task::spawn_blocking(move || {
            let conn = pool.get().map_err(|e| StoreError::Backend(anyhow!(e)))?;
            conn.execute(
                "INSERT INTO sessions
                    (id, agent, pid, branch, worktree_path, nickname,
                     started_at, last_heartbeat, last_seen_alive, ended_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, NULL)",
                rusqlite::params![
                    args.id.as_str(),
                    args.agent.as_tag(),
                    args.pid,
                    args.branch,
                    args.worktree_path.as_deref().map(|p| p.to_string_lossy().into_owned()),
                    args.nickname.as_str(),
                    now.to_rfc3339(),
                    now.to_rfc3339(),
                    now.to_rfc3339(),
                ],
            )
            .map_err(|e| StoreError::Backend(anyhow!(e)))?;
            Ok(())
        })
        .await
        .map_err(|e| StoreError::Backend(anyhow!(e)))?
    }

    async fn get_session(&self, id: &SessionId) -> Result<Session, StoreError> {
        let pool = self.pool.clone();
        let id_str = id.as_str().to_string();
        tokio::task::spawn_blocking(move || {
            let conn = pool.get().map_err(|e| StoreError::Backend(anyhow!(e)))?;
            conn.query_row(
                "SELECT id, agent, pid, branch, worktree_path, nickname,
                        started_at, last_heartbeat, last_seen_alive, ended_at
                 FROM sessions WHERE id = ?1",
                [&id_str],
                row_to_session,
            )
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => StoreError::NotFound,
                other => StoreError::Backend(anyhow!(other)),
            })
        })
        .await
        .map_err(|e| StoreError::Backend(anyhow!(e)))?
    }

    async fn list_active_sessions(&self) -> Result<Vec<Session>, StoreError> {
        let pool = self.pool.clone();
        tokio::task::spawn_blocking(move || {
            let conn = pool.get().map_err(|e| StoreError::Backend(anyhow!(e)))?;
            let mut stmt = conn
                .prepare(
                    "SELECT id, agent, pid, branch, worktree_path, nickname,
                            started_at, last_heartbeat, last_seen_alive, ended_at
                     FROM sessions WHERE ended_at IS NULL",
                )
                .map_err(|e| StoreError::Backend(anyhow!(e)))?;
            let rows = stmt
                .query_map([], row_to_session)
                .map_err(|e| StoreError::Backend(anyhow!(e)))?;
            rows.collect::<Result<Vec<_>, _>>()
                .map_err(|e| StoreError::Backend(anyhow!(e)))
        })
        .await
        .map_err(|e| StoreError::Backend(anyhow!(e)))?
    }

    async fn list_all_sessions(&self) -> Result<Vec<Session>, StoreError> {
        let pool = self.pool.clone();
        tokio::task::spawn_blocking(move || {
            let conn = pool.get().map_err(|e| StoreError::Backend(anyhow!(e)))?;
            let mut stmt = conn
                .prepare(
                    "SELECT id, agent, pid, branch, worktree_path, nickname,
                            started_at, last_heartbeat, last_seen_alive, ended_at
                     FROM sessions",
                )
                .map_err(|e| StoreError::Backend(anyhow!(e)))?;
            let rows = stmt
                .query_map([], row_to_session)
                .map_err(|e| StoreError::Backend(anyhow!(e)))?;
            rows.collect::<Result<Vec<_>, _>>()
                .map_err(|e| StoreError::Backend(anyhow!(e)))
        })
        .await
        .map_err(|e| StoreError::Backend(anyhow!(e)))?
    }

    async fn touch_heartbeat(&self, id: &SessionId, now: DateTime<Utc>) -> Result<(), StoreError> {
        let pool = self.pool.clone();
        let id_str = id.as_str().to_string();
        tokio::task::spawn_blocking(move || {
            let conn = pool.get().map_err(|e| StoreError::Backend(anyhow!(e)))?;
            let n = conn
                .execute(
                    "UPDATE sessions SET last_heartbeat = ?1 WHERE id = ?2",
                    rusqlite::params![now.to_rfc3339(), id_str],
                )
                .map_err(|e| StoreError::Backend(anyhow!(e)))?;
            if n == 0 { Err(StoreError::NotFound) } else { Ok(()) }
        })
        .await
        .map_err(|e| StoreError::Backend(anyhow!(e)))?
    }

    async fn touch_alive(&self, id: &SessionId, now: DateTime<Utc>) -> Result<(), StoreError> {
        let pool = self.pool.clone();
        let id_str = id.as_str().to_string();
        tokio::task::spawn_blocking(move || {
            let conn = pool.get().map_err(|e| StoreError::Backend(anyhow!(e)))?;
            let n = conn
                .execute(
                    "UPDATE sessions SET last_seen_alive = ?1 WHERE id = ?2",
                    rusqlite::params![now.to_rfc3339(), id_str],
                )
                .map_err(|e| StoreError::Backend(anyhow!(e)))?;
            if n == 0 { Err(StoreError::NotFound) } else { Ok(()) }
        })
        .await
        .map_err(|e| StoreError::Backend(anyhow!(e)))?
    }

    async fn rename_session(&self, id: &SessionId, new: Nickname) -> Result<(), StoreError> {
        let pool = self.pool.clone();
        let id_str = id.as_str().to_string();
        tokio::task::spawn_blocking(move || {
            let conn = pool.get().map_err(|e| StoreError::Backend(anyhow!(e)))?;
            let n = conn
                .execute(
                    "UPDATE sessions SET nickname = ?1 WHERE id = ?2",
                    rusqlite::params![new.as_str(), id_str],
                )
                .map_err(|e| StoreError::Backend(anyhow!(e)))?;
            if n == 0 { Err(StoreError::NotFound) } else { Ok(()) }
        })
        .await
        .map_err(|e| StoreError::Backend(anyhow!(e)))?
    }

    async fn set_branch(&self, id: &SessionId, branch: Option<String>) -> Result<(), StoreError> {
        let pool = self.pool.clone();
        let id_str = id.as_str().to_string();
        tokio::task::spawn_blocking(move || {
            let conn = pool.get().map_err(|e| StoreError::Backend(anyhow!(e)))?;
            let n = conn
                .execute(
                    "UPDATE sessions SET branch = ?1 WHERE id = ?2",
                    rusqlite::params![branch, id_str],
                )
                .map_err(|e| StoreError::Backend(anyhow!(e)))?;
            if n == 0 { Err(StoreError::NotFound) } else { Ok(()) }
        })
        .await
        .map_err(|e| StoreError::Backend(anyhow!(e)))?
    }

    async fn end_session(&self, id: &SessionId, now: DateTime<Utc>) -> Result<(), StoreError> {
        let pool = self.pool.clone();
        let id_str = id.as_str().to_string();
        tokio::task::spawn_blocking(move || {
            let conn = pool.get().map_err(|e| StoreError::Backend(anyhow!(e)))?;
            let n = conn
                .execute(
                    "UPDATE sessions SET ended_at = ?1 WHERE id = ?2",
                    rusqlite::params![now.to_rfc3339(), id_str],
                )
                .map_err(|e| StoreError::Backend(anyhow!(e)))?;
            if n == 0 { Err(StoreError::NotFound) } else { Ok(()) }
        })
        .await
        .map_err(|e| StoreError::Backend(anyhow!(e)))?
    }

    // messages ---------------------------------------------------------------

    async fn insert_message(&self, msg: Message) -> Result<(), StoreError> {
        let pool = self.pool.clone();
        tokio::task::spawn_blocking(move || {
            let conn = pool.get().map_err(|e| StoreError::Backend(anyhow!(e)))?;
            conn.execute(
                "INSERT INTO messages
                    (id, from_session, to_session, kind, in_reply_to, body, created_at, read_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                rusqlite::params![
                    msg.id.as_str(),
                    msg.from.as_str(),
                    msg.to.as_ref().map(|s| s.as_str()),
                    msg.kind.as_tag(),
                    msg.in_reply_to.as_ref().map(|m| m.as_str()),
                    msg.body,
                    msg.created_at.to_rfc3339(),
                    msg.read_at.map(|t| t.to_rfc3339()),
                ],
            )
            .map_err(|e| StoreError::Backend(anyhow!(e)))?;
            Ok(())
        })
        .await
        .map_err(|e| StoreError::Backend(anyhow!(e)))?
    }

    async fn inbox(
        &self,
        for_session: &SessionId,
        unread_only: bool,
        limit: usize,
    ) -> Result<Vec<Message>, StoreError> {
        let pool = self.pool.clone();
        let sid = for_session.as_str().to_string();
        tokio::task::spawn_blocking(move || {
            let conn = pool.get().map_err(|e| StoreError::Backend(anyhow!(e)))?;
            let unread_flag: i64 = if unread_only { 1 } else { 0 };
            let mut stmt = conn
                .prepare(
                    "SELECT id, from_session, to_session, kind, in_reply_to, body, created_at, read_at
                     FROM messages
                     WHERE (to_session = ?1 OR (to_session IS NULL AND from_session != ?1))
                       AND (?2 = 0 OR read_at IS NULL)
                     ORDER BY created_at ASC
                     LIMIT ?3",
                )
                .map_err(|e| StoreError::Backend(anyhow!(e)))?;
            let rows = stmt
                .query_map(rusqlite::params![sid, unread_flag, limit as i64], row_to_message)
                .map_err(|e| StoreError::Backend(anyhow!(e)))?;
            rows.collect::<Result<Vec<_>, _>>()
                .map_err(|e| StoreError::Backend(anyhow!(e)))
        })
        .await
        .map_err(|e| StoreError::Backend(anyhow!(e)))?
    }

    async fn mark_read(&self, ids: &[MessageId], now: DateTime<Utc>) -> Result<usize, StoreError> {
        if ids.is_empty() {
            return Ok(0);
        }
        let pool = self.pool.clone();
        let id_strs: Vec<String> = ids.iter().map(|m| m.as_str().to_string()).collect();
        tokio::task::spawn_blocking(move || {
            let conn = pool.get().map_err(|e| StoreError::Backend(anyhow!(e)))?;
            let mut count = 0usize;
            for id_str in &id_strs {
                let n = conn
                    .execute(
                        "UPDATE messages SET read_at = ?1 WHERE id = ?2 AND read_at IS NULL",
                        rusqlite::params![now.to_rfc3339(), id_str],
                    )
                    .map_err(|e| StoreError::Backend(anyhow!(e)))?;
                count += n;
            }
            Ok(count)
        })
        .await
        .map_err(|e| StoreError::Backend(anyhow!(e)))?
    }

    async fn recent_messages(&self, limit: usize) -> Result<Vec<Message>, StoreError> {
        let pool = self.pool.clone();
        tokio::task::spawn_blocking(move || {
            let conn = pool.get().map_err(|e| StoreError::Backend(anyhow!(e)))?;
            let mut stmt = conn
                .prepare(
                    "SELECT id, from_session, to_session, kind, in_reply_to, body, created_at, read_at
                     FROM messages
                     ORDER BY created_at DESC
                     LIMIT ?1",
                )
                .map_err(|e| StoreError::Backend(anyhow!(e)))?;
            let rows = stmt
                .query_map([limit as i64], row_to_message)
                .map_err(|e| StoreError::Backend(anyhow!(e)))?;
            rows.collect::<Result<Vec<_>, _>>()
                .map_err(|e| StoreError::Backend(anyhow!(e)))
        })
        .await
        .map_err(|e| StoreError::Backend(anyhow!(e)))?
    }

    // locks ------------------------------------------------------------------

    async fn claim_file(&self, lock: FileLock) -> Result<(), StoreError> {
        let pool = self.pool.clone();
        tokio::task::spawn_blocking(move || {
            let conn = pool.get().map_err(|e| StoreError::Backend(anyhow!(e)))?;
            let now_str = Utc::now().to_rfc3339();
            let path_str = lock.abs_path.to_string_lossy().into_owned();
            let session_str = lock.session_id.as_str().to_string();

            // Delete expired locks held by others, or locks held by same session
            conn.execute(
                "DELETE FROM file_locks WHERE abs_path = ?1 AND (expires_at <= ?2 OR session_id = ?3)",
                rusqlite::params![path_str, now_str, session_str],
            )
            .map_err(|e| StoreError::Backend(anyhow!(e)))?;

            // Check if a non-expired lock by someone else exists
            let existing: Option<String> = conn
                .query_row(
                    "SELECT session_id FROM file_locks WHERE abs_path = ?1",
                    [&path_str],
                    |r| r.get(0),
                )
                .optional()
                .map_err(|e| StoreError::Backend(anyhow!(e)))?;

            if let Some(holder) = existing {
                return Err(StoreError::LockHeld(SessionId::from_string_unchecked(holder)));
            }

            conn.execute(
                "INSERT INTO file_locks (abs_path, session_id, reason, claimed_at, expires_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params![
                    path_str,
                    session_str,
                    lock.reason,
                    lock.claimed_at.to_rfc3339(),
                    lock.expires_at.to_rfc3339(),
                ],
            )
            .map_err(|e| StoreError::Backend(anyhow!(e)))?;
            Ok(())
        })
        .await
        .map_err(|e| StoreError::Backend(anyhow!(e)))?
    }

    async fn release_file(&self, abs_path: &std::path::Path, session: &SessionId) -> Result<(), StoreError> {
        let pool = self.pool.clone();
        let path_str = abs_path.to_string_lossy().into_owned();
        let session_str = session.as_str().to_string();
        tokio::task::spawn_blocking(move || {
            let conn = pool.get().map_err(|e| StoreError::Backend(anyhow!(e)))?;

            // Check if lock exists and who holds it
            let holder: Option<String> = conn
                .query_row(
                    "SELECT session_id FROM file_locks WHERE abs_path = ?1",
                    [&path_str],
                    |r| r.get(0),
                )
                .optional()
                .map_err(|e| StoreError::Backend(anyhow!(e)))?;

            match holder {
                None => Err(StoreError::NotFound),
                Some(h) if h != session_str => {
                    Err(StoreError::LockHeld(SessionId::from_string_unchecked(h)))
                }
                _ => {
                    conn.execute(
                        "DELETE FROM file_locks WHERE abs_path = ?1 AND session_id = ?2",
                        rusqlite::params![path_str, session_str],
                    )
                    .map_err(|e| StoreError::Backend(anyhow!(e)))?;
                    Ok(())
                }
            }
        })
        .await
        .map_err(|e| StoreError::Backend(anyhow!(e)))?
    }

    async fn list_locks(&self) -> Result<Vec<FileLock>, StoreError> {
        let pool = self.pool.clone();
        tokio::task::spawn_blocking(move || {
            let conn = pool.get().map_err(|e| StoreError::Backend(anyhow!(e)))?;
            let mut stmt = conn
                .prepare("SELECT abs_path, session_id, reason, claimed_at, expires_at FROM file_locks")
                .map_err(|e| StoreError::Backend(anyhow!(e)))?;
            let rows = stmt
                .query_map([], row_to_lock)
                .map_err(|e| StoreError::Backend(anyhow!(e)))?;
            rows.collect::<Result<Vec<_>, _>>()
                .map_err(|e| StoreError::Backend(anyhow!(e)))
        })
        .await
        .map_err(|e| StoreError::Backend(anyhow!(e)))?
    }

    async fn locks_held_by(&self, session: &SessionId) -> Result<Vec<FileLock>, StoreError> {
        let pool = self.pool.clone();
        let session_str = session.as_str().to_string();
        tokio::task::spawn_blocking(move || {
            let conn = pool.get().map_err(|e| StoreError::Backend(anyhow!(e)))?;
            let mut stmt = conn
                .prepare(
                    "SELECT abs_path, session_id, reason, claimed_at, expires_at
                     FROM file_locks WHERE session_id = ?1",
                )
                .map_err(|e| StoreError::Backend(anyhow!(e)))?;
            let rows = stmt
                .query_map([&session_str], row_to_lock)
                .map_err(|e| StoreError::Backend(anyhow!(e)))?;
            rows.collect::<Result<Vec<_>, _>>()
                .map_err(|e| StoreError::Backend(anyhow!(e)))
        })
        .await
        .map_err(|e| StoreError::Backend(anyhow!(e)))?
    }

    async fn lock_for(&self, abs_path: &std::path::Path) -> Result<Option<FileLock>, StoreError> {
        let pool = self.pool.clone();
        let path_str = abs_path.to_string_lossy().into_owned();
        tokio::task::spawn_blocking(move || {
            let conn = pool.get().map_err(|e| StoreError::Backend(anyhow!(e)))?;
            conn.query_row(
                "SELECT abs_path, session_id, reason, claimed_at, expires_at
                 FROM file_locks WHERE abs_path = ?1",
                [&path_str],
                row_to_lock,
            )
            .optional()
            .map_err(|e| StoreError::Backend(anyhow!(e)))
        })
        .await
        .map_err(|e| StoreError::Backend(anyhow!(e)))?
    }

    async fn release_expired_locks(&self, now: DateTime<Utc>) -> Result<Vec<FileLock>, StoreError> {
        let pool = self.pool.clone();
        tokio::task::spawn_blocking(move || {
            let conn = pool.get().map_err(|e| StoreError::Backend(anyhow!(e)))?;
            let now_str = now.to_rfc3339();
            // First collect expired locks
            let mut stmt = conn
                .prepare(
                    "SELECT abs_path, session_id, reason, claimed_at, expires_at
                     FROM file_locks WHERE expires_at <= ?1",
                )
                .map_err(|e| StoreError::Backend(anyhow!(e)))?;
            let expired: Vec<FileLock> = stmt
                .query_map([&now_str], row_to_lock)
                .map_err(|e| StoreError::Backend(anyhow!(e)))?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| StoreError::Backend(anyhow!(e)))?;
            // Then delete them
            conn.execute(
                "DELETE FROM file_locks WHERE expires_at <= ?1",
                [&now_str],
            )
            .map_err(|e| StoreError::Backend(anyhow!(e)))?;
            Ok(expired)
        })
        .await
        .map_err(|e| StoreError::Backend(anyhow!(e)))?
    }

    async fn release_locks_of(&self, session: &SessionId) -> Result<Vec<FileLock>, StoreError> {
        let pool = self.pool.clone();
        let session_str = session.as_str().to_string();
        tokio::task::spawn_blocking(move || {
            let conn = pool.get().map_err(|e| StoreError::Backend(anyhow!(e)))?;
            let mut stmt = conn
                .prepare(
                    "SELECT abs_path, session_id, reason, claimed_at, expires_at
                     FROM file_locks WHERE session_id = ?1",
                )
                .map_err(|e| StoreError::Backend(anyhow!(e)))?;
            let owned: Vec<FileLock> = stmt
                .query_map([&session_str], row_to_lock)
                .map_err(|e| StoreError::Backend(anyhow!(e)))?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| StoreError::Backend(anyhow!(e)))?;
            conn.execute(
                "DELETE FROM file_locks WHERE session_id = ?1",
                [&session_str],
            )
            .map_err(|e| StoreError::Backend(anyhow!(e)))?;
            Ok(owned)
        })
        .await
        .map_err(|e| StoreError::Backend(anyhow!(e)))?
    }

    // status -----------------------------------------------------------------

    async fn push_status(&self, session: &SessionId, summary: String, now: DateTime<Utc>) -> Result<(), StoreError> {
        let pool = self.pool.clone();
        let session_str = session.as_str().to_string();
        tokio::task::spawn_blocking(move || {
            let conn = pool.get().map_err(|e| StoreError::Backend(anyhow!(e)))?;
            conn.execute(
                "INSERT INTO status_updates (session_id, summary, created_at) VALUES (?1, ?2, ?3)",
                rusqlite::params![session_str, summary, now.to_rfc3339()],
            )
            .map_err(|e| StoreError::Backend(anyhow!(e)))?;
            Ok(())
        })
        .await
        .map_err(|e| StoreError::Backend(anyhow!(e)))?
    }

    async fn latest_status(&self, session: &SessionId) -> Result<Option<String>, StoreError> {
        let pool = self.pool.clone();
        let session_str = session.as_str().to_string();
        tokio::task::spawn_blocking(move || {
            let conn = pool.get().map_err(|e| StoreError::Backend(anyhow!(e)))?;
            conn.query_row(
                "SELECT summary FROM status_updates WHERE session_id = ?1 ORDER BY id DESC LIMIT 1",
                [&session_str],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| StoreError::Backend(anyhow!(e)))
        })
        .await
        .map_err(|e| StoreError::Backend(anyhow!(e)))?
    }

    // waits ------------------------------------------------------------------

    async fn create_wait(&self, w: WaitRecord) -> Result<(), StoreError> {
        let pool = self.pool.clone();
        tokio::task::spawn_blocking(move || {
            let conn = pool.get().map_err(|e| StoreError::Backend(anyhow!(e)))?;
            let condition_json =
                serde_json::to_string(&w.condition).map_err(|e| StoreError::Backend(anyhow!(e)))?;
            conn.execute(
                "INSERT INTO waits
                    (id, session_id, condition_json, hint, created_at, expires_at, satisfied_at, outcome)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                rusqlite::params![
                    w.id,
                    w.session_id.as_str(),
                    condition_json,
                    w.hint,
                    w.created_at.to_rfc3339(),
                    w.expires_at.to_rfc3339(),
                    w.satisfied_at.map(|t| t.to_rfc3339()),
                    w.outcome,
                ],
            )
            .map_err(|e| StoreError::Backend(anyhow!(e)))?;
            Ok(())
        })
        .await
        .map_err(|e| StoreError::Backend(anyhow!(e)))?
    }

    async fn cancel_wait(&self, id: &str) -> Result<(), StoreError> {
        let pool = self.pool.clone();
        let id = id.to_string();
        tokio::task::spawn_blocking(move || {
            let conn = pool.get().map_err(|e| StoreError::Backend(anyhow!(e)))?;
            let now_str = Utc::now().to_rfc3339();
            let n = conn
                .execute(
                    "UPDATE waits SET satisfied_at = ?1, outcome = 'cancelled'
                     WHERE id = ?2 AND satisfied_at IS NULL",
                    rusqlite::params![now_str, id],
                )
                .map_err(|e| StoreError::Backend(anyhow!(e)))?;
            if n == 0 { Err(StoreError::NotFound) } else { Ok(()) }
        })
        .await
        .map_err(|e| StoreError::Backend(anyhow!(e)))?
    }

    async fn satisfy_wait(&self, id: &str, outcome: &str, now: DateTime<Utc>) -> Result<(), StoreError> {
        let pool = self.pool.clone();
        let id = id.to_string();
        let outcome = outcome.to_string();
        tokio::task::spawn_blocking(move || {
            let conn = pool.get().map_err(|e| StoreError::Backend(anyhow!(e)))?;
            let n = conn
                .execute(
                    "UPDATE waits SET satisfied_at = ?1, outcome = ?2
                     WHERE id = ?3 AND satisfied_at IS NULL",
                    rusqlite::params![now.to_rfc3339(), outcome, id],
                )
                .map_err(|e| StoreError::Backend(anyhow!(e)))?;
            if n == 0 { Err(StoreError::NotFound) } else { Ok(()) }
        })
        .await
        .map_err(|e| StoreError::Backend(anyhow!(e)))?
    }

    async fn pending_waits(&self) -> Result<Vec<WaitRecord>, StoreError> {
        let pool = self.pool.clone();
        tokio::task::spawn_blocking(move || {
            let conn = pool.get().map_err(|e| StoreError::Backend(anyhow!(e)))?;
            let mut stmt = conn
                .prepare(
                    "SELECT id, session_id, condition_json, hint, created_at, expires_at,
                            satisfied_at, outcome
                     FROM waits WHERE satisfied_at IS NULL",
                )
                .map_err(|e| StoreError::Backend(anyhow!(e)))?;
            let rows = stmt
                .query_map([], row_to_wait)
                .map_err(|e| StoreError::Backend(anyhow!(e)))?;
            rows.collect::<Result<Vec<_>, _>>()
                .map_err(|e| StoreError::Backend(anyhow!(e)))
        })
        .await
        .map_err(|e| StoreError::Backend(anyhow!(e)))?
    }

    async fn timeout_waits(&self, now: DateTime<Utc>) -> Result<Vec<WaitRecord>, StoreError> {
        let pool = self.pool.clone();
        tokio::task::spawn_blocking(move || {
            let conn = pool.get().map_err(|e| StoreError::Backend(anyhow!(e)))?;
            let now_str = now.to_rfc3339();
            // Collect expiring waits first
            let mut stmt = conn
                .prepare(
                    "SELECT id, session_id, condition_json, hint, created_at, expires_at,
                            satisfied_at, outcome
                     FROM waits WHERE satisfied_at IS NULL AND expires_at <= ?1",
                )
                .map_err(|e| StoreError::Backend(anyhow!(e)))?;
            let timed: Vec<WaitRecord> = stmt
                .query_map([&now_str], row_to_wait)
                .map_err(|e| StoreError::Backend(anyhow!(e)))?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| StoreError::Backend(anyhow!(e)))?;
            // Mark them
            conn.execute(
                "UPDATE waits SET satisfied_at = ?1, outcome = 'timeout'
                 WHERE satisfied_at IS NULL AND expires_at <= ?1",
                [&now_str],
            )
            .map_err(|e| StoreError::Backend(anyhow!(e)))?;
            // Return with updated fields
            let updated: Vec<WaitRecord> = timed
                .into_iter()
                .map(|mut w| {
                    w.satisfied_at = Some(now);
                    w.outcome = Some("timeout".into());
                    w
                })
                .collect();
            Ok(updated)
        })
        .await
        .map_err(|e| StoreError::Backend(anyhow!(e)))?
    }

    // events -----------------------------------------------------------------

    async fn log_event(
        &self,
        session: Option<&SessionId>,
        kind: &str,
        payload: serde_json::Value,
        now: DateTime<Utc>,
    ) -> Result<(), StoreError> {
        let pool = self.pool.clone();
        let session_str = session.map(|s| s.as_str().to_string());
        let kind = kind.to_string();
        tokio::task::spawn_blocking(move || {
            let conn = pool.get().map_err(|e| StoreError::Backend(anyhow!(e)))?;
            let payload_str =
                serde_json::to_string(&payload).map_err(|e| StoreError::Backend(anyhow!(e)))?;
            conn.execute(
                "INSERT INTO events (session_id, kind, payload, created_at) VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![session_str, kind, payload_str, now.to_rfc3339()],
            )
            .map_err(|e| StoreError::Backend(anyhow!(e)))?;
            Ok(())
        })
        .await
        .map_err(|e| StoreError::Backend(anyhow!(e)))?
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    fn open_tmp() -> SqliteStore {
        let f = NamedTempFile::new().unwrap();
        SqliteStore::open(f.path()).unwrap()
    }

    #[test]
    fn migration_succeeds() {
        let _store = open_tmp();
    }

    #[test]
    fn version_is_one_after_migration() {
        let store = open_tmp();
        assert_eq!(store.version().unwrap(), 1);
    }
}
