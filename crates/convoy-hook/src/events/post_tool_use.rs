use convoy_core::project_id_from_cwd;
use convoy_daemon::rpc::Request;
use convoy_daemon::DaemonClient;
use std::sync::OnceLock;

static GIT_BRANCH_RE: OnceLock<regex::Regex> = OnceLock::new();

fn git_branch_regex() -> &'static regex::Regex {
    GIT_BRANCH_RE.get_or_init(|| {
        regex::Regex::new(r"git\s+(checkout|switch|worktree)").expect("valid regex")
    })
}

pub async fn run() -> anyhow::Result<()> {
    let session_id = crate::read_session_id()?;
    let payload: serde_json::Value = serde_json::from_reader(std::io::stdin())?;
    let project_id = project_id_from_cwd()?;

    let client = DaemonClient::connect(&crate::default_socket()).await?;
    let _ = client.call_project(project_id.clone(), Request::Heartbeat { id: session_id.clone() }).await?;

    let tool = payload.get("tool_name").and_then(|v| v.as_str()).unwrap_or("");
    if tool == "Bash" {
        if let Some(cmd) = payload.pointer("/tool_input/command").and_then(|v| v.as_str()) {
            if git_branch_regex().is_match(cmd) {
                let branch = super::session_start::detect_branch();
                let _ = client.call_project(project_id, Request::SetBranch { id: session_id, branch }).await?;
            }
        }
    }
    Ok(())
}
