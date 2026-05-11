//! In-memory store. Single shared `Mutex<Inner>`; all methods async-but-synchronous.

use crate::*;
use convoy_core::LivenessStatus;
use std::collections::HashMap;
use std::sync::Mutex;

#[derive(Default)]
struct Inner {
    sessions: HashMap<SessionId, Session>,
    messages: Vec<Message>,
    locks: HashMap<PathBuf, FileLock>,
    statuses: HashMap<SessionId, Vec<(DateTime<Utc>, String)>>,
    waits: HashMap<String, WaitRecord>,
    events: Vec<(DateTime<Utc>, Option<SessionId>, String, serde_json::Value)>,
}

/// In-memory store.
#[derive(Default)]
pub struct MemoryStore {
    inner: Mutex<Inner>,
}

impl MemoryStore {
    /// New empty store.
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait::async_trait]
impl Store for MemoryStore {
    async fn register_session(&self, args: RegisterArgs, now: DateTime<Utc>) -> Result<(), StoreError> {
        let mut inner = self.inner.lock().unwrap();
        let s = Session {
            id: args.id.clone(),
            agent: args.agent,
            pid: args.pid,
            branch: args.branch,
            worktree_path: args.worktree_path,
            nickname: args.nickname,
            started_at: now,
            last_heartbeat: now,
            last_seen_alive: now,
            ended_at: None,
        };
        inner.sessions.insert(args.id, s);
        Ok(())
    }

    async fn get_session(&self, id: &SessionId) -> Result<Session, StoreError> {
        self.inner
            .lock()
            .unwrap()
            .sessions
            .get(id)
            .cloned()
            .ok_or(StoreError::NotFound)
    }

    async fn list_active_sessions(&self) -> Result<Vec<Session>, StoreError> {
        Ok(self
            .inner
            .lock()
            .unwrap()
            .sessions
            .values()
            .filter(|s| s.ended_at.is_none())
            .cloned()
            .collect())
    }

    async fn list_all_sessions(&self) -> Result<Vec<Session>, StoreError> {
        Ok(self.inner.lock().unwrap().sessions.values().cloned().collect())
    }

    async fn touch_heartbeat(&self, id: &SessionId, now: DateTime<Utc>) -> Result<(), StoreError> {
        let mut inner = self.inner.lock().unwrap();
        let s = inner.sessions.get_mut(id).ok_or(StoreError::NotFound)?;
        s.last_heartbeat = now;
        Ok(())
    }

    async fn touch_alive(&self, id: &SessionId, now: DateTime<Utc>) -> Result<(), StoreError> {
        let mut inner = self.inner.lock().unwrap();
        let s = inner.sessions.get_mut(id).ok_or(StoreError::NotFound)?;
        s.last_seen_alive = now;
        Ok(())
    }

    async fn rename_session(&self, id: &SessionId, new: Nickname) -> Result<(), StoreError> {
        let mut inner = self.inner.lock().unwrap();
        let s = inner.sessions.get_mut(id).ok_or(StoreError::NotFound)?;
        s.nickname = new;
        Ok(())
    }

    async fn set_branch(&self, id: &SessionId, branch: Option<String>) -> Result<(), StoreError> {
        let mut inner = self.inner.lock().unwrap();
        let s = inner.sessions.get_mut(id).ok_or(StoreError::NotFound)?;
        s.branch = branch;
        Ok(())
    }

    async fn end_session(&self, id: &SessionId, now: DateTime<Utc>) -> Result<(), StoreError> {
        let mut inner = self.inner.lock().unwrap();
        let s = inner.sessions.get_mut(id).ok_or(StoreError::NotFound)?;
        s.ended_at = Some(now);
        Ok(())
    }

    async fn insert_message(&self, msg: Message) -> Result<(), StoreError> {
        self.inner.lock().unwrap().messages.push(msg);
        Ok(())
    }

    async fn inbox(
        &self,
        for_session: &SessionId,
        unread_only: bool,
        limit: usize,
    ) -> Result<Vec<Message>, StoreError> {
        let inner = self.inner.lock().unwrap();
        let mut out: Vec<Message> = inner
            .messages
            .iter()
            .filter(|m| match &m.to {
                Some(to) => to == for_session,
                None => m.from != *for_session, // broadcasts excluding own
            })
            .filter(|m| !unread_only || m.read_at.is_none())
            .cloned()
            .collect();
        out.sort_by_key(|m| m.created_at);
        out.truncate(limit);
        Ok(out)
    }

    async fn mark_read(&self, ids: &[MessageId], now: DateTime<Utc>) -> Result<usize, StoreError> {
        let mut inner = self.inner.lock().unwrap();
        let mut marked = 0;
        for m in inner.messages.iter_mut() {
            if ids.contains(&m.id) && m.read_at.is_none() {
                m.read_at = Some(now);
                marked += 1;
            }
        }
        Ok(marked)
    }

    async fn recent_messages(&self, limit: usize) -> Result<Vec<Message>, StoreError> {
        let inner = self.inner.lock().unwrap();
        let mut v: Vec<Message> = inner.messages.iter().cloned().collect();
        v.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        v.truncate(limit);
        Ok(v)
    }

    // === remaining trait methods stubbed for later tasks ===
    async fn claim_file(&self, _lock: FileLock) -> Result<(), StoreError> { todo!() }
    async fn release_file(&self, _abs_path: &std::path::Path, _session: &SessionId) -> Result<(), StoreError> { todo!() }
    async fn list_locks(&self) -> Result<Vec<FileLock>, StoreError> { todo!() }
    async fn locks_held_by(&self, _session: &SessionId) -> Result<Vec<FileLock>, StoreError> { todo!() }
    async fn lock_for(&self, _abs_path: &std::path::Path) -> Result<Option<FileLock>, StoreError> { todo!() }
    async fn release_expired_locks(&self, _now: DateTime<Utc>) -> Result<Vec<FileLock>, StoreError> { todo!() }
    async fn release_locks_of(&self, _session: &SessionId) -> Result<Vec<FileLock>, StoreError> { todo!() }
    async fn push_status(&self, _session: &SessionId, _summary: String, _now: DateTime<Utc>) -> Result<(), StoreError> { todo!() }
    async fn latest_status(&self, _session: &SessionId) -> Result<Option<String>, StoreError> { todo!() }
    async fn create_wait(&self, _w: WaitRecord) -> Result<(), StoreError> { todo!() }
    async fn cancel_wait(&self, _id: &str) -> Result<(), StoreError> { todo!() }
    async fn satisfy_wait(&self, _id: &str, _outcome: &str, _now: DateTime<Utc>) -> Result<(), StoreError> { todo!() }
    async fn pending_waits(&self) -> Result<Vec<WaitRecord>, StoreError> { todo!() }
    async fn timeout_waits(&self, _now: DateTime<Utc>) -> Result<Vec<WaitRecord>, StoreError> { todo!() }
    async fn log_event(&self, _session: Option<&SessionId>, _kind: &str, _payload: serde_json::Value, _now: DateTime<Utc>) -> Result<(), StoreError> { todo!() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use convoy_core::{Agent, Nickname};

    fn args() -> RegisterArgs {
        RegisterArgs {
            id: SessionId::new(),
            agent: Agent::ClaudeCode,
            pid: 42,
            nickname: Nickname::new("feat-x").unwrap(),
            branch: Some("feature/x".into()),
            worktree_path: None,
        }
    }

    #[tokio::test]
    async fn register_and_get() {
        let store = MemoryStore::new();
        let a = args();
        let id = a.id.clone();
        store.register_session(a, Utc::now()).await.unwrap();
        let s = store.get_session(&id).await.unwrap();
        assert_eq!(s.id, id);
    }

    #[tokio::test]
    async fn end_session_filters_from_active() {
        let store = MemoryStore::new();
        let a = args();
        let id = a.id.clone();
        store.register_session(a, Utc::now()).await.unwrap();
        store.end_session(&id, Utc::now()).await.unwrap();
        assert!(store.list_active_sessions().await.unwrap().is_empty());
        assert_eq!(store.list_all_sessions().await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn rename_changes_nickname() {
        let store = MemoryStore::new();
        let a = args();
        let id = a.id.clone();
        store.register_session(a, Utc::now()).await.unwrap();
        store.rename_session(&id, Nickname::new("renamed").unwrap()).await.unwrap();
        assert_eq!(store.get_session(&id).await.unwrap().nickname.as_str(), "renamed");
    }

    use convoy_core::{Message, MessageId, MessageKind};

    fn msg(from: &SessionId, to: Option<&SessionId>, body: &str) -> Message {
        Message {
            id: MessageId::new(),
            from: from.clone(),
            to: to.cloned(),
            kind: MessageKind::Info,
            in_reply_to: None,
            body: body.into(),
            created_at: Utc::now(),
            read_at: None,
        }
    }

    #[tokio::test]
    async fn inbox_includes_targeted_and_broadcast_excludes_self() {
        let store = MemoryStore::new();
        let alice = SessionId::new();
        let bob = SessionId::new();
        store.insert_message(msg(&alice, Some(&bob), "hi bob")).await.unwrap();
        store.insert_message(msg(&alice, None, "broadcast")).await.unwrap();
        store.insert_message(msg(&bob, None, "from bob")).await.unwrap();
        let bob_inbox = store.inbox(&bob, true, 100).await.unwrap();
        assert_eq!(bob_inbox.len(), 2); // "hi bob" + "broadcast", not bob's own
        let bob_bodies: Vec<_> = bob_inbox.iter().map(|m| m.body.as_str()).collect();
        assert!(bob_bodies.contains(&"hi bob"));
        assert!(bob_bodies.contains(&"broadcast"));
    }

    #[tokio::test]
    async fn mark_read_only_counts_first_time() {
        let store = MemoryStore::new();
        let alice = SessionId::new();
        let bob = SessionId::new();
        let m = msg(&alice, Some(&bob), "x");
        let id = m.id.clone();
        store.insert_message(m).await.unwrap();
        assert_eq!(store.mark_read(&[id.clone()], Utc::now()).await.unwrap(), 1);
        assert_eq!(store.mark_read(&[id], Utc::now()).await.unwrap(), 0);
    }

    #[tokio::test]
    async fn inbox_unread_only_filters() {
        let store = MemoryStore::new();
        let alice = SessionId::new();
        let bob = SessionId::new();
        let m = msg(&alice, Some(&bob), "x");
        let id = m.id.clone();
        store.insert_message(m).await.unwrap();
        store.mark_read(&[id], Utc::now()).await.unwrap();
        assert!(store.inbox(&bob, true, 100).await.unwrap().is_empty());
        assert_eq!(store.inbox(&bob, false, 100).await.unwrap().len(), 1);
    }
}
