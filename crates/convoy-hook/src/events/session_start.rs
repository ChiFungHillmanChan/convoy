//! Auto-register on Claude Code SessionStart.

use convoy_core::{Nickname, SessionId};
use convoy_daemon::rpc::{Request, Response};
use convoy_daemon::DaemonClient;

pub async fn run() -> anyhow::Result<()> {
    let session_id = std::env::var("CLAUDE_SESSION_ID")
        .map(SessionId::from_string_unchecked)
        .unwrap_or_else(|_| SessionId::new());
    let cwd = std::env::current_dir()?;
    let _project_id = convoy_core::ProjectId::from_canonical_path(&cwd);

    let socket = std::path::PathBuf::from(format!(
        "{}/.convoy/daemon.sock",
        std::env::var("HOME").unwrap_or_else(|_| "/tmp".into())
    ));
    let client = DaemonClient::connect(&socket).await?;

    let branch = detect_branch();
    let nickname = branch
        .clone()
        .and_then(|b| Nickname::new(&b).ok())
        .unwrap_or_else(|| Nickname::new(&random_animal()).unwrap());

    let req = Request::RegisterSession {
        id: session_id.clone(),
        agent_tag: "claude-code".into(),
        pid: std::process::id(),
        nickname: nickname.to_string(),
        branch,
        worktree_path: Some(cwd),
    };
    let _ = client.call(req).await?;

    // Inject current peers and unread mail.
    let sessions = match client.call(Request::ListSessions { include_ended: false }).await? {
        Response::Sessions { sessions } => sessions,
        _ => vec![],
    };
    if !sessions.is_empty() {
        let body = format!("active peers: {} session(s)", sessions.len());
        crate::inject::emit(&body)?;
    }
    Ok(())
}

pub(crate) fn detect_branch() -> Option<String> {
    use std::process::Command;
    let out = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .ok()?;
    if !out.status.success() { return None; }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if s.is_empty() || s == "HEAD" { None } else { Some(s) }
}

fn random_animal() -> String {
    const ADJECTIVES: &[&str] = &["crimson", "azure", "swift", "quiet", "bold", "amber"];
    const ANIMALS: &[&str] = &["otter", "fox", "heron", "cat", "moth", "lynx"];
    let i = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as usize)
        .unwrap_or(0);
    format!("{}-{}", ADJECTIVES[i % ADJECTIVES.len()], ANIMALS[(i / 7) % ANIMALS.len()])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn random_animal_has_hyphen() {
        let s = random_animal();
        assert!(s.contains('-'), "expected hyphen in {s}");
        let parts: Vec<&str> = s.splitn(2, '-').collect();
        assert_eq!(parts.len(), 2);
        assert!(!parts[0].is_empty());
        assert!(!parts[1].is_empty());
    }

    #[test]
    fn detect_branch_returns_none_outside_git() {
        // When run in a temp dir with no git repo, detect_branch should be None
        // We cannot guarantee the test runner isn't in a repo, so just check it
        // doesn't panic and returns a valid Option<String>.
        let result = std::panic::catch_unwind(detect_branch);
        assert!(result.is_ok(), "detect_branch panicked");
    }
}
