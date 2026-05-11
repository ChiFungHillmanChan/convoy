//! Daemon RPC protocol. JSON-encoded, one line per message.

use convoy_core::{FileLock, MessageKind, Nickname, ProjectId, SessionId, WaitCondition};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Wire wrapper that routes an operation to a specific project's store.
/// Callers serialize this on the wire; the daemon deserializes it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Envelope {
    /// Project this operation targets.
    pub project_id: ProjectId,
    /// The actual operation.
    pub op: Request,
}

/// All requests from MCP / hook / CLI to daemon.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Request {
    RegisterSession {
        id: SessionId,
        agent_tag: String,
        pid: u32,
        nickname: String,
        branch: Option<String>,
        worktree_path: Option<PathBuf>,
    },
    Heartbeat { id: SessionId },
    EndSession { id: SessionId },
    SetBranch { id: SessionId, branch: Option<String> },
    Rename { id: SessionId, new: String },

    ListSessions { include_ended: bool },
    UpdateStatus { id: SessionId, summary: String },

    SendMessage {
        from: SessionId,
        to: Option<SessionId>,
        kind: MessageKind,
        in_reply_to: Option<String>,
        body: String,
    },
    ReadInbox { id: SessionId, unread_only: bool, limit: usize },
    MarkRead { ids: Vec<String> },

    ClaimFile {
        session: SessionId,
        abs_path: PathBuf,
        reason: Option<String>,
        ttl_sec: i64,
    },
    ReleaseFile { session: SessionId, abs_path: PathBuf },
    ListLocks,
    ListMyClaims { session: SessionId },

    WaitFor {
        session: SessionId,
        condition: WaitCondition,
        timeout_sec: i64,
        hint: Option<String>,
    },
    CancelWait { wait_id: String },

    Shutdown,
    Ping,
}

/// All responses from daemon to caller.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Response {
    Ok,
    Pong,
    Error { message: String },
    Sessions { sessions: Vec<serde_json::Value> },
    Inbox { messages: Vec<serde_json::Value> },
    MessageCreated { id: String },
    ClaimResult {
        claimed: bool,
        held_by: Option<SessionId>,
        held_until: Option<i64>,
        expires_at: Option<i64>,
    },
    Locks { locks: Vec<FileLock> },
    WaitCreated { wait_id: String, status: String },
    MarkRead { marked: usize },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_roundtrips() {
        let r = Request::Ping;
        let s = serde_json::to_string(&r).unwrap();
        let back: Request = serde_json::from_str(&s).unwrap();
        assert!(matches!(back, Request::Ping));
    }

    #[test]
    fn response_roundtrips() {
        let r = Response::Pong;
        let s = serde_json::to_string(&r).unwrap();
        assert!(s.contains("pong"));
    }
}
