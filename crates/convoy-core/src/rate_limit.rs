//! Rolling-window rate limiter.

use chrono::{DateTime, Duration, Utc};
use std::collections::VecDeque;

/// Tracks event timestamps within a fixed rolling window.
/// Decision: "is the next event allowed at `now`?"
#[derive(Debug, Clone)]
pub struct RateLimiter {
    window: Duration,
    capacity: usize,
    events: VecDeque<DateTime<Utc>>,
}

impl RateLimiter {
    /// New limiter with `capacity` events allowed per `window`.
    pub fn new(capacity: usize, window: Duration) -> Self {
        Self {
            window,
            capacity,
            events: VecDeque::new(),
        }
    }

    /// Prune entries older than `now - window`. Public for tests.
    pub fn prune(&mut self, now: DateTime<Utc>) {
        let cutoff = now - self.window;
        while let Some(&front) = self.events.front() {
            if front < cutoff {
                self.events.pop_front();
            } else {
                break;
            }
        }
    }

    /// Try to record an event. Returns true on success, false if over capacity.
    pub fn try_record(&mut self, now: DateTime<Utc>) -> bool {
        self.prune(now);
        if self.events.len() >= self.capacity {
            false
        } else {
            self.events.push_back(now);
            true
        }
    }

    /// Current count within window.
    pub fn count(&self) -> usize {
        self.events.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_up_to_capacity() {
        let mut rl = RateLimiter::new(3, Duration::minutes(1));
        let t = Utc::now();
        assert!(rl.try_record(t));
        assert!(rl.try_record(t));
        assert!(rl.try_record(t));
        assert!(!rl.try_record(t)); // 4th in same instant rejected
        assert_eq!(rl.count(), 3);
    }

    #[test]
    fn rolls_off_old_events() {
        let mut rl = RateLimiter::new(2, Duration::seconds(60));
        let t0 = Utc::now();
        assert!(rl.try_record(t0));
        assert!(rl.try_record(t0));
        let t1 = t0 + Duration::seconds(30);
        assert!(!rl.try_record(t1)); // still in window
        let t2 = t0 + Duration::seconds(61);
        assert!(rl.try_record(t2)); // old ones pruned
        assert_eq!(rl.count(), 1);
    }

    #[test]
    fn zero_capacity_rejects_everything() {
        let mut rl = RateLimiter::new(0, Duration::seconds(60));
        assert!(!rl.try_record(Utc::now()));
    }
}
