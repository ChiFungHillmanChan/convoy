use convoy_daemon::rpc::Request;
use convoy_daemon::DaemonClient;

pub async fn run() -> anyhow::Result<()> {
    let session_id = crate::read_session_id()?;
    let payload: serde_json::Value = serde_json::from_reader(std::io::stdin())?;

    let client = DaemonClient::connect(&crate::default_socket()).await?;
    let _ = client.call(Request::Heartbeat { id: session_id.clone() }).await?;

    let tool = payload.get("tool_name").and_then(|v| v.as_str()).unwrap_or("");
    if tool == "Bash" {
        if let Some(cmd) = payload.pointer("/tool_input/command").and_then(|v| v.as_str()) {
            let re = regex::Regex::new(r"git\s+(checkout|switch|worktree)").unwrap();
            if re.is_match(cmd) {
                let branch = super::session_start::detect_branch();
                let _ = client.call(Request::SetBranch { id: session_id, branch }).await?;
            }
        }
    }
    Ok(())
}
