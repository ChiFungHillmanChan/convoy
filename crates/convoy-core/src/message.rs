//! Messages exchanged between sessions.

use crate::SessionId;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

/// Stable per-message UUID.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MessageId(String);

impl MessageId {
    /// Fresh random id.
    pub fn new() -> Self {
        Self(Uuid::new_v4().to_string())
    }
    /// Hex view.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for MessageId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for MessageId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Allowed kinds of messages. Must match spec §6.2 and DB CHECK constraint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MessageKind {
    /// Notification, no reply expected.
    Info,
    /// Asks something of recipient.
    Question,
    /// Acknowledgement / reply.
    Ack,
    /// Requests recipient to pause / release something.
    RequestYield,
    /// Automatic notice emitted by `claim_file` (not user-callable).
    ClaimNotice,
    /// Public progress update.
    StatusBroadcast,
}

impl MessageKind {
    /// Stable lowercase tag used in DB and wire formats.
    pub fn as_tag(&self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Question => "question",
            Self::Ack => "ack",
            Self::RequestYield => "request-yield",
            Self::ClaimNotice => "claim-notice",
            Self::StatusBroadcast => "status-broadcast",
        }
    }

    /// Parse from the tag string.
    pub fn from_tag(s: &str) -> Option<Self> {
        Some(match s {
            "info" => Self::Info,
            "question" => Self::Question,
            "ack" => Self::Ack,
            "request-yield" => Self::RequestYield,
            "claim-notice" => Self::ClaimNotice,
            "status-broadcast" => Self::StatusBroadcast,
            _ => return None,
        })
    }

    /// Kinds the model is allowed to send directly.
    pub fn is_user_sendable(&self) -> bool {
        !matches!(self, Self::ClaimNotice)
    }
}

/// A message between sessions. `to` is `None` for broadcasts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    /// Unique id.
    pub id: MessageId,
    /// Sending session.
    pub from: SessionId,
    /// Receiving session, or `None` for broadcast.
    pub to: Option<SessionId>,
    /// Kind.
    pub kind: MessageKind,
    /// Optional thread anchor.
    pub in_reply_to: Option<MessageId>,
    /// Body text.
    pub body: String,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
    /// When recipient marked it read.
    pub read_at: Option<DateTime<Utc>>,
}

impl Message {
    /// Has the recipient read this yet?
    pub fn is_unread(&self) -> bool {
        self.read_at.is_none()
    }
    /// True for broadcasts (to == None).
    pub fn is_broadcast(&self) -> bool {
        self.to.is_none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_tag_roundtrips() {
        for k in [
            MessageKind::Info,
            MessageKind::Question,
            MessageKind::Ack,
            MessageKind::RequestYield,
            MessageKind::ClaimNotice,
            MessageKind::StatusBroadcast,
        ] {
            assert_eq!(MessageKind::from_tag(k.as_tag()), Some(k));
        }
    }

    #[test]
    fn unknown_tag_returns_none() {
        assert_eq!(MessageKind::from_tag("nope"), None);
    }

    #[test]
    fn claim_notice_is_not_user_sendable() {
        assert!(!MessageKind::ClaimNotice.is_user_sendable());
        assert!(MessageKind::Info.is_user_sendable());
    }

    #[test]
    fn broadcast_when_to_is_none() {
        let m = Message {
            id: MessageId::new(),
            from: SessionId::new(),
            to: None,
            kind: MessageKind::Info,
            in_reply_to: None,
            body: "hi".into(),
            created_at: Utc::now(),
            read_at: None,
        };
        assert!(m.is_broadcast());
        assert!(m.is_unread());
    }
}
