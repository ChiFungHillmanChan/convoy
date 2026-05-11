//! File-claim lock.

use crate::SessionId;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// A claim on a file held by one session, with a TTL.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileLock {
    /// Absolute path of the claimed file.
    pub abs_path: PathBuf,
    /// Session that holds the lock.
    pub session_id: SessionId,
    /// Why this lock was taken (free-form).
    pub reason: Option<String>,
    /// When the lock was first claimed.
    pub claimed_at: DateTime<Utc>,
    /// When the lock auto-expires.
    pub expires_at: DateTime<Utc>,
}

impl FileLock {
    /// Is this lock past its expiry as of `now`?
    pub fn is_expired(&self, now: DateTime<Utc>) -> bool {
        now >= self.expires_at
    }

    /// Return seconds until expiry; 0 if already expired.
    pub fn remaining_seconds(&self, now: DateTime<Utc>) -> i64 {
        (self.expires_at - now).num_seconds().max(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    fn make_lock(ttl: Duration) -> FileLock {
        let now = Utc::now();
        FileLock {
            abs_path: PathBuf::from("/x/y.rs"),
            session_id: SessionId::new(),
            reason: Some("test".into()),
            claimed_at: now,
            expires_at: now + ttl,
        }
    }

    #[test]
    fn fresh_lock_not_expired() {
        let l = make_lock(Duration::seconds(30));
        assert!(!l.is_expired(Utc::now()));
    }

    #[test]
    fn lock_expires_after_ttl() {
        let l = make_lock(Duration::seconds(-1));
        assert!(l.is_expired(Utc::now()));
    }

    #[test]
    fn remaining_seconds_clamps_at_zero() {
        let l = make_lock(Duration::seconds(-30));
        assert_eq!(l.remaining_seconds(Utc::now()), 0);
    }

    #[test]
    fn remaining_seconds_positive_when_fresh() {
        let l = make_lock(Duration::seconds(120));
        let r = l.remaining_seconds(Utc::now());
        assert!(r > 100 && r <= 120, "got {r}");
    }
}
