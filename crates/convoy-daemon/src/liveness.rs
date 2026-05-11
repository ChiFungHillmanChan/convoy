//! Periodic PID probe. On macOS / Linux uses `kill(pid, 0)` semantics via libc.

use convoy_store::Store;
use std::sync::Arc;
use std::time::Duration;

/// Probes every active session's PID at `interval`. If alive, refreshes
/// `last_seen_alive`. If dead and `last_seen_alive` is older than
/// `dead_threshold`, calls `end_session` + releases locks.
pub async fn run(
    store: Arc<dyn Store>,
    interval: Duration,
    dead_threshold: chrono::Duration,
) {
    loop {
        tokio::time::sleep(interval).await;
        tick(store.clone(), dead_threshold).await;
    }
}

/// Single liveness sweep over one store. Exposed for multi-project daemon loop.
pub async fn tick(store: Arc<dyn Store>, dead_threshold: chrono::Duration) {
    let now = chrono::Utc::now();
    let sessions = match store.list_active_sessions().await {
        Ok(v) => v,
        Err(_) => return,
    };
    for s in sessions {
        let alive = pid_alive(s.pid);
        if alive {
            let _ = store.touch_alive(&s.id, now).await;
        } else if now - s.last_seen_alive > dead_threshold {
            tracing::info!(session=%s.id, "session pid dead, reaping");
            let _ = store.release_locks_of(&s.id).await;
            let _ = store.end_session(&s.id, now).await;
        }
    }
}

#[allow(unsafe_code)]
fn pid_alive(pid: u32) -> bool {
    // Send signal 0: no-op, but errors on ESRCH (no such process).
    // Safe for foreign PIDs since signal 0 is "check only".
    unsafe { libc::kill(pid as i32, 0) == 0 || *libc::__error() == libc::EPERM }
}
