//! `convoy session` subcommand group — wraps every daemon RPC for Bash callers.

use clap::{Args, Subcommand};
use convoy_core::{MessageKind, SessionId, WaitCondition};
use convoy_daemon::{DaemonClient, Request};
use std::path::PathBuf;

/// Options shared by all session subcommands.
#[derive(Args, Debug)]
pub struct SessionArgs {
    /// Session id override (default: $CLAUDE_SESSION_ID)
    #[arg(long, global = true)]
    session: Option<String>,

    #[command(subcommand)]
    command: SessionCmd,
}

#[derive(Subcommand, Debug)]
enum SessionCmd {
    /// Send a message to another session or broadcast
    Send {
        /// Target session id, or omit for broadcast
        #[arg(long)]
        to: Option<String>,
        /// Message kind (e.g. status-update, file-claim-notice)
        #[arg(long, default_value = "status-update")]
        kind: String,
        /// Message body
        body: String,
    },
    /// Claim a file lock
    Claim {
        abs_path: PathBuf,
        #[arg(long)]
        reason: Option<String>,
        #[arg(long, default_value = "300")]
        ttl: i64,
    },
    /// Release a file lock
    Release {
        abs_path: PathBuf,
    },
    /// Show inbox
    Inbox {
        #[arg(long)]
        unread_only: bool,
        #[arg(long, default_value = "20")]
        limit: usize,
    },
    /// Mark messages as read
    MarkRead {
        ids: Vec<String>,
    },
    /// Push a status update
    Status {
        summary: String,
    },
    /// List all active sessions
    ListSessions,
    /// List own file claims
    Locks,
    /// Register a wait condition
    Wait {
        #[arg(long)]
        condition: String,
        #[arg(long, default_value = "300")]
        timeout: i64,
    },
    /// Cancel a pending wait
    CancelWait {
        wait_id: String,
    },
    /// Rename this session
    Rename {
        new_nickname: String,
    },
}

fn resolve_session(override_id: Option<String>) -> anyhow::Result<SessionId> {
    let raw = override_id
        .or_else(|| std::env::var("CLAUDE_SESSION_ID").ok())
        .ok_or_else(|| anyhow::anyhow!("session id required: set CLAUDE_SESSION_ID or pass --session"))?;
    Ok(SessionId::from_string_unchecked(raw))
}

fn default_socket() -> PathBuf {
    PathBuf::from(format!(
        "{}/.convoy/daemon.sock",
        std::env::var("HOME").unwrap_or_else(|_| "/tmp".into())
    ))
}

pub async fn run(args: SessionArgs) -> anyhow::Result<()> {
    let session_id = resolve_session(args.session)?;
    let client = DaemonClient::connect(&default_socket()).await?;

    let resp = match args.command {
        SessionCmd::Send { to, kind, body } => {
            let to_id = to.map(SessionId::from_string_unchecked);
            let msg_kind = MessageKind::from_tag(&kind).unwrap_or(MessageKind::Info);
            client.call(Request::SendMessage {
                from: session_id,
                to: to_id,
                kind: msg_kind,
                in_reply_to: None,
                body,
            }).await?
        }
        SessionCmd::Claim { abs_path, reason, ttl } => {
            client.call(Request::ClaimFile {
                session: session_id,
                abs_path,
                reason,
                ttl_sec: ttl,
            }).await?
        }
        SessionCmd::Release { abs_path } => {
            client.call(Request::ReleaseFile {
                session: session_id,
                abs_path,
            }).await?
        }
        SessionCmd::Inbox { unread_only, limit } => {
            client.call(Request::ReadInbox {
                id: session_id,
                unread_only,
                limit,
            }).await?
        }
        SessionCmd::MarkRead { ids } => {
            client.call(Request::MarkRead { ids }).await?
        }
        SessionCmd::Status { summary } => {
            client.call(Request::UpdateStatus {
                id: session_id,
                summary,
            }).await?
        }
        SessionCmd::ListSessions => {
            client.call(Request::ListSessions { include_ended: false }).await?
        }
        SessionCmd::Locks => {
            client.call(Request::ListMyClaims { session: session_id }).await?
        }
        SessionCmd::Wait { condition, timeout } => {
            let cond: WaitCondition = serde_json::from_str(&condition)?;
            client.call(Request::WaitFor {
                session: session_id,
                condition: cond,
                timeout_sec: timeout,
                hint: None,
            }).await?
        }
        SessionCmd::CancelWait { wait_id } => {
            client.call(Request::CancelWait { wait_id }).await?
        }
        SessionCmd::Rename { new_nickname } => {
            client.call(Request::Rename {
                id: session_id,
                new: new_nickname,
            }).await?
        }
    };

    println!("{}", serde_json::to_string_pretty(&resp)?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    /// Minimal top-level parser used for parser tests only.
    #[derive(Parser, Debug)]
    struct TestCli {
        #[command(subcommand)]
        cmd: super::SessionCmd,
    }

    #[test]
    fn parse_send_subcommand() {
        let cli = TestCli::try_parse_from(["cli", "send", "--to", "abc123", "hello world"]).unwrap();
        assert!(matches!(cli.cmd, super::SessionCmd::Send { .. }));
    }

    #[test]
    fn parse_inbox_subcommand() {
        let cli = TestCli::try_parse_from(["cli", "inbox", "--unread-only"]).unwrap();
        assert!(matches!(cli.cmd, super::SessionCmd::Inbox { unread_only: true, .. }));
    }

    #[test]
    fn parse_list_sessions() {
        let cli = TestCli::try_parse_from(["cli", "list-sessions"]).unwrap();
        assert!(matches!(cli.cmd, super::SessionCmd::ListSessions));
    }
}
