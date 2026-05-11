use convoy_core::{Agent, Nickname, SessionId};
use convoy_daemon::liveness;
use convoy_store::{MemoryStore, RegisterArgs, Store};
use std::sync::Arc;

#[tokio::test]
async fn reaps_dead_session_after_threshold() {
    let store: Arc<dyn Store> = Arc::new(MemoryStore::new());
    // PID 1 (init) is unlikely to be reaped; instead use a PID that does not exist.
    let nonexistent_pid: u32 = 999_999_999;
    let id = SessionId::new();
    let args = RegisterArgs {
        id: id.clone(),
        agent: Agent::ClaudeCode,
        pid: nonexistent_pid,
        nickname: Nickname::new("ghost").unwrap(),
        branch: None,
        worktree_path: None,
    };
    let now = chrono::Utc::now();
    store.register_session(args, now - chrono::Duration::minutes(10)).await.unwrap();
    // Set last_seen_alive far in the past
    store.touch_alive(&id, now - chrono::Duration::minutes(10)).await.unwrap();

    tokio::spawn(liveness::run(
        store.clone(),
        std::time::Duration::from_millis(50),
        chrono::Duration::seconds(1),
    ));
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let s = store.get_session(&id).await.unwrap();
    assert!(s.ended_at.is_some(), "session should be reaped");
}
