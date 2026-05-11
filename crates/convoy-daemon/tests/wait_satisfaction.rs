use convoy_core::{Agent, FileLock, Nickname, SessionId, WaitCondition};
use convoy_daemon::{notify::Notifier, server::Daemon};
use convoy_store::{MemoryStore, RegisterArgs, Store, WaitRecord};
use std::path::PathBuf;
use std::sync::Arc;

#[tokio::test]
async fn release_satisfies_pending_wait() {
    let store: Arc<dyn Store> = Arc::new(MemoryStore::new());
    let notifier = Notifier::new(store.clone());

    // Create a wait for a release.
    let waiter = SessionId::new();
    store.register_session(
        RegisterArgs {
            id: waiter.clone(),
            agent: Agent::ClaudeCode,
            pid: std::process::id(),
            nickname: Nickname::new("w").unwrap(),
            branch: None,
            worktree_path: None,
        },
        chrono::Utc::now(),
    ).await.unwrap();
    store.create_wait(WaitRecord {
        id: "w1".into(),
        session_id: waiter.clone(),
        condition: WaitCondition::LockReleased { abs_path: PathBuf::from("/x.rs") },
        hint: None,
        created_at: chrono::Utc::now(),
        expires_at: chrono::Utc::now() + chrono::Duration::minutes(10),
        satisfied_at: None,
        outcome: None,
    }).await.unwrap();

    // Simulate a release event.
    let lock = FileLock {
        abs_path: PathBuf::from("/x.rs"),
        session_id: SessionId::new(),
        reason: None,
        claimed_at: chrono::Utc::now(),
        expires_at: chrono::Utc::now() + chrono::Duration::minutes(30),
    };
    notifier.lock_released(&lock).await;

    let pending = store.pending_waits().await.unwrap();
    assert!(pending.is_empty(), "wait should be satisfied");
}
