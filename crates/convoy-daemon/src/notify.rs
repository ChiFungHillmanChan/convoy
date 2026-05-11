//! Central event bus: lock released, session ended, message created.
//! On each event, scans pending waits and satisfies matching ones.

use convoy_core::{Event, FileLock, MessageKind, SessionId};
use convoy_store::Store;
use std::sync::Arc;

#[derive(Clone)]
pub struct Notifier {
    store: Arc<dyn Store>,
}

impl Notifier {
    pub fn new(store: Arc<dyn Store>) -> Self {
        Self { store }
    }

    pub async fn lock_released(&self, lock: &FileLock) {
        let pending = match self.store.pending_waits().await {
            Ok(v) => v,
            Err(_) => return,
        };
        let ev = Event::LockReleased { abs_path: &lock.abs_path };
        for w in pending {
            if w.condition.matches(&w.session_id, &ev) {
                let _ = self
                    .store
                    .satisfy_wait(&w.id, "satisfied", chrono::Utc::now())
                    .await;
            }
        }
    }

    pub async fn session_ended(&self, session: &SessionId) {
        let pending = match self.store.pending_waits().await { Ok(v) => v, Err(_) => return };
        let ev = Event::SessionEnded { session_id: session };
        for w in pending {
            if w.condition.matches(&w.session_id, &ev) {
                let _ = self.store.satisfy_wait(&w.id, "satisfied", chrono::Utc::now()).await;
            }
        }
    }

    pub async fn message_created(&self, from: &SessionId, to: Option<&SessionId>, kind: MessageKind) {
        let pending = match self.store.pending_waits().await { Ok(v) => v, Err(_) => return };
        let ev = Event::MessageCreated { from, to, kind };
        for w in pending {
            if w.condition.matches(&w.session_id, &ev) {
                let _ = self.store.satisfy_wait(&w.id, "satisfied", chrono::Utc::now()).await;
            }
        }
    }
}
