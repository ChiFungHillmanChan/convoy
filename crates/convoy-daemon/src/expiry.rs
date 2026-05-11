//! Periodically releases locks past TTL and times out expired waits.

use convoy_store::Store;
use std::sync::Arc;
use std::time::Duration;

pub async fn run(store: Arc<dyn Store>, interval: Duration, notify: crate::notify::Notifier) {
    loop {
        tokio::time::sleep(interval).await;
        let now = chrono::Utc::now();
        if let Ok(dropped) = store.release_expired_locks(now).await {
            for lock in dropped {
                notify.lock_released(&lock).await;
            }
        }
        let _ = store.timeout_waits(now).await;
    }
}
