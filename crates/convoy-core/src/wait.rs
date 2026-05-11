//! Wait conditions and matching against observable state events.

use crate::{MessageKind, SessionId};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// What a session is waiting for. Tagged on the wire as `{type: "...", ...}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WaitCondition {
    /// Wait until a file lock at `abs_path` is released.
    LockReleased {
        /// Absolute file path being watched.
        abs_path: PathBuf,
    },
    /// Wait until another session ends.
    SessionEnded {
        /// Session to watch.
        session_id: SessionId,
    },
    /// Wait until a matching message arrives.
    MessageReceived {
        /// Optional sender filter.
        from: Option<SessionId>,
        /// Optional kind filter.
        kind: Option<MessageKind>,
    },
}

/// State changes the daemon can broadcast to the matcher.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event<'a> {
    /// A lock at this path was released (graceful, expiry, or session end).
    LockReleased { abs_path: &'a std::path::Path },
    /// A session ended (any reason).
    SessionEnded { session_id: &'a SessionId },
    /// A message addressed to (or broadcast for) some session was created.
    MessageCreated {
        from: &'a SessionId,
        to: Option<&'a SessionId>,
        kind: MessageKind,
    },
}

impl WaitCondition {
    /// Does `event`, observed for `waiting_session`, satisfy this condition?
    pub fn matches(&self, waiting_session: &SessionId, event: &Event<'_>) -> bool {
        match (self, event) {
            (
                WaitCondition::LockReleased { abs_path },
                Event::LockReleased { abs_path: p },
            ) => abs_path.as_path() == *p,

            (
                WaitCondition::SessionEnded { session_id },
                Event::SessionEnded { session_id: s },
            ) => session_id == *s,

            (
                WaitCondition::MessageReceived { from, kind },
                Event::MessageCreated {
                    from: ev_from,
                    to,
                    kind: ev_kind,
                },
            ) => {
                let addressed_to_us = match to {
                    Some(to_id) => *to_id == waiting_session,
                    None => true, // broadcast
                };
                let sender_ok = match from {
                    Some(f) => f == *ev_from,
                    None => true,
                };
                let kind_ok = match kind {
                    Some(k) => k == ev_kind,
                    None => true,
                };
                addressed_to_us && sender_ok && kind_ok
            }

            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sid() -> SessionId {
        SessionId::new()
    }

    #[test]
    fn lock_released_matches_same_path() {
        let me = sid();
        let cond = WaitCondition::LockReleased {
            abs_path: "/a.rs".into(),
        };
        let ev = Event::LockReleased {
            abs_path: std::path::Path::new("/a.rs"),
        };
        assert!(cond.matches(&me, &ev));
    }

    #[test]
    fn lock_released_does_not_match_different_path() {
        let me = sid();
        let cond = WaitCondition::LockReleased {
            abs_path: "/a.rs".into(),
        };
        let ev = Event::LockReleased {
            abs_path: std::path::Path::new("/b.rs"),
        };
        assert!(!cond.matches(&me, &ev));
    }

    #[test]
    fn message_received_matches_broadcast_to_anyone() {
        let me = sid();
        let other = sid();
        let cond = WaitCondition::MessageReceived {
            from: None,
            kind: None,
        };
        let ev = Event::MessageCreated {
            from: &other,
            to: None,
            kind: MessageKind::Info,
        };
        assert!(cond.matches(&me, &ev));
    }

    #[test]
    fn message_received_filters_by_sender() {
        let me = sid();
        let other = sid();
        let stranger = sid();
        let cond = WaitCondition::MessageReceived {
            from: Some(other.clone()),
            kind: None,
        };
        // Matching sender
        let ev_ok = Event::MessageCreated {
            from: &other,
            to: Some(&me),
            kind: MessageKind::Info,
        };
        let ev_bad = Event::MessageCreated {
            from: &stranger,
            to: Some(&me),
            kind: MessageKind::Info,
        };
        assert!(cond.matches(&me, &ev_ok));
        assert!(!cond.matches(&me, &ev_bad));
    }

    #[test]
    fn message_received_filters_by_kind() {
        let me = sid();
        let other = sid();
        let cond = WaitCondition::MessageReceived {
            from: None,
            kind: Some(MessageKind::Ack),
        };
        let ev_ack = Event::MessageCreated {
            from: &other,
            to: Some(&me),
            kind: MessageKind::Ack,
        };
        let ev_info = Event::MessageCreated {
            from: &other,
            to: Some(&me),
            kind: MessageKind::Info,
        };
        assert!(cond.matches(&me, &ev_ack));
        assert!(!cond.matches(&me, &ev_info));
    }

    #[test]
    fn message_to_someone_else_is_not_for_us() {
        let me = sid();
        let other = sid();
        let cond = WaitCondition::MessageReceived {
            from: None,
            kind: None,
        };
        let ev = Event::MessageCreated {
            from: &other,
            to: Some(&other), // not us
            kind: MessageKind::Info,
        };
        assert!(!cond.matches(&me, &ev));
    }

    #[test]
    fn session_ended_matches_correct_id() {
        let me = sid();
        let other = sid();
        let cond = WaitCondition::SessionEnded {
            session_id: other.clone(),
        };
        let ev = Event::SessionEnded { session_id: &other };
        assert!(cond.matches(&me, &ev));
    }
}
