//! Parity tests — run the same behaviour against both MemoryStore and SqliteStore.

use chrono::{Duration, Utc};
use convoy_core::{Agent, FileLock, Message, MessageId, MessageKind, Nickname, SessionId, WaitCondition};
use convoy_store::{MemoryStore, RegisterArgs, SqliteStore, Store, StoreError, WaitRecord};
use std::path::PathBuf;
use std::sync::Arc;
use tempfile::NamedTempFile;

// ---------------------------------------------------------------------------
// Helper: build both stores
// ---------------------------------------------------------------------------

fn memory_store() -> Arc<dyn Store> {
    Arc::new(MemoryStore::new())
}

fn sqlite_store() -> (Arc<dyn Store>, NamedTempFile) {
    let f = NamedTempFile::new().unwrap();
    let store = SqliteStore::open(f.path()).unwrap();
    (Arc::new(store), f)
}

fn reg_args(id: SessionId) -> RegisterArgs {
    RegisterArgs {
        id,
        agent: Agent::ClaudeCode,
        pid: 99,
        nickname: Nickname::new("parity-test").unwrap(),
        branch: Some("main".into()),
        worktree_path: None,
    }
}

fn make_msg(from: &SessionId, to: Option<&SessionId>, body: &str) -> Message {
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

fn make_lock(session: &SessionId, path: &str, ttl: Duration) -> FileLock {
    let now = Utc::now();
    FileLock {
        abs_path: PathBuf::from(path),
        session_id: session.clone(),
        reason: None,
        claimed_at: now,
        expires_at: now + ttl,
    }
}

// ---------------------------------------------------------------------------
// Session parity
// ---------------------------------------------------------------------------

async fn run_sessions_parity(store: Arc<dyn Store>) {
    let id = SessionId::new();
    let now = Utc::now();

    // register and retrieve
    store.register_session(reg_args(id.clone()), now).await.unwrap();
    let s = store.get_session(&id).await.unwrap();
    assert_eq!(s.id, id);
    assert_eq!(s.agent, Agent::ClaudeCode);
    assert_eq!(s.nickname.as_str(), "parity-test");

    // active vs all
    assert_eq!(store.list_active_sessions().await.unwrap().len(), 1);
    assert_eq!(store.list_all_sessions().await.unwrap().len(), 1);

    // end session removes from active
    store.end_session(&id, now).await.unwrap();
    assert!(store.list_active_sessions().await.unwrap().is_empty());
    assert_eq!(store.list_all_sessions().await.unwrap().len(), 1);

    // rename
    let id2 = SessionId::new();
    store.register_session(reg_args(id2.clone()), now).await.unwrap();
    store.rename_session(&id2, Nickname::new("renamed").unwrap()).await.unwrap();
    assert_eq!(store.get_session(&id2).await.unwrap().nickname.as_str(), "renamed");

    // set_branch
    store.set_branch(&id2, Some("feat/x".into())).await.unwrap();
    assert_eq!(store.get_session(&id2).await.unwrap().branch.as_deref(), Some("feat/x"));

    // touch_heartbeat / touch_alive don't error
    let later = now + Duration::seconds(10);
    store.touch_heartbeat(&id2, later).await.unwrap();
    store.touch_alive(&id2, later).await.unwrap();

    // get_session on missing id returns NotFound
    let missing = SessionId::new();
    assert!(matches!(store.get_session(&missing).await, Err(StoreError::NotFound)));
}

#[tokio::test]
async fn memory_store_sessions() {
    run_sessions_parity(memory_store()).await;
}

#[tokio::test]
async fn sqlite_store_sessions() {
    let (_store, _tmp) = sqlite_store();
    run_sessions_parity(_store).await;
}

// ---------------------------------------------------------------------------
// Message parity
// ---------------------------------------------------------------------------

async fn run_messages_parity(store: Arc<dyn Store>) {
    let alice = SessionId::new();
    let bob = SessionId::new();

    let m1 = make_msg(&alice, Some(&bob), "direct to bob");
    let m2 = make_msg(&alice, None, "broadcast");
    let m3 = make_msg(&bob, None, "bob broadcasts");
    let m1_id = m1.id.clone();

    store.insert_message(m1).await.unwrap();
    store.insert_message(m2).await.unwrap();
    store.insert_message(m3).await.unwrap();

    // Bob's inbox: direct + alice's broadcast (not bob's own)
    let inbox = store.inbox(&bob, true, 100).await.unwrap();
    assert_eq!(inbox.len(), 2);
    let bodies: Vec<_> = inbox.iter().map(|m| m.body.as_str()).collect();
    assert!(bodies.contains(&"direct to bob"));
    assert!(bodies.contains(&"broadcast"));

    // mark_read idempotent
    assert_eq!(store.mark_read(&[m1_id.clone()], Utc::now()).await.unwrap(), 1);
    assert_eq!(store.mark_read(&[m1_id.clone()], Utc::now()).await.unwrap(), 0);

    // unread_only filter
    let unread = store.inbox(&bob, true, 100).await.unwrap();
    assert_eq!(unread.len(), 1); // only broadcast still unread
    let all = store.inbox(&bob, false, 100).await.unwrap();
    assert_eq!(all.len(), 2);

    // recent_messages
    let recent = store.recent_messages(10).await.unwrap();
    assert_eq!(recent.len(), 3);
}

#[tokio::test]
async fn memory_store_messages() {
    run_messages_parity(memory_store()).await;
}

#[tokio::test]
async fn sqlite_store_messages() {
    let (_store, _tmp) = sqlite_store();
    run_messages_parity(_store).await;
}

// ---------------------------------------------------------------------------
// Lock parity
// ---------------------------------------------------------------------------

async fn run_locks_parity(store: Arc<dyn Store>) {
    let a = SessionId::new();
    let b = SessionId::new();

    // basic claim
    store.claim_file(make_lock(&a, "/x.rs", Duration::seconds(30))).await.unwrap();

    // conflict by another session
    let err = store
        .claim_file(make_lock(&b, "/x.rs", Duration::seconds(30)))
        .await
        .unwrap_err();
    assert!(matches!(err, StoreError::LockHeld(s) if s == a));

    // re-claim by same session succeeds
    store.claim_file(make_lock(&a, "/x.rs", Duration::seconds(60))).await.unwrap();

    // list_locks / locks_held_by
    assert_eq!(store.list_locks().await.unwrap().len(), 1);
    assert_eq!(store.locks_held_by(&a).await.unwrap().len(), 1);
    assert!(store.locks_held_by(&b).await.unwrap().is_empty());

    // lock_for
    let lf = store.lock_for(std::path::Path::new("/x.rs")).await.unwrap();
    assert!(lf.is_some());
    assert_eq!(lf.unwrap().session_id, a);

    // release by wrong session
    let err2 = store
        .release_file(std::path::Path::new("/x.rs"), &b)
        .await
        .unwrap_err();
    assert!(matches!(err2, StoreError::LockHeld(_)));

    // release by owner
    store.release_file(std::path::Path::new("/x.rs"), &a).await.unwrap();
    assert!(store.list_locks().await.unwrap().is_empty());

    // claim after expiry
    store.claim_file(make_lock(&a, "/y.rs", Duration::seconds(-1))).await.unwrap();
    store.claim_file(make_lock(&b, "/y.rs", Duration::seconds(30))).await.unwrap();
    assert_eq!(
        store.lock_for(std::path::Path::new("/y.rs")).await.unwrap().unwrap().session_id,
        b
    );

    // release_expired_locks
    store.claim_file(make_lock(&a, "/z.rs", Duration::seconds(-1))).await.unwrap();
    let expired = store.release_expired_locks(Utc::now()).await.unwrap();
    assert_eq!(expired.len(), 1);
    assert_eq!(expired[0].abs_path, PathBuf::from("/z.rs"));

    // release_locks_of
    store.claim_file(make_lock(&a, "/q.rs", Duration::seconds(30))).await.unwrap();
    store.claim_file(make_lock(&a, "/r.rs", Duration::seconds(30))).await.unwrap();
    let released = store.release_locks_of(&a).await.unwrap();
    assert_eq!(released.len(), 2);
    assert!(store.locks_held_by(&a).await.unwrap().is_empty());
}

#[tokio::test]
async fn memory_store_locks() {
    run_locks_parity(memory_store()).await;
}

#[tokio::test]
async fn sqlite_store_locks() {
    let (_store, _tmp) = sqlite_store();
    run_locks_parity(_store).await;
}

// ---------------------------------------------------------------------------
// Status parity
// ---------------------------------------------------------------------------

async fn run_status_parity(store: Arc<dyn Store>) {
    let s = SessionId::new();

    // No status yet
    assert!(store.latest_status(&s).await.unwrap().is_none());

    store.push_status(&s, "phase 1".into(), Utc::now()).await.unwrap();
    store.push_status(&s, "phase 2".into(), Utc::now()).await.unwrap();
    assert_eq!(store.latest_status(&s).await.unwrap().as_deref(), Some("phase 2"));
}

#[tokio::test]
async fn memory_store_status() {
    run_status_parity(memory_store()).await;
}

#[tokio::test]
async fn sqlite_store_status() {
    let (_store, _tmp) = sqlite_store();
    run_status_parity(_store).await;
}

// ---------------------------------------------------------------------------
// Waits parity
// ---------------------------------------------------------------------------

async fn run_waits_parity(store: Arc<dyn Store>) {
    let now = Utc::now();
    let sid = SessionId::new();

    let w = WaitRecord {
        id: "w1".into(),
        session_id: sid.clone(),
        condition: WaitCondition::LockReleased { abs_path: "/a".into() },
        hint: None,
        created_at: now,
        expires_at: now + Duration::seconds(60),
        satisfied_at: None,
        outcome: None,
    };
    store.create_wait(w).await.unwrap();

    // pending
    let pending = store.pending_waits().await.unwrap();
    assert_eq!(pending.len(), 1);

    // satisfy
    store.satisfy_wait("w1", "satisfied", now).await.unwrap();
    assert!(store.pending_waits().await.unwrap().is_empty());

    // expired wait timeout
    let w2 = WaitRecord {
        id: "w2".into(),
        session_id: sid.clone(),
        condition: WaitCondition::LockReleased { abs_path: "/b".into() },
        hint: None,
        created_at: now,
        expires_at: now - Duration::seconds(1),
        satisfied_at: None,
        outcome: None,
    };
    store.create_wait(w2).await.unwrap();
    let timed = store.timeout_waits(now).await.unwrap();
    assert_eq!(timed.len(), 1);
    assert_eq!(timed[0].outcome.as_deref(), Some("timeout"));

    // cancel
    let w3 = WaitRecord {
        id: "w3".into(),
        session_id: sid.clone(),
        condition: WaitCondition::LockReleased { abs_path: "/c".into() },
        hint: None,
        created_at: now,
        expires_at: now + Duration::seconds(60),
        satisfied_at: None,
        outcome: None,
    };
    store.create_wait(w3).await.unwrap();
    store.cancel_wait("w3").await.unwrap();
    let pending2 = store.pending_waits().await.unwrap();
    assert!(pending2.iter().all(|w| w.id != "w3"));
}

#[tokio::test]
async fn memory_store_waits() {
    run_waits_parity(memory_store()).await;
}

#[tokio::test]
async fn sqlite_store_waits() {
    let (_store, _tmp) = sqlite_store();
    run_waits_parity(_store).await;
}

// ---------------------------------------------------------------------------
// Events parity
// ---------------------------------------------------------------------------

async fn run_events_parity(store: Arc<dyn Store>) {
    let sid = SessionId::new();
    let now = Utc::now();

    // log_event with session
    store
        .log_event(Some(&sid), "test.event", serde_json::json!({"key": "val"}), now)
        .await
        .unwrap();

    // log_event without session
    store
        .log_event(None, "global.event", serde_json::json!(null), now)
        .await
        .unwrap();
}

#[tokio::test]
async fn memory_store_events() {
    run_events_parity(memory_store()).await;
}

#[tokio::test]
async fn sqlite_store_events() {
    let (_store, _tmp) = sqlite_store();
    run_events_parity(_store).await;
}
