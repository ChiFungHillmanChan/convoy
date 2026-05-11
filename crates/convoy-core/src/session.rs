//! Session record and derived status.

use crate::{Nickname, SessionId};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Which AI tool runs this session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Agent {
    /// Claude Code CLI (M1).
    ClaudeCode,
    /// Codex CLI (M2).
    Codex,
    /// Gemini CLI (M2).
    Gemini,
}

impl Agent {
    /// Wire/DB tag.
    pub fn as_tag(&self) -> &'static str {
        match self {
            Agent::ClaudeCode => "claude-code",
            Agent::Codex => "codex",
            Agent::Gemini => "gemini",
        }
    }

    /// Parse wire/DB tag.
    pub fn from_tag(s: &str) -> Option<Self> {
        Some(match s {
            "claude-code" => Self::ClaudeCode,
            "codex" => Self::Codex,
            "gemini" => Self::Gemini,
            _ => return None,
        })
    }
}

/// Liveness classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LivenessStatus {
    /// Heartbeat within `active_threshold`.
    Active,
    /// Process alive but no recent heartbeat.
    Idle,
    /// Process gone (PID probe failed).
    Dead,
    /// `ended_at` is set.
    Ended,
}

/// A session record (matches `sessions` DB row).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Session {
    /// Stable id.
    pub id: SessionId,
    /// Which tool.
    pub agent: Agent,
    /// OS process id.
    pub pid: u32,
    /// Git branch, if any.
    pub branch: Option<String>,
    /// Worktree absolute path.
    pub worktree_path: Option<PathBuf>,
    /// Display nickname.
    pub nickname: Nickname,
    /// First registration.
    pub started_at: DateTime<Utc>,
    /// Last hook-driven activity.
    pub last_heartbeat: DateTime<Utc>,
    /// Last daemon-driven liveness probe success.
    pub last_seen_alive: DateTime<Utc>,
    /// Set when the session ends; None while alive.
    pub ended_at: Option<DateTime<Utc>>,
}

impl Session {
    /// Compute liveness classification as of `now` given thresholds.
    pub fn liveness(&self, now: DateTime<Utc>, active_threshold: Duration) -> LivenessStatus {
        if self.ended_at.is_some() {
            return LivenessStatus::Ended;
        }
        let since_heartbeat = now - self.last_heartbeat;
        let since_alive = now - self.last_seen_alive;
        // If we have not seen the process alive for >5min and no heartbeat
        // for >5min, classify as Dead (daemon should soon mark ended).
        if since_alive > Duration::minutes(5) && since_heartbeat > Duration::minutes(5) {
            return LivenessStatus::Dead;
        }
        if since_heartbeat <= active_threshold {
            LivenessStatus::Active
        } else {
            LivenessStatus::Idle
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh_session(now: DateTime<Utc>) -> Session {
        Session {
            id: SessionId::new(),
            agent: Agent::ClaudeCode,
            pid: 1,
            branch: None,
            worktree_path: None,
            nickname: Nickname::new("test").unwrap(),
            started_at: now,
            last_heartbeat: now,
            last_seen_alive: now,
            ended_at: None,
        }
    }

    #[test]
    fn agent_tag_roundtrip() {
        for a in [Agent::ClaudeCode, Agent::Codex, Agent::Gemini] {
            assert_eq!(Agent::from_tag(a.as_tag()), Some(a));
        }
    }

    #[test]
    fn fresh_session_is_active() {
        let now = Utc::now();
        let s = fresh_session(now);
        assert_eq!(s.liveness(now, Duration::seconds(60)), LivenessStatus::Active);
    }

    #[test]
    fn idle_after_heartbeat_gap() {
        let now = Utc::now();
        let mut s = fresh_session(now);
        s.last_heartbeat = now - Duration::minutes(2);
        // last_seen_alive recent (daemon probe still working)
        assert_eq!(s.liveness(now, Duration::seconds(60)), LivenessStatus::Idle);
    }

    #[test]
    fn dead_after_long_silence() {
        let now = Utc::now();
        let mut s = fresh_session(now);
        s.last_heartbeat = now - Duration::minutes(10);
        s.last_seen_alive = now - Duration::minutes(10);
        assert_eq!(s.liveness(now, Duration::seconds(60)), LivenessStatus::Dead);
    }

    #[test]
    fn ended_overrides_other_states() {
        let now = Utc::now();
        let mut s = fresh_session(now);
        s.ended_at = Some(now);
        assert_eq!(s.liveness(now, Duration::seconds(60)), LivenessStatus::Ended);
    }
}
